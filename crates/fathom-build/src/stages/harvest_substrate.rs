//! Stage 11 — harvest-substrate.
//!
//! Operator-only stage. Mines high-confidence substrate-term occurrences from
//! the chunked corpus and emits `pending-lexicon.jsonl` for operator review.
//! Output is intended to be reviewed by hand and merged into the curated YAML
//! lexicon at `lexicon/` (repo root); a future TUI review tool is out of scope.
//!
//! Pipeline per book:
//! 1. For each chunk, ask Gemma to identify English phrases doing technical
//!    philosophical work (`fathom_core::identify::identify_terms`).
//! 2. For each identified phrase, ask Gemma for a substrate-anchored gloss
//!    using a single-turn prompt (`HARVEST_GLOSS_PROMPT`).
//! 3. NLI-judge the proposed gloss against the source chunk via
//!    `fathom_core::judge::score_paraphrase`. Accept iff
//!    `support > 0.6 && contradiction_max < 0.1`.
//! 4. Cluster accepted candidates by `(canonical, substrate)` across all books.
//! 5. Emit one JSON-Lines record per cluster, sorted by cluster size descending.
//!
//! Gating thresholds are the same constants the runtime uses
//! (`FAITHFULNESS_SUPPORT_FLOOR`, `FAITHFULNESS_CONTRADICTION_CEILING` in
//! `fathom_core::types`).

#[path = "harvest_prompts.rs"]
mod harvest_prompts;

use crate::fs_state::{build_state_dir, ensure_dir};
use crate::stages::chunk_stage::ChunkedBook;
use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use fathom_core::judge;
use fathom_core::lexicon::all_entries;
use fathom_core::types::{FAITHFULNESS_CONTRADICTION_CEILING, FAITHFULNESS_SUPPORT_FLOOR};
use fathom_engine::{Backend as _, LlamaCppBackend};
use harvest_prompts::{render_gloss_prompt, HarvestGlossResponse};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Directory containing gemma3-4b-it Q4_K_M GGUF + DeBERTa NLI ONNX +
    /// tokenizer. Defaults to the user's standard fathom model dir.
    #[arg(long, env = "FATHOM_MODEL_DIR")]
    pub model_dir: Option<PathBuf>,
    /// Limit to first N books — useful for smoke-testing.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Maximum candidates per chunk to gate. Default 5.
    #[arg(long, default_value_t = 5)]
    pub max_candidates_per_chunk: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestCandidate {
    pub canonical: String,
    pub substrate: String,
    pub gloss: String,
    pub citations: Vec<Citation>,
    pub support_mean: f32,
    pub seed_match: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub gutenberg_id: u32,
    pub chunk_id: String,
    pub support: f32,
}

type Observation = (String, String, String, Citation);

fn seed_canonical_set() -> HashSet<String> {
    all_entries()
        .iter()
        .flat_map(|e| e.passage.terms.keys().cloned())
        .map(|k| k.to_lowercase())
        .collect()
}

