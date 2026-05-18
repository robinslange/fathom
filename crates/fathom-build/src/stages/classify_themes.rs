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

use crate::fs_state::{build_state_dir, write_json};
use crate::shard_format::Shard;
use crate::stages::manifest::Manifest;
use crate::stages::shard::dist_dir;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufWriter, Write};
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

const ALLOWED_DROP_REASONS: &[&str] = &[
    "duplicate",
    "history-of-philosophy",
    "biography",
    "intro-textbook",
];

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

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeOutputRecord {
    pub gutenberg_id: u32,
    pub themes: Vec<String>,
    pub confidence: String,
    pub reasoning: String,
    #[serde(default = "default_keep")]
    pub keep: bool,
    #[serde(default)]
    pub drop_reason: Option<String>,
}

fn default_keep() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DroppedBook {
    pub gutenberg_id: u32,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemesFile {
    pub version: u32,
    pub generated_at: String,
    pub themes: Vec<ThemeEntry>,
    pub assignments: Vec<ThemeAssignment>,
    pub dropped: Vec<DroppedBook>,
    pub metadata: ThemesMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeEntry {
    pub slug: String,
    pub label: String,
    pub order: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeAssignment {
    pub gutenberg_id: u32,
    pub themes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemesMetadata {
    pub books_classified: usize,
    pub books_in_other: usize,
    pub books_dropped: usize,
    pub model: String,
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
    let output_path = themes_output_path();
    let file = std::fs::File::open(&output_path)
        .with_context(|| format!("open {}", output_path.display()))?;
    let reader = std::io::BufReader::new(file);

    let known: std::collections::HashSet<&str> =
        SEED_TAXONOMY.iter().map(|(s, _)| *s).collect();

    let mut assignments: Vec<ThemeAssignment> = Vec::new();
    let mut dropped: Vec<DroppedBook> = Vec::new();
    let mut books_in_other: usize = 0;

    for line in reader.lines() {
        let line = line.context("read line from themes-output.jsonl")?;
        if line.trim().is_empty() {
            continue;
        }
        let record: ThemeOutputRecord = serde_json::from_str(&line)
            .with_context(|| format!("parse ThemeOutputRecord from: {}", line))?;

        if !record.keep {
            match &record.drop_reason {
                Some(reason) if ALLOWED_DROP_REASONS.contains(&reason.as_str()) => {
                    dropped.push(DroppedBook {
                        gutenberg_id: record.gutenberg_id,
                        reason: reason.clone(),
                    });
                }
                _ => anyhow::bail!(
                    "invalid drop_reason '{:?}' for pg{}",
                    record.drop_reason,
                    record.gutenberg_id
                ),
            }
            continue;
        }

        for slug in &record.themes {
            if !known.contains(slug.as_str()) {
                anyhow::bail!(
                    "unknown theme slug '{}' for pg{} — taxonomy drift",
                    slug,
                    record.gutenberg_id
                );
            }
        }

        if record.themes.iter().any(|t| t == "other") {
            books_in_other += 1;
        }

        assignments.push(ThemeAssignment {
            gutenberg_id: record.gutenberg_id,
            themes: record.themes,
        });
    }

    assignments.sort_by_key(|a| a.gutenberg_id);
    dropped.sort_by_key(|d| d.gutenberg_id);

    let themes: Vec<ThemeEntry> = SEED_TAXONOMY
        .iter()
        .enumerate()
        .map(|(i, (slug, label))| ThemeEntry {
            slug: slug.to_string(),
            label: label.to_string(),
            order: if *slug == "other" { 99 } else { i as u32 + 1 },
        })
        .collect();

    let out_path = PathBuf::from("crates/fathom-core/data/themes.json");
    let books_classified = assignments.len();
    let books_dropped = dropped.len();
    let themes_file = ThemesFile {
        version: 1,
        generated_at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        themes,
        assignments,
        dropped,
        metadata: ThemesMetadata {
            books_classified,
            books_in_other,
            books_dropped,
            model: "claude-sonnet-4-6".to_string(),
        },
    };

    write_json(&out_path, &themes_file)
        .with_context(|| format!("write {}", out_path.display()))?;

    eprintln!(
        "classify-themes: wrote {} assignments ({} other, {} dropped) → {}",
        books_classified,
        books_in_other,
        books_dropped,
        out_path.display()
    );
    Ok(())
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

    #[test]
    fn assembler_rejects_unknown_theme_slugs() {
        let bad_record = ThemeOutputRecord {
            gutenberg_id: 1,
            themes: vec!["mind-and-self".into(), "phlogiston".into()],
            confidence: "high".into(),
            reasoning: "test".into(),
            keep: true,
            drop_reason: None,
        };
        let known: std::collections::HashSet<&str> =
            SEED_TAXONOMY.iter().map(|(s, _)| *s).collect();
        let unknown: Vec<&String> = bad_record
            .themes
            .iter()
            .filter(|t| !known.contains(t.as_str()))
            .collect();
        assert_eq!(unknown, vec![&"phlogiston".to_string()]);
    }

    #[test]
    fn other_bucket_gets_order_99() {
        let themes: Vec<ThemeEntry> = SEED_TAXONOMY
            .iter()
            .enumerate()
            .map(|(i, (slug, label))| ThemeEntry {
                slug: slug.to_string(),
                label: label.to_string(),
                order: if *slug == "other" { 99 } else { i as u32 + 1 },
            })
            .collect();
        let other = themes.iter().find(|t| t.slug == "other").unwrap();
        assert_eq!(other.order, 99);
    }

    #[test]
    fn drop_reason_must_be_in_allowlist() {
        let unknown = "made-up-reason";
        assert!(!ALLOWED_DROP_REASONS.contains(&unknown));
        for r in ALLOWED_DROP_REASONS {
            assert!(!r.is_empty());
        }
    }
}
