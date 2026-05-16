use crate::identify::identify_terms;
use crate::lexicon::{lookup_canonical, LexiconEntry};
use crate::parser::parse_response;
use crate::prompts::{
    render, CURATED_PROMPT, GLOSS_NO_SUBSTRATE_PROMPT, GLOSS_WITH_IDENTIFIED_TERMS_PROMPT,
};
use crate::types::{FathomResult, Mode, Passage, Resolution, Tier};
use anyhow::{anyhow, Result};
use fathom_engine::Backend;
use std::collections::HashMap;

/// Main entry point.
///
/// Routes through the three-tier substrate resolution strategy:
///   `Mode::Auto`         — curated lookup, fall through to JIT, then no-substrate
///   `Mode::Curated`      — curated only; error if no lexicon entry matches
///   `Mode::Jit`          — JIT identification + gloss-with-guard
///   `Mode::NoSubstrate`  — model unaided (weakest fidelity)
pub async fn fathom(
    passage: impl Into<Passage>,
    tier: Tier,
    mode: Mode,
    backend: &dyn Backend,
) -> Result<FathomResult> {
    let passage = passage.into();
    let audience = tier.audience();
    let model = backend.model_label().to_string();

    if matches!(mode, Mode::Auto | Mode::Curated) {
        if let Some(entry) = lookup_canonical(&passage.text) {
            return gloss_curated(passage, entry, audience, tier, model, backend).await;
        }
        if matches!(mode, Mode::Curated) {
            return Err(anyhow!(
                "no curated substrate found for this passage; try Mode::Auto or Mode::Jit"
            ));
        }
    }

    if matches!(mode, Mode::Auto | Mode::Jit) {
        let terms = identify_terms(&passage.text, backend).await?;
        if !terms.is_empty() {
            return gloss_with_identified_terms(passage, terms, audience, tier, model, backend)
                .await;
        }
    }

    gloss_no_substrate(passage, audience, tier, model, backend).await
}

async fn gloss_curated(
    passage: Passage,
    entry: LexiconEntry,
    audience: &str,
    tier: Tier,
    model: String,
    backend: &dyn Backend,
) -> Result<FathomResult> {
    let mut substrate_lines = Vec::with_capacity(entry.passage.terms.len());
    let mut substrate_to_english = HashMap::new();
    for (english, info) in &entry.passage.terms {
        substrate_lines.push(format!(
            "- \"{}\" → `{}`: {}",
            english, info.substrate, info.gloss
        ));
        substrate_to_english.insert(info.substrate.clone(), english.clone());
    }
    let prompt = render(
        CURATED_PROMPT,
        &[
            ("author", &entry.source.author),
            ("audience", audience),
            ("substrate", &substrate_lines.join("\n")),
            ("passage", &passage.text),
        ],
    );
    let raw = backend.generate(&prompt).await?;
    let (paraphrase, glossary) = parse_response(&raw, Some(&substrate_to_english));
    let identified_terms = entry.passage.terms.keys().cloned().collect();
    Ok(FathomResult {
        passage,
        paraphrase,
        glossary,
        tier,
        resolution: Resolution::Curated,
        model,
        identified_terms,
    })
}

async fn gloss_with_identified_terms(
    passage: Passage,
    terms: Vec<String>,
    audience: &str,
    tier: Tier,
    model: String,
    backend: &dyn Backend,
) -> Result<FathomResult> {
    let terms_block = terms
        .iter()
        .map(|t| format!("- \"{}\"", t))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = render(
        GLOSS_WITH_IDENTIFIED_TERMS_PROMPT,
        &[
            ("audience", audience),
            ("terms_list", &terms_block),
            ("passage", &passage.text),
        ],
    );
    let raw = backend.generate(&prompt).await?;
    let (paraphrase, glossary) = parse_response(&raw, None);
    Ok(FathomResult {
        passage,
        paraphrase,
        glossary,
        tier,
        resolution: Resolution::Jit,
        model,
        identified_terms: terms,
    })
}

async fn gloss_no_substrate(
    passage: Passage,
    audience: &str,
    tier: Tier,
    model: String,
    backend: &dyn Backend,
) -> Result<FathomResult> {
    let prompt = render(
        GLOSS_NO_SUBSTRATE_PROMPT,
        &[("audience", audience), ("passage", &passage.text)],
    );
    let raw = backend.generate(&prompt).await?;
    let (paraphrase, glossary) = parse_response(&raw, None);
    Ok(FathomResult {
        passage,
        paraphrase,
        glossary,
        tier,
        resolution: Resolution::NoSubstrate,
        model,
        identified_terms: Vec::new(),
    })
}

impl From<String> for Passage {
    fn from(text: String) -> Self {
        Passage::new(text)
    }
}

impl From<&str> for Passage {
    fn from(text: &str) -> Self {
        Passage::new(text)
    }
}