fn cluster_observations(
    observations: Vec<Observation>,
    seed: &HashSet<String>,
) -> Vec<HarvestCandidate> {
    let mut by_key: HashMap<(String, String), Vec<(String, String, Citation)>> = HashMap::new();
    for (canonical, substrate, gloss, citation) in observations {
        let key = (canonical.to_lowercase(), substrate.clone());
        by_key
            .entry(key)
            .or_default()
            .push((canonical, gloss, citation));
    }
    let mut clusters: Vec<HarvestCandidate> = by_key
        .into_iter()
        .map(|((canonical_key, substrate), entries)| {
            let citations: Vec<Citation> = entries.iter().map(|(_, _, c)| c.clone()).collect();
            let support_mean: f32 =
                citations.iter().map(|c| c.support).sum::<f32>() / citations.len() as f32;
            // gloss from highest-support entry (deterministic: lowest gutenberg_id on tie)
            let best = entries
                .iter()
                .min_by(|a, b| {
                    b.2.support
                        .partial_cmp(&a.2.support)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.2.gutenberg_id.cmp(&b.2.gutenberg_id))
                })
                .expect("cluster is non-empty by construction");
            let canonical = best.0.clone();
            let gloss = best.1.clone();
            let mut sorted_citations = citations;
            sorted_citations.sort_by(|a, b| {
                b.support
                    .partial_cmp(&a.support)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.gutenberg_id.cmp(&b.gutenberg_id))
                    .then_with(|| a.chunk_id.cmp(&b.chunk_id))
            });
            HarvestCandidate {
                canonical,
                substrate,
                gloss,
                citations: sorted_citations,
                support_mean,
                seed_match: seed.contains(&canonical_key),
            }
        })
        .collect();
    clusters.sort_by(|a, b| {
        b.citations
            .len()
            .cmp(&a.citations.len())
            .then_with(|| a.canonical.cmp(&b.canonical))
    });
    clusters
}

