use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Simple,
    Standard,
    Scholarly,
}

impl Tier {
    pub fn audience(self) -> &'static str {
        match self {
            Tier::Simple => {
                "a curious 15-year-old encountering philosophy for the first time. \
                 Use short sentences and common words. Avoid jargon."
            }
            Tier::Standard => {
                "a university undergraduate in their first philosophy course. \
                 Standard sentence structure, accessible academic English."
            }
            Tier::Scholarly => {
                "a graduate student or scholar comfortable with technical philosophical prose. \
                 Complex sentence structure, full register, sustained argument."
            }
        }
    }
}

impl Default for Tier {
    fn default() -> Self {
        Tier::Standard
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Curated,
    Jit,
    NoSubstrate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Auto,
    Curated,
    Jit,
    NoSubstrate,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Passage {
    pub text: String,
    #[serde(default)]
    pub source: String,
}

impl Passage {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into().trim().to_string(),
            source: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryEntry {
    pub term: String,
    pub gloss: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substrate_term: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FathomResult {
    pub passage: Passage,
    pub paraphrase: String,
    pub glossary: Vec<GlossaryEntry>,
    pub tier: Tier,
    pub resolution: Resolution,
    pub model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identified_terms: Vec<String>,
    /// `None` if the judge couldn't run (NLI model not yet downloaded, load
    /// failure, etc.). The paraphrase is still useful; the UI surfaces
    /// "verification unavailable" when this is None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faithfulness: Option<FaithfulnessScore>,
}

/// Sentence-aggregated NLI judgment over a paraphrase against the original passage.
/// Unidirectional: premise = original sentence, hypothesis = paraphrase sentence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaithfulnessScore {
    /// Mean per-paraphrase-sentence entailment probability (best-aligned original
    /// sentence). Range 0..=1; higher = better supported.
    pub support: f32,
    /// Worst per-paraphrase-sentence contradiction probability. Range 0..=1;
    /// any value above ~0.1 typically indicates a hard semantic flip somewhere.
    pub contradiction_max: f32,
    /// Paraphrase sentences whose best-aligned original sentence still falls
    /// below the entailment threshold. Candidates for glossing introductions
    /// rather than penalties — the spike validated these as a separate channel.
    pub introductions: Vec<String>,
}

impl FaithfulnessScore {
    /// Heuristic gate: a paraphrase passes if it has strong overall support
    /// and no single sentence flips. Tuned against the spike v4 calibration.
    pub fn is_faithful(&self) -> bool {
        self.support > 0.6 && self.contradiction_max < 0.1
    }
}
