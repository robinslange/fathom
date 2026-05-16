//! Stage 7 — shard.
//!
//! Pack per-book {chunks.json + embeddings.bin + translator metadata from
//! filtered.json} into a single zstd-compressed msgpack blob written to
//! `dist/shards/{id}.shard`. Records SHA-256 of the compressed bytes for the
//! manifest stage.

use crate::fs_state::{build_state_dir, ensure_dir, filtered_path, read_json};
use crate::shard_format::{Shard, ShardChunk, SHARD_FORMAT_VERSION};
use crate::stages::chunk_stage::ChunkedBook;
use crate::types::Filtered;
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use fathom_embed::EMBED_MODEL_ID;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

const ZSTD_LEVEL: i32 = 3;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long)]
    pub force: bool,
}

/// Sidecar written alongside the shard tree — feeds the manifest stage.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShardSummary {
    pub gutenberg_id: u32,
    pub shard_filename: String,
    pub shard_sha256: String,
    pub shard_size_bytes: u64,
    pub chunk_count: usize,
}

pub async fn run(args: Args) -> Result<()> {
    let filtered: Vec<Filtered> =
        read_json(&filtered_path()).context("load filtered.json — run filter first")?;
    let by_id: HashMap<u32, &Filtered> = filtered.iter().map(|f| (f.gutenberg_id, f)).collect();

    let chunks_dir = build_state_dir().join("chunks");
    let embeddings_dir = build_state_dir().join("embeddings");
    let shards_dir = dist_shards_dir();
    ensure_dir(&shards_dir)?;

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

    eprintln!("shard: {} books", entries.len());

    let mut summaries = Vec::with_capacity(entries.len());
    let mut total_bytes = 0u64;

    for path in &entries {
        let cb: ChunkedBook = read_json(path)?;
        let book = by_id
            .get(&cb.gutenberg_id)
            .with_context(|| format!("pg{} not in filtered.json", cb.gutenberg_id))?;

        let bin_path = embeddings_dir.join(format!("{}.bin", cb.gutenberg_id));
        let embeddings_bytes = std::fs::read(&bin_path)
            .with_context(|| format!("read {}", bin_path.display()))?;
        let expected = cb.chunks.len() * 768;
        if embeddings_bytes.len() != expected {
            bail!(
                "pg{}: embeddings.bin is {} bytes, expected {} (chunks * 768)",
                cb.gutenberg_id,
                embeddings_bytes.len(),
                expected
            );
        }

        let shard_chunks: Vec<ShardChunk> = cb
            .chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let start = i * 768;
                let end = start + 768;
                let embedding = embeddings_bytes[start..end].to_vec();
                ShardChunk::from_chunk(c, embedding)
            })
            .collect();

        let shard = Shard {
            format_version: SHARD_FORMAT_VERSION,
            gutenberg_id: cb.gutenberg_id,
            title: cb.title.clone(),
            translators: book.translators.clone(),
            embed_model_id: EMBED_MODEL_ID.to_string(),
            canonical_text: cb.canonical_text.clone(),
            chunks: shard_chunks,
        };

        let mp = rmp_serde::to_vec_named(&shard)
            .with_context(|| format!("msgpack serialise pg{}", cb.gutenberg_id))?;
        let compressed = zstd::encode_all(&mp[..], ZSTD_LEVEL)
            .with_context(|| format!("zstd compress pg{}", cb.gutenberg_id))?;

        let mut hasher = Sha256::new();
        hasher.update(&compressed);
        let sha = hex_digest(hasher.finalize().as_ref());
        let shard_filename = format!("{}.shard", cb.gutenberg_id);
        let out_path = shards_dir.join(&shard_filename);
        std::fs::write(&out_path, &compressed)
            .with_context(|| format!("write {}", out_path.display()))?;

        total_bytes += compressed.len() as u64;
        summaries.push(ShardSummary {
            gutenberg_id: cb.gutenberg_id,
            shard_filename,
            shard_sha256: sha,
            shard_size_bytes: compressed.len() as u64,
            chunk_count: cb.chunks.len(),
        });
    }

    let summaries_path = build_state_dir().join("shard-summaries.json");
    crate::fs_state::write_json(&summaries_path, &summaries)?;

    eprintln!(
        "shard: {} shards, total {} bytes ({:.1} MB) → {}",
        summaries.len(),
        total_bytes,
        total_bytes as f64 / 1_048_576.0,
        shards_dir.display()
    );
    Ok(())
}

pub fn dist_shards_dir() -> PathBuf {
    dist_dir().join("shards")
}

pub fn dist_dir() -> PathBuf {
    std::env::var("FATHOM_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("dist"))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Read + decompress a shard from disk. Used by `verify`.
#[allow(dead_code)]
pub fn read_shard(path: &PathBuf) -> Result<Shard> {
    let mut f = std::fs::File::open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let mut compressed = Vec::new();
    f.read_to_end(&mut compressed)?;
    let mp = zstd::decode_all(&compressed[..])
        .with_context(|| format!("zstd decode {}", path.display()))?;
    let shard: Shard = rmp_serde::from_slice(&mp)
        .with_context(|| format!("msgpack decode {}", path.display()))?;
    Ok(shard)
}
