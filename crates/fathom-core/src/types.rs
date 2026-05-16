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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaithfulnessScore {
    pub entailment: f32,
    pub contradiction: f32,
    pub neutral: f32,
}

impl FaithfulnessScore {
    pub fn is_faithful(&self) -> bool {
        self.entailment > 0.5 && self.contradiction < 0.1
    }
}
