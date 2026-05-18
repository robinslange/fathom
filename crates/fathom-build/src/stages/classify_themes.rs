//! Stage 12 — classify-themes.
//!
//! Operator-only stage. For each book in the corpus manifest, dispatches
//! a Sonnet 4.6 subagent (via the host Claude Code session) with the book's
//! title + first-page text + a beginner-framed theme taxonomy; assembles
//! the returned classifications into `crates/fathom-core/data/themes.json`
//! for in-binary inclusion.
//!
//! The actual subagent dispatch is driven by the host session, not by this
//! binary — this stage prepares the per-book input JSON
//! (`build-state/themes-input.jsonl`) and consumes the agent-produced
//! output JSON (`build-state/themes-output.jsonl`), then assembles the
//! final `themes.json`.

use crate::fs_state::build_state_dir;
use crate::shard_format::Shard;
use crate::stages::manifest::Manifest;
use crate::stages::shard::dist_dir;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Limit to first N books — useful for the 50-book validation pass.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Skip the prepare step (input JSONL already written by a previous run).
    #[arg(long, default_value_t = false)]
    pub skip_prepare: bool,
    /// Skip the assemble step (only emit the input JSONL).
    #[arg(long, default_value_t = false)]
    pub skip_assemble: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            limit: None,
            skip_prepare: false,
            skip_assemble: false,
        }
    }
}

/// Seed taxonomy. Slugs are stable; labels may iterate during validation pass.
pub const SEED_TAXONOMY: &[(&str, &str)] = &[
    ("mind-and-self", "Who am I really?"),
    ("how-to-live", "How should I live?"),
    ("suffering-and-loss", "Suffering and loss"),
    ("love-and-friendship", "Love and friendship"),
    ("power-and-justice", "Power and justice"),
    ("knowledge", "What can we know?"),
    ("reality", "What is real?"),
    ("meaning", "Why are we here?"),
    ("religion", "God, faith, doubt"),
    ("other", "Other"),
];

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeInputRecord {
    pub gutenberg_id: u32,
    pub title: String,
    pub translators: Vec<String>,
    pub locc: Vec<String>,
    pub first_page: String,
}

pub fn themes_input_path() -> PathBuf {
    build_state_dir().join("themes-input.jsonl")
}

pub fn themes_output_path() -> PathBuf {
    build_state_dir().join("themes-output.jsonl")
}

pub async fn run(args: Args) -> Result<()> {
    eprintln!("classify-themes: limit={:?}", args.limit);
    if !args.skip_prepare {
        prepare_input(args.limit)?;
    }
    if !args.skip_assemble {
        assemble_output()?;
    }
    Ok(())
}

fn prepare_input(limit: Option<usize>) -> Result<()> {
    let manifest_path = dist_dir().join("index.msgpack");
    let mp_bytes =
        std::fs::read(&manifest_path).with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: Manifest =
        rmp_serde::from_slice(&mp_bytes).context("msgpack decode index.msgpack")?;

    let books = match limit {
        Some(n) => &manifest.books[..n.min(manifest.books.len())],
        None => &manifest.books[..],
    };

    let out_path = themes_input_path();
    let file = std::fs::File::create(&out_path)
        .with_context(|| format!("create {}", out_path.display()))?;
    let mut writer = BufWriter::new(file);

    for book in books {
        let shard_path = dist_dir().join(&book.shard_filename);
        let compressed = std::fs::read(&shard_path)
            .with_context(|| format!("read shard {}", shard_path.display()))?;
        let mp = zstd::decode_all(&compressed[..])
            .with_context(|| format!("zstd decode {}", shard_path.display()))?;
        let shard: Shard = rmp_serde::from_slice(&mp)
            .with_context(|| format!("msgpack decode {}", shard_path.display()))?;

        let first_page: String = shard.canonical_text.chars().take(1500).collect();

        let record = ThemeInputRecord {
            gutenberg_id: book.gutenberg_id,
            title: book.title.clone(),
            translators: book.translators.iter().map(|t| t.name.clone()).collect(),
            locc: book.locc.clone(),
            first_page,
        };

        serde_json::to_writer(&mut writer, &record)
            .with_context(|| format!("serialize record for gutenberg_id {}", book.gutenberg_id))?;
        writeln!(writer)?;
    }

    writer.flush().context("flush themes-input.jsonl")?;

    eprintln!(
        "classify-themes: wrote {} input records → {}",
        books.len(),
        out_path.display()
    );
    Ok(())
}

fn assemble_output() -> Result<()> {
    // Implemented in Task 3.
    anyhow::bail!("assemble_output not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_record_serializes_to_one_line() {
        let r = ThemeInputRecord {
            gutenberg_id: 1497,
            title: "The Republic".to_string(),
            translators: vec!["Benjamin Jowett".to_string()],
            locc: vec!["B358".to_string()],
            first_page: "Socrates speaks first.".to_string(),
        };
        let line = serde_json::to_string(&r).unwrap();
        assert!(!line.contains('\n'), "JSONL records must be single-line");
        assert!(line.contains("\"gutenberg_id\":1497"));
        assert!(line.contains("\"first_page\":\"Socrates speaks first.\""));
    }

    #[test]
    fn seed_taxonomy_contains_other_bucket() {
        let slugs: Vec<&str> = SEED_TAXONOMY.iter().map(|(s, _)| *s).collect();
        assert!(slugs.contains(&"other"), "misfit bucket required");
        assert_eq!(SEED_TAXONOMY.len(), 10, "9 content themes + other");
    }
}
