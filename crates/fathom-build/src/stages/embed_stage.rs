//! Stage 6 — embed.
//!
//! For each chunked book: load chunks.json, embed all chunks via
//! `fathom-embed::embed_batch` in batches of 32, write packed f16 vectors to
//! `build-state/embeddings/{id}.bin`. Wire format: sequence of 768-byte vectors
//! in chunk-order — no header, no count. Sharding stage reads chunks.json for
//! count + order and zips them together.

use crate::fs_state::{build_state_dir, ensure_dir, read_json};
use crate::stages::chunk_stage::ChunkedBook;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use fathom_embed::{embed_batch, init_embedder, to_f16_bytes, EMBED_DIMS};
use std::io::Write;
use std::path::PathBuf;

const DEFAULT_BATCH: usize = 32;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Directory containing bge-small.onnx + tokenizer.json. Required.
    #[arg(long, env = "FATHOM_BGE_MODEL_DIR")]
    pub model_dir: PathBuf,
    /// Batch size for embed_batch. Default 32 — bge-small at 384-dim CPU is
    /// memory-light; 32 stays well under typical 1-2 GB working set.
    #[arg(long, default_value_t = DEFAULT_BATCH)]
    pub batch: usize,
    /// Limit to first N books.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Force re-embed even if .bin exists.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(args: Args) -> Result<()> {
    let model = args.model_dir.join("bge-small.onnx");
    let tokenizer = args.model_dir.join("tokenizer.json");
    init_embedder(&model, &tokenizer)
        .with_context(|| format!("init embedder from {}", args.model_dir.display()))?;

    let chunks_dir = build_state_dir().join("chunks");
    let embeddings_dir = build_state_dir().join("embeddings");
    ensure_dir(&embeddings_dir)?;

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&chunks_dir)
        .with_context(|| format!("read chunks dir {}", chunks_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "json"))
        .collect();
    entries.sort();

    if let Some(n) = args.limit {
        entries.truncate(n);
    }

    eprintln!("embed: {} books", entries.len());

    let mut total_vectors = 0usize;
    for (i, path) in entries.iter().enumerate() {
        let cb: ChunkedBook = read_json(path)?;
        let out_path = embeddings_dir.join(format!("{}.bin", cb.gutenberg_id));
        if !args.force && out_path.exists() {
            continue;
        }
        if cb.chunks.is_empty() {
            // Still write an empty file so the manifest stage sees the book.
            std::fs::write(&out_path, [])?;
            continue;
        }

        let mut f = std::fs::File::create(&out_path)
            .with_context(|| format!("create {}", out_path.display()))?;

        let mut written = 0usize;
        for batch in cb.chunks.chunks(args.batch) {
            let texts: Vec<&str> = batch.iter().map(|c| c.text.as_str()).collect();
            let embeds = embed_batch(&texts)
                .with_context(|| format!("embed batch in pg{}", cb.gutenberg_id))?;
            for e in embeds {
                debug_assert_eq!(e.vector.len(), EMBED_DIMS);
                let bytes = to_f16_bytes(&e.vector);
                f.write_all(&bytes)?;
                written += 1;
            }
        }
        f.flush()?;
        total_vectors += written;

        if (i + 1) % 25 == 0 || i + 1 == entries.len() {
            eprintln!(
                "  ...{}/{} (cum vectors {})",
                i + 1,
                entries.len(),
                total_vectors
            );
        }
    }

    eprintln!("embed: done. total_vectors={}", total_vectors);
    Ok(())
}
