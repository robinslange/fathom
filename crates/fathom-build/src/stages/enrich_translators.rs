//! Stage 2 — enrich-translators.
//!
//! For each Candidate from stage 1:
//! - If the CSV already gave us a translator with a death year, use it.
//! - Otherwise fetch pg{id}.rdf, parse <marcrel:trl>, cache locally.
//! - If the RDF also gives us nothing, record `rdf_has_translator: false` for
//!   the fallback pass (Wikidata + Open Library — not implemented in v0.2.0).
//!
//! Rate-limited fetcher: ~1 req/sec by default to stay polite with Gutenberg.

use crate::fs_state::{
    candidates_path, ensure_dir, rdf_cache_dir, read_json, translators_path, write_json,
};
use crate::translators::parse_translators_from_rdf;
use crate::types::{Agent, AgentRole, BookTranslators, Candidate};
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::time::Duration;
use tokio::time::sleep;

const RDF_URL_PREFIX: &str = "https://www.gutenberg.org/cache/epub";
const USER_AGENT: &str = "Fathom-Build/0.2 (https://fathom.app contact@robinslange.dev)";

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Minimum delay between RDF fetches (milliseconds). Default 1000ms.
    #[arg(long, default_value_t = 1000)]
    pub delay_ms: u64,
    /// Stop after this many books (for partial runs / smoke testing).
    #[arg(long)]
    pub limit: Option<usize>,
    /// Reuse cached RDFs in build-state/rdf-cache/ where present.
    #[arg(long, default_value_t = true)]
    pub use_cache: bool,
}

pub async fn run(args: Args) -> Result<()> {
    let candidates: Vec<Candidate> = read_json(&candidates_path())
        .with_context(|| "load candidates.json — run catalog-sync first")?;
    eprintln!("enrich-translators: {} candidates", candidates.len());

    let cache_dir = rdf_cache_dir();
    ensure_dir(&cache_dir)?;

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()?;

    let target: Vec<&Candidate> = if let Some(limit) = args.limit {
        candidates.iter().take(limit).collect()
    } else {
        candidates.iter().collect()
    };

    let mut out = Vec::with_capacity(target.len());
    let mut csv_hits = 0usize;
    let mut rdf_hits = 0usize;
    let mut gaps = 0usize;
    let mut original_lang = 0usize;

    for (i, candidate) in target.iter().enumerate() {
        if i > 0 && !rdf_fetched_from_csv(candidate) {
            sleep(Duration::from_millis(args.delay_ms)).await;
        }

        let mut translators: Vec<Agent> = candidate.csv_translators.clone();
        let has_csv_translator = !translators.is_empty();
        if has_csv_translator {
            csv_hits += 1;
        }

        let mut rdf_has_translator = false;

        if !has_csv_translator {
            // Need to check RDF.
            let xml =
                fetch_rdf(&client, candidate.gutenberg_id, &cache_dir, args.use_cache).await?;
            match parse_translators_from_rdf(&xml) {
                Ok(rdf_translators) => {
                    if !rdf_translators.is_empty() {
                        rdf_has_translator = true;
                        rdf_hits += 1;
                        translators.extend(rdf_translators);
                    } else {
                        gaps += 1;
                    }
                }
                Err(e) => {
                    eprintln!("  warn: parse RDF for pg{}: {}", candidate.gutenberg_id, e);
                    gaps += 1;
                }
            }
        }

        let is_original_language = looks_like_original_language(candidate, &translators);
        if is_original_language {
            original_lang += 1;
        }

        out.push(BookTranslators {
            gutenberg_id: candidate.gutenberg_id,
            translators,
            is_original_language,
            rdf_has_translator: has_csv_translator || rdf_has_translator,
        });

        if (i + 1) % 50 == 0 {
            eprintln!(
                "  ...{}/{} (csv {} rdf {} gaps {} original {})",
                i + 1,
                target.len(),
                csv_hits,
                rdf_hits,
                gaps,
                original_lang
            );
        }
    }

    let out_path = translators_path();
    write_json(&out_path, &out)?;
    eprintln!(
        "enrich-translators: csv={} rdf={} gaps={} original-lang={} → {}",
        csv_hits,
        rdf_hits,
        gaps,
        original_lang,
        out_path.display()
    );
    Ok(())
}

fn rdf_fetched_from_csv(c: &Candidate) -> bool {
    !c.csv_translators.is_empty()
}

async fn fetch_rdf(
    client: &reqwest::Client,
    gutenberg_id: u32,
    cache_dir: &std::path::Path,
    use_cache: bool,
) -> Result<String> {
    let cache_path = cache_dir.join(format!("pg{}.rdf", gutenberg_id));
    if use_cache && cache_path.exists() {
        return std::fs::read_to_string(&cache_path)
            .with_context(|| format!("read cached {}", cache_path.display()));
    }
    let url = format!("{}/{}/pg{}.rdf", RDF_URL_PREFIX, gutenberg_id, gutenberg_id);
    let resp = client.get(&url).send().await?.error_for_status()?;
    let body = resp.text().await?;
    std::fs::write(&cache_path, &body)
        .with_context(|| format!("write cache {}", cache_path.display()))?;
    Ok(body)
}

/// Heuristic: a book is in its "original language" (no translator needed)
/// if no translator role is recorded AND the language matches what the author's
/// native language would plausibly be. We can't reliably detect this from CSV
/// alone, so we adopt a conservative rule: if no translator was found AND the
/// authors_raw mentions "Translator" anywhere (other roles), the book likely
/// needs a translator we missed. Otherwise treat as original-language.
fn looks_like_original_language(candidate: &Candidate, translators: &[Agent]) -> bool {
    if translators.iter().any(|a| a.role == AgentRole::Translator) {
        return false;
    }
    // If the CSV mentions [Translator] anywhere, our extraction missed something.
    if candidate.authors_raw.contains("[Translator]") {
        return false;
    }
    true
}
