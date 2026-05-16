//! NLI faithfulness judge.
//!
//! DeBERTa-v3-base-mnli-fever-anli (int8 ONNX, ~270MB) scores per-sentence
//! entailment between the original passage and the paraphrase. Aggregates
//! to the three spike v4 channels.
//!
//! Label order (from the model card): 0 = entailment, 1 = neutral, 2 = contradiction.
//!
//! Design (locked from spike v4 + the NLI-harness research subagent):
//! - Sentence-level alignment (passage-level loses to NLI's 512-token cap)
//! - Unidirectional only (premise = original, hypothesis = paraphrase) —
//!   bidirectional rejected per prior calibration, 4x FPs for one extra TP
//! - For each paraphrase sentence, the score against the original is the
//!   MAX entailment over every original sentence (best-aligned premise)
//! - `support` = mean of those per-paraphrase-sentence entailment maxes
//! - `contradiction_max` = max of per-paraphrase-sentence contradiction
//!   values (using the best-supporting original sentence: the one that
//!   maximised entailment)
//! - `introductions` = paraphrase sentences whose best entailment falls below
//!   the threshold — candidate glosses, not penalties

use crate::bootstrap::{ensure_model_downloaded, ProgressCallback};
use crate::types::FaithfulnessScore;
use anyhow::{anyhow, Context, Result};
use ndarray::Array2;
use once_cell::sync::OnceCell;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::PathBuf;
use std::sync::Mutex;
use tokenizers::Tokenizer;

const ENTAILMENT_IDX: usize = 0;
const CONTRADICTION_IDX: usize = 2;

/// Entailment probability below this for a paraphrase sentence flags it as
/// an "introduction". 0.5 is the spike v4 calibration midpoint.
const SUPPORT_THRESHOLD: f32 = 0.5;

/// DeBERTa is a 512-token model. Pairs longer than this get truncated.
const MAX_SEQ_LEN: usize = 512;

struct JudgeState {
    session: Session,
    tokenizer: Tokenizer,
}

static JUDGE: OnceCell<Mutex<JudgeState>> = OnceCell::new();

/// Load the NLI model and tokenizer on first call (async because it may need
/// to download the ONNX file). Subsequent calls return immediately.
pub async fn ensure_loaded(progress: Option<ProgressCallback>) -> Result<()> {
    if JUDGE.get().is_some() {
        return Ok(());
    }
    let model_path = ensure_model_downloaded("deberta-nli", progress).await?;
    let tokenizer_path = ensure_model_downloaded("deberta-nli-tokenizer", None).await?;
    let state = load_state(&model_path, &tokenizer_path)?;
    let _ = JUDGE.set(Mutex::new(state));
    Ok(())
}