pub async fn run(args: Args) -> Result<()> {
    let model_dir = args
        .model_dir
        .or_else(|| {
            directories::ProjectDirs::from("nz", "omit", "fathom")
                .map(|p| p.data_dir().join("models"))
        })
        .ok_or_else(|| anyhow!("no --model-dir given and ProjectDirs resolution failed"))?;
    let gemma_path = model_dir.join("gemma-3-4b-it-Q4_K_M.gguf");
    if !gemma_path.exists() {
        return Err(anyhow!(
            "Gemma GGUF not found at {} — run the desktop app once or sideload it",
            gemma_path.display()
        ));
    }

    let gemma = LlamaCppBackend::load(gemma_path).context("load Gemma")?;
    judge::ensure_loaded(None)
        .await
        .context("load DeBERTa NLI")?;
    let seed = seed_canonical_set();

    let chunks_dir = build_state_dir().join("chunks");
    let mut book_paths: Vec<PathBuf> = std::fs::read_dir(&chunks_dir)
        .with_context(|| format!("read chunks dir {}", chunks_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "json"))
        .collect();
    book_paths.sort();
    if let Some(n) = args.limit {
        book_paths.truncate(n);
    }

    eprintln!("harvest: scanning {} books", book_paths.len());

    let mut observations: Vec<Observation> = Vec::new();
    for (book_idx, path) in book_paths.iter().enumerate() {
        let cb: ChunkedBook =
            crate::fs_state::read_json(path).with_context(|| format!("read {}", path.display()))?;
        if book_idx % 10 == 0 {
            eprintln!(
                "harvest: book {}/{} gid={}",
                book_idx + 1,
                book_paths.len(),
                cb.gutenberg_id
            );
        }
        for chunk in &cb.chunks {
            let phrases = match fathom_core::identify::identify_terms(&chunk.text, &gemma).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!(
                        "harvest: identify_terms failed for gid={} chunk={}: {e:#}",
                        cb.gutenberg_id, chunk.chunk_id
                    );
                    continue;
                }
            };
            for phrase in phrases.into_iter().take(args.max_candidates_per_chunk) {
                let prompt = render_gloss_prompt(&chunk.text, &phrase);
                let response_json = match gemma.generate_json(&prompt).await {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!(
                            "harvest: gloss failed for gid={} chunk={} phrase={phrase:?}: {e:#}",
                            cb.gutenberg_id, chunk.chunk_id
                        );
                        continue;
                    }
                };
                let response: HarvestGlossResponse = match serde_json::from_str(&response_json) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let (Some(substrate), Some(gloss)) = (response.substrate, response.gloss) else {
                    continue;
                };
                let paraphrase = format!("{phrase}: {gloss}");
                let score = match judge::score_paraphrase(&chunk.text, &paraphrase) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!(
                            "harvest: NLI score failed for gid={} chunk={}: {e:#}",
                            cb.gutenberg_id, chunk.chunk_id
                        );
                        continue;
                    }
                };
                if score.support <= FAITHFULNESS_SUPPORT_FLOOR
                    || score.contradiction_max >= FAITHFULNESS_CONTRADICTION_CEILING
                {
                    continue;
                }
                observations.push((
                    phrase,
                    substrate,
                    gloss,
                    Citation {
                        gutenberg_id: cb.gutenberg_id,
                        chunk_id: chunk.chunk_id.clone(),
                        support: score.support,
                    },
                ));
            }
        }
    }

    let clusters = cluster_observations(observations, &seed);

    let out_dir = build_state_dir().join("harvest");
    ensure_dir(&out_dir)?;
    let out_path = out_dir.join("pending-lexicon.jsonl");
    let mut out = std::fs::File::create(&out_path)
        .with_context(|| format!("create {}", out_path.display()))?;
    for c in &clusters {
        let line = serde_json::to_string(c).context("serialise HarvestCandidate")?;
        writeln!(out, "{line}").context("write line")?;
    }

    let total_citations: usize = clusters.iter().map(|c| c.citations.len()).sum();
    let seed_matches = clusters.iter().filter(|c| c.seed_match).count();
    let net_new = clusters.len() - seed_matches;
    eprintln!(
        "harvest: {} clusters, {} total citations, {} seed-match, {} net-new → {}",
        clusters.len(),
        total_citations,
        seed_matches,
        net_new,
        out_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvest_candidate_roundtrips_through_jsonl() {
        let c = HarvestCandidate {
            canonical: "in our power".into(),
            substrate: "eph' hēmin".into(),
            gloss: "up to us — actions that depend on our choice".into(),
            citations: vec![Citation {
                gutenberg_id: 45109,
                chunk_id: "0001-000003".into(),
                support: 0.87,
            }],
            support_mean: 0.87,
            seed_match: true,
        };
        let line = serde_json::to_string(&c).unwrap();
        let back: HarvestCandidate = serde_json::from_str(&line).unwrap();
        assert_eq!(back.canonical, c.canonical);
        assert_eq!(back.substrate, c.substrate);
        assert_eq!(back.citations.len(), 1);
        assert!((back.support_mean - c.support_mean).abs() < 1e-6);
        assert!(back.seed_match);
    }

    #[test]
    fn cluster_collapses_same_canonical_substrate_across_chunks() {
        let observations = vec![
            (
                "in our power".to_string(),
                "eph' hēmin".to_string(),
                "up to us".to_string(),
                Citation {
                    gutenberg_id: 45109,
                    chunk_id: "0001-000003".into(),
                    support: 0.87,
                },
            ),
            (
                "in our power".to_string(),
                "eph' hēmin".to_string(),
                "what depends on us".to_string(),
                Citation {
                    gutenberg_id: 1232,
                    chunk_id: "0007-000012".into(),
                    support: 0.92,
                },
            ),
            (
                "preferred indifferents".to_string(),
                "proēgmena".to_string(),
                "things naturally preferred but morally neutral".to_string(),
                Citation {
                    gutenberg_id: 45109,
                    chunk_id: "0010-000002".into(),
                    support: 0.81,
                },
            ),
        ];
        let seed: HashSet<String> = ["in our power".to_string()].iter().cloned().collect();

        let clusters = cluster_observations(observations, &seed);

        assert_eq!(clusters.len(), 2);
        let eph = clusters
            .iter()
            .find(|c| c.substrate == "eph' hēmin")
            .unwrap();
        assert_eq!(eph.citations.len(), 2);
        assert!(eph.seed_match);
        assert_eq!(eph.gloss, "what depends on us");
        assert!((eph.support_mean - 0.895).abs() < 1e-3);
        let pro = clusters
            .iter()
            .find(|c| c.substrate == "proēgmena")
            .unwrap();
        assert_eq!(pro.citations.len(), 1);
        assert!(!pro.seed_match);
    }
}
