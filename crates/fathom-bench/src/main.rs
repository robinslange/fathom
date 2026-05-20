//! Fathom retrieval benchmark.
//!
//! Loads the full corpus (549 shards, ~263MB) into a cache-extended Runtime,
//! runs a fixed query set defined in `data/queries.json`, computes recall
//! metrics on iconic-phrase queries, similarity ranges on topical queries,
//! and noise-floor checks on adversarial queries. Writes a timestamped JSON
//! to `results/` so v0.21 BM25 hybrid retrieval can do a side-by-side.
//!
//! Run: `FATHOM_BGE_MODEL_DIR=/tmp/fathom-bge-models cargo run -p fathom-bench --release`
//!
//! Optional flags:
//!   --limit-books N      load only first N books (faster smoke; recall numbers won't be comparable)
//!   --top-k K            ranking depth scored (default 50)
//!   --tag NAME           label this run, e.g. v0.20-baseline or v0.21-bm25-rrf
//!   --queries PATH       override the queries.json path
//!   --output-dir PATH    override results directory

use anyhow::{Context, Result};
use clap::Parser;
use fathom_core::runtime::{Runtime, SearchHit};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum FusionModeArg {
    Rrf,
    Linear,
    DenseOnly,
    Bm25Only,
}

#[derive(Parser)]
struct Args {
    /// Limit to the first N books (for smoke testing — recall numbers won't be comparable across runs).
    #[arg(long)]
    limit_books: Option<usize>,
    /// Top-K depth scored for each query.
    #[arg(long, default_value_t = 50)]
    top_k: usize,
    /// Label for this run; ends up in the output filename and the JSON top-level.
    #[arg(long, default_value = "v0.20-baseline")]
    tag: String,
    /// Override the queries.json path.
    #[arg(long)]
    queries: Option<PathBuf>,
    /// Override the results output directory.
    #[arg(long)]
    output_dir: Option<PathBuf>,
    /// Fusion mode for the hybrid retrieval lane.
    #[arg(long, value_enum, default_value_t = FusionModeArg::Rrf)]
    fusion_mode: FusionModeArg,
    /// Alpha for linear convex combination (only used when --fusion-mode=linear).
    #[arg(long, default_value_t = 0.5)]
    fusion_alpha: f32,
    /// k for RRF (only used when --fusion-mode=rrf).
    #[arg(long, default_value_t = 10)]
    rrf_k: u32,
}

#[derive(Debug, Deserialize)]
struct QuerySet {
    version: u32,
    notes: String,
    iconic_phrase: Vec<IconicQuery>,
    topical: Vec<TopicalQuery>,
    adversarial: Vec<AdversarialQuery>,
}

