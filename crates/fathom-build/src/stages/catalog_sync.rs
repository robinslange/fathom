//! Stage 1 — catalog-sync.
//!
//! Fetch the Gutenberg pg_catalog.csv, parse it into typed rows, filter to
//! LCC B/BC/BD/BH/BJ + English-language, write build-state/candidates.json.

use crate::catalog::{candidate_from_row, is_philosophy_locc, parse_semi_list};
use crate::fs_state::{candidates_path, catalog_csv_path, ensure_dir, write_json};
use crate::types::Candidate;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::path::PathBuf;

const CATALOG_URL: &str = "https://www.gutenberg.org/cache/epub/feeds/pg_catalog.csv";
const USER_AGENT: &str = "Fathom-Build/0.2 (https://fathom.app contact@robinslange.dev)";

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Skip fetch and reuse build-state/pg_catalog.csv if it exists.
    #[arg(long)]
    pub offline: bool,
    /// Override the catalog URL (for testing).
    #[arg(long)]
    pub catalog_url: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    let csv_path = catalog_csv_path();
    ensure_dir(&csv_path.parent().unwrap().to_path_buf())?;

    if !args.offline || !csv_path.exists() {
        let url = args.catalog_url.unwrap_or_else(|| CATALOG_URL.to_string());
        eprintln!("catalog-sync: fetching {}", url);
        fetch_catalog(&url, &csv_path).await?;
    } else {
        eprintln!("catalog-sync: using cached {}", csv_path.display());
    }

    let candidates = parse_and_filter(&csv_path)?;
    let out_path = candidates_path();
    write_json(&out_path, &candidates)?;
    eprintln!(
        "catalog-sync: {} philosophy/English candidates → {}",
        candidates.len(),
        out_path.display()
    );
    Ok(())
}

async fn fetch_catalog(url: &str, out: &PathBuf) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    std::fs::write(out, &bytes).with_context(|| format!("write catalog to {}", out.display()))?;
    Ok(())
}

fn parse_and_filter(csv_path: &PathBuf) -> Result<Vec<Candidate>> {
    let mut rdr = csv::Reader::from_path(csv_path)
        .with_context(|| format!("open csv {}", csv_path.display()))?;
    let headers = rdr.headers()?.clone();
    let idx = |name: &str| {
        headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| anyhow::anyhow!("missing column {}", name))
    };
    let i_text = idx("Text#")?;
    let i_type = idx("Type")?;
    let i_title = idx("Title")?;
    let i_lang = idx("Language")?;
    let i_authors = idx("Authors")?;
    let i_subjects = idx("Subjects")?;
    let i_locc = idx("LoCC")?;
    let i_shelves = idx("Bookshelves")?;

    let mut candidates = Vec::new();
    let mut total = 0usize;
    let mut non_text = 0usize;
    let mut non_english = 0usize;
    let mut non_philosophy = 0usize;

    for rec in rdr.records() {
        let rec = rec?;
        total += 1;
        let row_type = rec.get(i_type).unwrap_or("");
        if row_type != "Text" {
            non_text += 1;
            continue;
        }
        let lang = rec.get(i_lang).unwrap_or("");
        if lang != "en" {
            non_english += 1;
            continue;
        }
        let locc_raw = rec.get(i_locc).unwrap_or("");
        let locc_values = parse_semi_list(locc_raw);
        if !is_philosophy_locc(&locc_values) {
            non_philosophy += 1;
            continue;
        }

        let gutenberg_id = rec
            .get(i_text)
            .unwrap_or("")
            .parse::<u32>()
            .with_context(|| format!("parse Text# {:?}", rec.get(i_text)))?;
        let candidate = candidate_from_row(
            gutenberg_id,
            rec.get(i_title).unwrap_or(""),
            lang,
            rec.get(i_authors).unwrap_or(""),
            rec.get(i_subjects).unwrap_or(""),
            locc_raw,
            rec.get(i_shelves).unwrap_or(""),
        );
        candidates.push(candidate);
    }

    eprintln!(
        "catalog-sync: scanned {} rows ({} non-Text, {} non-English, {} non-philosophy)",
        total, non_text, non_english, non_philosophy
    );
    Ok(candidates)
}
