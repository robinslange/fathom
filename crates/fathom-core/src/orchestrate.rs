use crate::bootstrap::ProgressCallback;
use crate::identify::identify_terms;
use crate::judge;
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
    fathom_with_judge(passage, tier, mode, backend, JudgeMode::Always(None)).await
}

/// How `fathom` interacts with the NLI judge.
pub enum JudgeMode {
    /// Always run the judge. If the model isn't loaded, attempt to load it
    /// (this may download the ONNX on first call). On any judge failure, the
    /// paraphrase still succeeds but `faithfulness` is `None`. The optional
    /// callback receives download progress while the model is being fetched.
    Always(Option<ProgressCallback>),
    /// Skip the judge entirely. `faithfulness` is `None`. Useful when the
    /// caller wants raw paraphrase output without paying judge latency.
    Skip,
}

/// Lower-level entry point: same as `fathom` but with explicit judge control.
pub async fn fathom_with_judge(
    passage: impl Into<Passage>,
    tier: Tier,
    mode: Mode,
    backend: &dyn Backend,
    judge_mode: JudgeMode,
) -> Result<FathomResult> {
    let passage = passage.into();
    let audience = tier.audience();
    let model = backend.model_label().to_string();

    let mut result = if matches!(mode, Mode::Auto | Mode::Curated) {
        match lookup_canonical(&passage.text) {
            Some(entry) => gloss_curated(passage, entry, audience, tier, model, backend).await?,
            None if matches!(mode, Mode::Curated) => {
                return Err(anyhow!(
                    "no curated substrate found for this passage; try Mode::Auto or Mode::Jit"
                ));
            }
            None => fall_through_after_curated(passage, audience, tier, mode, model, backend).await?,
        }
    } else if matches!(mode, Mode::Jit) {
        let terms = identify_terms(&passage.text, backend).await?;
        if terms.is_empty() {
            gloss_no_substrate(passage, audience, tier, model, backend).await?
        } else {
            gloss_with_identified_terms(passage, terms, audience, tier, model, backend).await?
        }
    } else {
        gloss_no_substrate(passage, audience, tier, model, backend).await?
    };

    if let JudgeMode::Always(progress) = judge_mode {
        result.faithfulness = run_judge(&result.passage.text, &result.paraphrase, progress).await;
        result.faithfulness_verdict = result.faithfulness.as_ref().map(|f| f.verdict());
    }

    Ok(result)
}

async fn fall_through_after_curated(
    passage: Passage,
    audience: &str,
    tier: Tier,
    mode: Mode,
    model: String,
    backend: &dyn Backend,
) -> Result<FathomResult> {
    if matches!(mode, Mode::Auto) {
        let terms = identify_terms(&passage.text, backend).await?;
        if !terms.is_empty() {
            return gloss_with_identified_terms(passage, terms, audience, tier, model, backend)
                .await;
        }
    }
    gloss_no_substrate(passage, audience, tier, model, backend).await
}

/// Graceful-degrade judge call. Returns `Some(score)` on success, `None` on
/// any failure (model not yet on disk, download failed, inference failed).
/// The paraphrase pipeline is never broken by judge failure.
async fn run_judge(
    original: &str,
    paraphrase: &str,
    progress: Option<ProgressCallback>,
) -> Option<crate::types::FaithfulnessScore> {
    if let Err(e) = judge::ensure_loaded(progress).await {
        eprintln!("fathom: NLI judge unavailable, faithfulness skipped: {e:#}");
        return None;
    }
    match judge::score_paraphrase(original, paraphrase) {
        Ok(score) => Some(score),
        Err(e) => {
            eprintln!("fathom: NLI scoring failed: {e:#}");
            None
        }
    }
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
        faithfulness: None,
        faithfulness_verdict: None,
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
        faithfulness: None,
        faithfulness_verdict: None,
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
        faithfulness: None,
        faithfulness_verdict: None,
    })
}

/// Library paraphrase entry point: curated path with a substrate map built
/// from the whole enlarged lexicon (not a single per-passage match). The
/// caller is expected to have already snapped the selection to a sentence
/// boundary; this fn just paraphrases what it receives.
pub async fn fathom_with_global_substrate(
    passage: impl Into<Passage>,
    tier: Tier,
    backend: &dyn Backend,
    substrate_map: &std::collections::BTreeMap<String, crate::lexicon::TermEntry>,
    judge_mode: JudgeMode,
) -> Result<FathomResult> {
    let passage = passage.into();
    let audience = tier.audience();
    let model = backend.model_label().to_string();

    let mut substrate_lines = Vec::with_capacity(substrate_map.len());
    let mut substrate_to_english = HashMap::new();
    for (english, info) in substrate_map {
        substrate_lines.push(format!(
            "- \"{}\" → `{}`: {}",
            english, info.substrate, info.gloss
        ));
        substrate_to_english.insert(info.substrate.clone(), english.clone());
    }
    let prompt = render(
        CURATED_PROMPT,
        &[
            ("author", "(library passage)"),
            ("audience", audience),
            ("substrate", &substrate_lines.join("\n")),
            ("passage", &passage.text),
        ],
    );
    let raw = backend.generate(&prompt).await?;
    let (paraphrase, glossary) = parse_response(&raw, Some(&substrate_to_english));
    let identified_terms = substrate_map.keys().cloned().collect();

    let mut result = FathomResult {
        passage,
        paraphrase,
        glossary,
        tier,
        resolution: Resolution::Curated,
        model,
        identified_terms,
        faithfulness: None,
        faithfulness_verdict: None,
    };

    if let JudgeMode::Always(progress) = judge_mode {
        result.faithfulness = run_judge(&result.passage.text, &result.paraphrase, progress).await;
        result.faithfulness_verdict = result.faithfulness.as_ref().map(|f| f.verdict());
    }

    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_substrate_map_builds_without_panic() {
        let m = crate::lexicon::global_substrate_map();
        assert!(!m.is_empty(), "seed lexicon should yield at least one substrate entry");
        // Sanity: at least one expected term from the seed (Aristotelian).
        // If the seed changes shape, update this assertion.
        let has_eudaimonia = m.values().any(|v| v.substrate == "eudaimonia");
        assert!(has_eudaimonia, "expected 'eudaimonia' somewhere in the seed lexicon");
    }
}
