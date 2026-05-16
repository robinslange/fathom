//! Stage 8 — manifest.
//!
//! Assemble `dist/index.msgpack`: the single artefact the runtime fetches first
//! to know what books exist, which traditions they belong to, and where their
//! shards live. Schema is deliberately small and stable — adding fields is
//! backward-compatible; renaming or removing is a `manifest_version` bump.

use crate::fs_state::{build_state_dir, filtered_path, read_json};
use crate::stages::shard::{dist_dir, ShardSummary};
use crate::types::{Agent, Filtered};
use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MANIFEST_VERSION: u32 = 1;
pub const TRADITIONS_DEFAULT: &str = "uncategorised";

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Optional path to traditions.json. If absent, every book ships as
    /// "uncategorised".
    #[arg(long)]
    pub traditions: Option<PathBuf>,
    /// Build identifier (e.g. "2026-05"). Defaults to current YYYY-MM.
    #[arg(long)]
    pub build_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub build_id: String,
    pub generated: String,
    pub embed_model_id: String,
    pub embed_dims: usize,
    pub book_count: usize,
    pub books: Vec<ManifestBook>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestBook {
    pub gutenberg_id: u32,
    pub title: String,
    pub translators: Vec<Agent>,
    pub locc: Vec<String>,
    pub tradition: String,
    pub shard_filename: String,
    pub shard_sha256: String,
    pub shard_size_bytes: u64,
    pub chunk_count: usize,
}

#[derive(Debug, Deserialize)]
struct TraditionsEntry {
    gutenberg_id: u32,
    tradition: String,
}

pub async fn run(args: Args) -> Result<()> {
    let summaries: Vec<ShardSummary> =
        read_json(&build_state_dir().join("shard-summaries.json"))
            .context("load shard-summaries.json — run shard first")?;
    let filtered: Vec<Filtered> =
        read_json(&filtered_path()).context("load filtered.json")?;

    let by_id: HashMap<u32, &Filtered> =
        filtered.iter().map(|f| (f.gutenberg_id, f)).collect();

    let traditions_map = match args.traditions {
        Some(path) => {
            let entries: Vec<TraditionsEntry> = read_json(&path)
                .with_context(|| format!("load traditions {}", path.display()))?;
            entries
                .into_iter()
                .map(|e| (e.gutenberg_id, e.tradition))
                .collect()
        }
        None => HashMap::new(),
    };

    let build_id = args
        .build_id
        .unwrap_or_else(|| Utc::now().format("%Y-%m").to_string());
    let generated = Utc::now().to_rfc3339();

    let books: Vec<ManifestBook> = summaries
        .into_iter()
        .map(|s| {
            let book = by_id.get(&s.gutenberg_id);
            let tradition = traditions_map
                .get(&s.gutenberg_id)
                .cloned()
                .unwrap_or_else(|| TRADITIONS_DEFAULT.to_string());
            ManifestBook {
                gutenberg_id: s.gutenberg_id,
                title: book.map(|b| b.title.clone()).unwrap_or_default(),
                translators: book.map(|b| b.translators.clone()).unwrap_or_default(),
                locc: book.map(|b| b.locc.clone()).unwrap_or_default(),
                tradition,
                shard_filename: s.shard_filename,
                shard_sha256: s.shard_sha256,
                shard_size_bytes: s.shard_size_bytes,
                chunk_count: s.chunk_count,
            }
        })
        .collect();

    let manifest = Manifest {
        manifest_version: MANIFEST_VERSION,
        build_id,
        generated,
        embed_model_id: fathom_embed::EMBED_MODEL_ID.to_string(),
        embed_dims: fathom_embed::EMBED_DIMS,
        book_count: books.len(),
        books,
    };

    let mp = rmp_serde::to_vec_named(&manifest).context("manifest msgpack encode")?;
    let out_path = dist_dir().join("index.msgpack");
    std::fs::write(&out_path, &mp)
        .with_context(|| format!("write {}", out_path.display()))?;

    eprintln!(
        "manifest: {} books, {} bytes → {}",
        manifest.book_count,
        mp.len(),
        out_path.display()
    );
    Ok(())
}
