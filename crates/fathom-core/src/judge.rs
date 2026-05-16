//! NLI faithfulness judge.
//!
//! Stub. Real implementation pending — will embed DeBERTa-v3-base-mnli-fever-anli
//! ONNX (~270MB int8) and run via the `ort` crate. Downloaded on first launch
//! into the OS-conventional app data directory, not bundled in the binary.
//!
//! Design (locked from spike v4 + the NLI-harness research subagent):
//! - Sentence-level alignment (passage-level loses to NLI's 512-token cap)
//! - Unidirectional only (premise=original, hypothesis=paraphrase) —
//!   bidirectional rejected per prior calibration, 4x FPs for one extra TP
//! - Three score channels: `support` (mean entailment), `contradiction_max`
//!   (worst sentence-level flip), `introductions` (paraphrase sentences not
//!   entailed by anything in the original — candidate glosses, not penalties)
//! - Separate glossary-presence check on the protected-terms list catches
//!   lexical flattening that NLI is blind to.

use crate::types::FaithfulnessScore;
use anyhow::Result;

pub fn score_paraphrase(_original: &str, _paraphrase: &str) -> Result<FaithfulnessScore> {
    anyhow::bail!("NLI judge not yet wired. See judge.rs design notes.")
}