fn load_state(model_path: &PathBuf, tokenizer_path: &PathBuf) -> Result<JudgeState> {
    let mut builder = Session::builder()
        .map_err(|e| anyhow!("creating ORT session builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("configuring ORT optimization level: {e}"))?;
    let session = builder
        .commit_from_file(model_path)
        .map_err(|e| anyhow!("loading ONNX model from {}: {e}", model_path.display()))?;

    let tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| anyhow!("loading tokenizer: {e}"))?;

    Ok(JudgeState { session, tokenizer })
}

/// Score a paraphrase against the original passage.
///
/// Fails if the model isn't yet loaded — callers should `ensure_loaded` first
/// (or accept the failure and surface "verification unavailable" to the user).
pub fn score_paraphrase(original: &str, paraphrase: &str) -> Result<FaithfulnessScore> {
    let judge = JUDGE
        .get()
        .ok_or_else(|| anyhow!("NLI judge not loaded; call ensure_loaded first"))?;
    let mut state = judge
        .lock()
        .map_err(|_| anyhow!("NLI judge mutex poisoned"))?;

    let originals = split_sentences(original);
    let paraphrases = split_sentences(paraphrase);
    if originals.is_empty() || paraphrases.is_empty() {
        return Ok(FaithfulnessScore {
            support: 0.0,
            contradiction_max: 0.0,
            introductions: Vec::new(),
        });
    }

    let mut per_paraphrase_support = Vec::with_capacity(paraphrases.len());
    let mut per_paraphrase_contradiction = Vec::with_capacity(paraphrases.len());
    let mut introductions = Vec::new();

    for hyp in &paraphrases {
        let mut best_entail = 0.0f32;
        let mut contradiction_at_best = 0.0f32;
        for prem in &originals {
            let probs = score_pair(&mut state, prem, hyp)?;
            if probs[ENTAILMENT_IDX] > best_entail {
                best_entail = probs[ENTAILMENT_IDX];
                contradiction_at_best = probs[CONTRADICTION_IDX];
            }
        }
        per_paraphrase_support.push(best_entail);
        per_paraphrase_contradiction.push(contradiction_at_best);
        if best_entail < SUPPORT_THRESHOLD {
            introductions.push(hyp.clone());
        }
    }

    let support = per_paraphrase_support.iter().sum::<f32>() / paraphrases.len() as f32;
    let contradiction_max = per_paraphrase_contradiction
        .iter()
        .copied()
        .fold(0.0f32, f32::max);

    Ok(FaithfulnessScore {
        support,
        contradiction_max,
        introductions,
    })
}

fn score_pair(state: &mut JudgeState, premise: &str, hypothesis: &str) -> Result<[f32; 3]> {
    let encoding = state
        .tokenizer
        .encode((premise, hypothesis), true)
        .map_err(|e| anyhow!("tokenizing pair: {e}"))?;

    let ids = encoding.get_ids();
    let mask = encoding.get_attention_mask();
    let len = ids.len().min(MAX_SEQ_LEN);

    let input_ids: Vec<i64> = ids[..len].iter().map(|&x| x as i64).collect();
    let attention_mask: Vec<i64> = mask[..len].iter().map(|&x| x as i64).collect();

    let ids_arr = Array2::<i64>::from_shape_vec((1, len), input_ids)
        .context("shaping input_ids tensor")?;
    let mask_arr = Array2::<i64>::from_shape_vec((1, len), attention_mask)
        .context("shaping attention_mask tensor")?;

    let ids_tensor =
        Tensor::from_array(ids_arr).map_err(|e| anyhow!("input_ids -> tensor: {e}"))?;
    let mask_tensor =
        Tensor::from_array(mask_arr).map_err(|e| anyhow!("attention_mask -> tensor: {e}"))?;

    let outputs = state
        .session
        .run(ort::inputs![
            "input_ids" => ids_tensor,
            "attention_mask" => mask_tensor,
        ])
        .map_err(|e| anyhow!("ORT inference failed: {e}"))?;

    let logits_view = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| anyhow!("extracting logits as f32: {e}"))?;
    let logits: Vec<f32> = logits_view.iter().copied().collect();
    if logits.len() < 3 {
        return Err(anyhow!("expected 3 logits, got {}", logits.len()));
    }

    Ok(softmax3([logits[0], logits[1], logits[2]]))
}

fn softmax3(logits: [f32; 3]) -> [f32; 3] {
    let m = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps = [
        (logits[0] - m).exp(),
        (logits[1] - m).exp(),
        (logits[2] - m).exp(),
    ];
    let sum = exps[0] + exps[1] + exps[2];
    [exps[0] / sum, exps[1] / sum, exps[2] / sum]
}

/// Naive sentence splitter: split on `.`, `!`, `?` followed by whitespace or
/// end-of-string. Good enough for v1; replace with a real splitter (e.g. the
/// `srx` crate) if the heuristic misses too many cases.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') {
            let next_is_break = match chars.peek() {
                None => true,
                Some(&n) => n.is_whitespace(),
            };
            if next_is_break {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current.clear();
            }
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_sums_to_one() {
        let p = softmax3([1.0, 2.0, 3.0]);
        let sum: f32 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "softmax did not sum to 1: {sum}");
        assert!(p[2] > p[1] && p[1] > p[0]);
    }

    #[test]
    fn softmax_handles_large_logits() {
        // overflow check
        let p = softmax3([1000.0, 1000.0, 1001.0]);
        assert!(p.iter().all(|x| x.is_finite()));
        assert!(p[2] > p[0]);
    }

    #[test]
    fn sentences_split_on_terminators() {
        let s = split_sentences("Hello world. This is a test! Yes? Indeed.");
        assert_eq!(
            s,
            vec![
                "Hello world.",
                "This is a test!",
                "Yes?",
                "Indeed.",
            ]
        );
    }

    #[test]
    fn sentences_handle_trailing_fragment() {
        let s = split_sentences("First. Second");
        assert_eq!(s, vec!["First.", "Second"]);
    }

    #[test]
    fn sentences_skip_empty() {
        let s = split_sentences("   ");
        assert!(s.is_empty());
    }
}