#[derive(Debug, Deserialize)]
struct IconicQuery {
    query: String,
    expected_gutenberg_ids: Vec<u32>,
    source: String,
    /// Free-text annotation in queries.json; surfaced into the result JSON
    /// only when present so reviewers can see why a query was picked.
    #[serde(default)]
    #[allow(dead_code)]
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TopicalQuery {
    query: String,
    #[allow(dead_code)]
    expected_traditions: Vec<String>,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct AdversarialQuery {
    query: String,
    notes: String,
}

#[derive(Debug, Serialize)]
struct BenchResults {
    tag: String,
    generated: String,
    manifest_build_id: String,
    embed_model_id: String,
    books_loaded: usize,
    top_k: usize,
    queries_version: u32,
    queries_notes: String,
    iconic: Vec<IconicResult>,
    topical: Vec<TopicalResult>,
    adversarial: Vec<AdversarialResult>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct HitRecord {
    rank: usize,
    gutenberg_id: u32,
    chunk_id: String,
    similarity: f32,
    title: String,
    excerpt: String,
}

#[derive(Debug, Serialize)]
struct IconicResult {
    query: String,
    source: String,
    expected_gutenberg_ids: Vec<u32>,
    /// Position (1-indexed) of the FIRST expected book in the ranking, or
    /// None if no expected book appears in top-K. None for empty
    /// expected_gutenberg_ids (e.g. queries not in corpus).
    expected_rank: Option<usize>,
    /// Best similarity score seen for any expected book in top-K.
    expected_best_similarity: Option<f32>,
    /// Reciprocal rank for MRR aggregation: 1/expected_rank or 0.
    reciprocal_rank: f32,
    top_hits: Vec<HitRecord>,
}

#[derive(Debug, Serialize)]
struct TopicalResult {
    query: String,
    notes: String,
    top_hits: Vec<HitRecord>,
    /// Span between max and min similarity across top-K. Wider span = the
    /// ranking is doing meaningful discrimination; narrow = all hits equally
    /// (un)related.
    similarity_span: f32,
}

#[derive(Debug, Serialize)]
struct AdversarialResult {
    query: String,
    notes: String,
    top_similarity: f32,
    /// Top-K hits trimmed to 5 — full list is noise.
    top_hits: Vec<HitRecord>,
}

#[derive(Debug, Serialize)]
struct Summary {
    iconic_count: usize,
    iconic_with_expected_books: usize,
    /// Out of iconic queries that HAD an expected book in the corpus, how
    /// many surfaced ANY expected book in top-K?
    iconic_recall_at_k: f32,
    /// Mean reciprocal rank over iconic queries that had expected books.
    iconic_mrr: f32,
    /// Per-rank-cutoff recall: hits@1, hits@5, hits@10, hits@top_k.
    iconic_hits_at_1: f32,
    iconic_hits_at_5: f32,
    iconic_hits_at_10: f32,
    /// Adversarial similarity ceiling: max top-similarity across all
    /// adversarial queries. v0.21 hybrid retrieval should NOT raise this.
    adversarial_max_similarity: f32,
    /// Total wall time of the search pass (excludes load).
    search_pass_seconds: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.fusion_mode {
        FusionModeArg::Rrf => std::env::set_var("FATHOM_FUSION_MODE", "rrf"),
        FusionModeArg::Linear => std::env::set_var("FATHOM_FUSION_MODE", "linear"),
        FusionModeArg::DenseOnly => std::env::set_var("FATHOM_FUSION_MODE", "dense_only"),
        FusionModeArg::Bm25Only => std::env::set_var("FATHOM_FUSION_MODE", "bm25_only"),
    }
    std::env::set_var("FATHOM_FUSION_ALPHA", args.fusion_alpha.to_string());
    std::env::set_var("FATHOM_RRF_K", args.rrf_k.to_string());

    let queries_path = args
        .queries
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/queries.json"));
    let out_dir = args
        .output_dir
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results"));
    std::fs::create_dir_all(&out_dir).with_context(|| format!("create {}", out_dir.display()))?;

    let queries_json = std::fs::read_to_string(&queries_path)
        .with_context(|| format!("read {}", queries_path.display()))?;
    let queries: QuerySet = serde_json::from_str(&queries_json).context("parse queries.json")?;
    eprintln!(
        "loaded {} iconic + {} topical + {} adversarial queries from {}",
        queries.iconic_phrase.len(),
        queries.topical.len(),
        queries.adversarial.len(),
        queries_path.display()
    );

    eprintln!("[1/5] fetching manifest");
    let manifest = fathom_core::runtime::fetch_manifest()
        .await
        .context("fetch_manifest")?;
    let build_id = manifest.build_id.clone();
    let embed_model_id = manifest.embed_model_id.clone();
    let total_books = manifest.book_count;
    eprintln!(
        "  manifest: {} books, build_id={}, embed_model={}",
        total_books, build_id, embed_model_id
    );

    let load_n = args.limit_books.unwrap_or(total_books).min(total_books);
    // Cache must hold every loaded book; +10 head-room for any incidental loads.
    let runtime = std::sync::Arc::new(Runtime::with_cache_capacity(manifest, load_n + 10));

    eprintln!("[2/5] init bge-small embedder");
    let model_dir = std::env::var("FATHOM_BGE_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/fathom-bge-models"));
    fathom_embed::init_embedder(
        &model_dir.join("bge-small.onnx"),
        &model_dir.join("tokenizer.json"),
    )?;
    eprintln!("  ok: embedder loaded from {}", model_dir.display());

    eprintln!("[3/5] loading {} shards into the cache", load_n);
    let load_start = Instant::now();
    let ids_to_load: Vec<u32> = runtime
        .manifest()
        .books
        .iter()
        .take(load_n)
        .map(|b| b.gutenberg_id)
        .collect();
    for (i, gid) in ids_to_load.iter().enumerate() {
        runtime.ensure_shard(*gid).await?;
        if (i + 1) % 50 == 0 || i + 1 == ids_to_load.len() {
            eprintln!(
                "  ...{}/{}  ({:.1}s elapsed)",
                i + 1,
                ids_to_load.len(),
                load_start.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "  loaded {} shards in {:.1}s",
        load_n,
        load_start.elapsed().as_secs_f64()
    );

    eprintln!("[4/5] running queries (top_k={})", args.top_k);
    let search_start = Instant::now();

    let mut iconic_results = Vec::with_capacity(queries.iconic_phrase.len());
    for q in &queries.iconic_phrase {
        let hits = if q.query.is_empty() {
            Vec::new()
        } else {
            runtime.search(&q.query, args.top_k).await?
        };
        let (expected_rank, expected_best_similarity, reciprocal_rank) =
            score_iconic(&hits, &q.expected_gutenberg_ids);
        iconic_results.push(IconicResult {
            query: q.query.clone(),
            source: q.source.clone(),
            expected_gutenberg_ids: q.expected_gutenberg_ids.clone(),
            expected_rank,
            expected_best_similarity,
            reciprocal_rank,
            top_hits: hits_to_records(&hits, &runtime, 10),
        });
    }

    let mut topical_results = Vec::with_capacity(queries.topical.len());
    for q in &queries.topical {
        let hits = runtime.search(&q.query, args.top_k).await?;
        let span = if hits.is_empty() {
            0.0
        } else {
            hits.first().map(|h| h.similarity).unwrap_or(0.0)
                - hits.last().map(|h| h.similarity).unwrap_or(0.0)
        };
        topical_results.push(TopicalResult {
            query: q.query.clone(),
            notes: q.notes.clone(),
            top_hits: hits_to_records(&hits, &runtime, 10),
            similarity_span: span,
        });
    }

    let mut adversarial_results = Vec::with_capacity(queries.adversarial.len());
    for q in &queries.adversarial {
        let hits = if q.query.is_empty() {
            Vec::new()
        } else {
            // Empty-query path errors at fathom_embed::embed; the rest are valid.
            match runtime.search(&q.query, args.top_k).await {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("  adversarial query {q:?} errored (expected for some): {e:#}");
                    Vec::new()
                }
            }
        };
        let top_sim = hits.first().map(|h| h.similarity).unwrap_or(0.0);
        adversarial_results.push(AdversarialResult {
            query: q.query.clone(),
            notes: q.notes.clone(),
            top_similarity: top_sim,
            top_hits: hits_to_records(&hits, &runtime, 5),
        });
    }

    let search_seconds = search_start.elapsed().as_secs_f64();

    let summary = summarise(&iconic_results, &adversarial_results, search_seconds);

    eprintln!("[5/5] writing results");
    let results = BenchResults {
        tag: args.tag.clone(),
        generated: chrono::Utc::now().to_rfc3339(),
        manifest_build_id: build_id,
        embed_model_id,
        books_loaded: load_n,
        top_k: args.top_k,
        queries_version: queries.version,
        queries_notes: queries.notes,
        iconic: iconic_results,
        topical: topical_results,
        adversarial: adversarial_results,
        summary,
    };

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let out_path = out_dir.join(format!("{}-{}.json", stamp, args.tag));
    let pretty = serde_json::to_string_pretty(&results)?;
    std::fs::write(&out_path, &pretty).with_context(|| format!("write {}", out_path.display()))?;
    eprintln!("  → {}", out_path.display());

    print_summary(&results);
    Ok(())
}

fn score_iconic(hits: &[SearchHit], expected: &[u32]) -> (Option<usize>, Option<f32>, f32) {
    if expected.is_empty() {
        return (None, None, 0.0);
    }
    let mut best_rank: Option<usize> = None;
    let mut best_sim: Option<f32> = None;
    for (rank, h) in hits.iter().enumerate() {
        if expected.contains(&h.gutenberg_id) {
            let one_indexed = rank + 1;
            if best_rank.is_none_or(|r| one_indexed < r) {
                best_rank = Some(one_indexed);
            }
            if best_sim.is_none_or(|s| h.similarity > s) {
                best_sim = Some(h.similarity);
            }
        }
    }
    let rr = best_rank.map_or(0.0, |r| 1.0 / (r as f32));
    (best_rank, best_sim, rr)
}

fn hits_to_records(hits: &[SearchHit], runtime: &Runtime, limit: usize) -> Vec<HitRecord> {
    hits.iter()
        .take(limit)
        .enumerate()
        .map(|(i, h)| {
            let title = runtime
                .book(h.gutenberg_id)
                .map(|b| b.title.lines().next().unwrap_or("").to_string())
                .unwrap_or_default();
            HitRecord {
                rank: i + 1,
                gutenberg_id: h.gutenberg_id,
                chunk_id: h.chunk_id.clone(),
                similarity: h.similarity,
                title,
                excerpt: h
                    .excerpt
                    .chars()
                    .take(200)
                    .collect::<String>()
                    .replace('\n', " "),
            }
        })
        .collect()
}

fn summarise(
    iconic: &[IconicResult],
    adversarial: &[AdversarialResult],
    search_seconds: f64,
) -> Summary {
    let with_expected: Vec<&IconicResult> = iconic
        .iter()
        .filter(|r| !r.expected_gutenberg_ids.is_empty())
        .collect();
    let denom = with_expected.len().max(1) as f32;
    let recall_at_k = with_expected
        .iter()
        .filter(|r| r.expected_rank.is_some())
        .count() as f32
        / denom;
    let mrr = with_expected.iter().map(|r| r.reciprocal_rank).sum::<f32>() / denom;
    let at_thresh = |thresh: usize| {
        with_expected
            .iter()
            .filter(|r| r.expected_rank.is_some_and(|r| r <= thresh))
            .count() as f32
            / denom
    };
    let adv_max = adversarial
        .iter()
        .map(|r| r.top_similarity)
        .fold(0.0_f32, f32::max);

    Summary {
        iconic_count: iconic.len(),
        iconic_with_expected_books: with_expected.len(),
        iconic_recall_at_k: recall_at_k,
        iconic_mrr: mrr,
        iconic_hits_at_1: at_thresh(1),
        iconic_hits_at_5: at_thresh(5),
        iconic_hits_at_10: at_thresh(10),
        adversarial_max_similarity: adv_max,
        search_pass_seconds: search_seconds,
    }
}

fn print_summary(r: &BenchResults) {
    eprintln!();
    eprintln!("════════════════════════════════════════");
    eprintln!("  fathom-bench  tag={}", r.tag);
    eprintln!("════════════════════════════════════════");
    eprintln!("  books_loaded:                {}", r.books_loaded);
    eprintln!("  top_k:                       {}", r.top_k);
    eprintln!(
        "  search wall:                 {:.2}s",
        r.summary.search_pass_seconds
    );
    eprintln!();
    eprintln!(
        "  iconic queries:              {} ({} with expected books in corpus)",
        r.summary.iconic_count, r.summary.iconic_with_expected_books
    );
    eprintln!(
        "    recall@top_k:              {:.1}%",
        r.summary.iconic_recall_at_k * 100.0
    );
    eprintln!("    MRR:                       {:.3}", r.summary.iconic_mrr);
    eprintln!(
        "    hits@1:                    {:.1}%",
        r.summary.iconic_hits_at_1 * 100.0
    );
    eprintln!(
        "    hits@5:                    {:.1}%",
        r.summary.iconic_hits_at_5 * 100.0
    );
    eprintln!(
        "    hits@10:                   {:.1}%",
        r.summary.iconic_hits_at_10 * 100.0
    );
    eprintln!();
    eprintln!(
        "  adversarial max similarity:  {:.3}",
        r.summary.adversarial_max_similarity
    );
    eprintln!("    (lower is better — we want adversarial queries to NOT score confidently)");
    eprintln!();
    eprintln!("  per-iconic-query expected-source rank:");

    // Sort with unranked at the bottom.
    let mut sorted: Vec<&IconicResult> = r.iconic.iter().collect();
    sorted.sort_by_key(|r| r.expected_rank.unwrap_or(usize::MAX));
    for ir in sorted {
        let rank_str = match ir.expected_rank {
            Some(r) => format!("#{}", r),
            None if ir.expected_gutenberg_ids.is_empty() => "n/a (not in corpus)".to_string(),
            None => "miss".to_string(),
        };
        let sim_str = ir
            .expected_best_similarity
            .map(|s| format!(" sim={:.3}", s))
            .unwrap_or_default();
        eprintln!(
            "    [{:>8}]{}  {:?}",
            rank_str,
            sim_str,
            ir.query.chars().take(60).collect::<String>()
        );
    }
    eprintln!("════════════════════════════════════════");
}
