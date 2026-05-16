//! Stage 3 — filter.
//!
//! Apply NZ life+50: translator d. ≤ 1975 → PD in NZ.
//! For original-language works (no translator), the *author's* death date drives
//! the same rule. Default-exclude on unresolved.

use crate::fs_state::{candidates_path, filtered_path, read_json, translators_path, write_json};
use crate::types::{AgentRole, BookTranslators, Candidate, FilterReason, Filtered};
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::collections::HashMap;

const NZ_CUTOFF_YEAR: i32 = 1975;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Cutoff year for translator death date. Defaults to NZ life+50 = 1975
    /// (works become PD in NZ on 1 Jan of cutoff+50+1 = 2026).
    #[arg(long, default_value_t = NZ_CUTOFF_YEAR)]
    pub cutoff: i32,
}

pub async fn run(args: Args) -> Result<()> {
    let candidates: Vec<Candidate> = read_json(&candidates_path())
        .with_context(|| "load candidates.json — run catalog-sync first")?;
    let translators: Vec<BookTranslators> = read_json(&translators_path())
        .with_context(|| "load translators.json — run enrich-translators first")?;

    let by_id: HashMap<u32, &Candidate> = candidates.iter().map(|c| (c.gutenberg_id, c)).collect();

    let mut kept: Vec<Filtered> = Vec::new();
    let mut excluded_no_data = 0usize;
    let mut excluded_too_recent = 0usize;
    let mut excluded_missing_candidate = 0usize;

    for bt in &translators {
        let Some(candidate) = by_id.get(&bt.gutenberg_id) else {
            excluded_missing_candidate += 1;
            continue;
        };

        if bt.is_original_language {
            // CSV authors live in authors_raw (csv_translators only holds the
            // translator agents). Re-parse to recover author death years.
            let author_death_year = crate::catalog::parse_authors_field(&candidate.authors_raw)
                .iter()
                .filter(|a| a.role == AgentRole::Author)
                .filter_map(|a| a.death_year)
                .max();

            match author_death_year {
                Some(yr) if yr <= args.cutoff => kept.push(Filtered {
                    gutenberg_id: candidate.gutenberg_id,
                    title: candidate.title.clone(),
                    locc: candidate.locc.clone(),
                    translators: bt.translators.clone(),
                    reason: FilterReason::OriginalLanguagePublicDomain,
                }),
                Some(_) => excluded_too_recent += 1,
                None => excluded_no_data += 1,
            }
            continue;
        }

        // Translated work — apply rule to translator(s).
        let translator_deaths: Vec<i32> = bt
            .translators
            .iter()
            .filter(|a| a.role == AgentRole::Translator)
            .filter_map(|a| a.death_year)
            .collect();

        if translator_deaths.is_empty() {
            // Unresolved — default-exclude per policy.
            excluded_no_data += 1;
            continue;
        }

        // Most-recent translator death is the binding date — if any
        // translator died after the cutoff, the translation is in copyright.
        let latest_death = *translator_deaths.iter().max().unwrap();
        if latest_death <= args.cutoff {
            kept.push(Filtered {
                gutenberg_id: candidate.gutenberg_id,
                title: candidate.title.clone(),
                locc: candidate.locc.clone(),
                translators: bt.translators.clone(),
                reason: FilterReason::TranslatorPublicDomain,
            });
        } else {
            excluded_too_recent += 1;
        }
    }

    let out_path = filtered_path();
    write_json(&out_path, &kept)?;
    eprintln!(
        "filter: kept={} excluded_no_data={} excluded_too_recent={} excluded_missing_candidate={} → {}",
        kept.len(),
        excluded_no_data,
        excluded_too_recent,
        excluded_missing_candidate,
        out_path.display()
    );

    Ok(())
}
