//! bge-small embedding pipeline.
//!
//! 384-dim sentence embeddings via BAAI/bge-small-en-v1.5 ONNX. CPU-only
//! execution provider for determinism — build-time output must byte-match
//! runtime output across machines and re-runs.
//!
//! Shared between two callers:
//! - Build pipeline (`fathom-build`): embeds chunked corpus rows into shards
//! - Runtime (Tauri command): embeds user query string for kNN search
//!
//! Model + tokenizer are loaded once into a process-singleton.

use anyhow::{anyhow, bail, Result};
use half::f16;
use ndarray::Array2;
use once_cell::sync::OnceCell;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;

pub const EMBED_DIMS: usize = 384;
pub const EMBED_MODEL_ID: &str = "bge-small-en-v1.5";
pub const MAX_SEQ_LEN: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub model_id: String,
    pub vector: Vec<f32>,
}

impl Embedding {
    pub fn dims(&self) -> usize {
        self.vector.len()
    }
}

struct EmbedderState {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

static STATE: OnceCell<EmbedderState> = OnceCell::new();

/// Initialise the embedder. Must be called once before any `embed()` or
/// `embed_batch()`. Subsequent calls are no-ops (initialised state stands).
pub fn init_embedder(model_path: &Path, tokenizer_path: &Path) -> Result<()> {
    STATE.get_or_try_init(|| {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort Session::builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("set optimization: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("commit model from {:?}: {e}", model_path))?;
        let tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow!("load tokenizer: {}", e))?;
        Ok::<EmbedderState, anyhow::Error>(EmbedderState {
            session: Mutex::new(session),
            tokenizer,
        })
    })?;
    Ok(())
}

fn state() -> Result<&'static EmbedderState> {
    STATE
        .get()
        .ok_or_else(|| anyhow!("embedder not initialised: call init_embedder first"))
}

/// Embed a single text into a 384-dim f32 vector.
pub fn embed(text: &str) -> Result<Embedding> {
    let batch = embed_batch(&[text])?;
    batch
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("embed_batch returned empty result"))
}

/// Embed a batch of texts in one ONNX call. Yields one Embedding per input
/// in order. Inputs longer than MAX_SEQ_LEN tokens are truncated.
pub fn embed_batch(texts: &[&str]) -> Result<Vec<Embedding>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let st = state()?;

    let encodings = st
        .tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| anyhow!("tokenize batch: {}", e))?;

    let batch_size = encodings.len();
    let mut max_len = 0usize;
    for enc in &encodings {
        max_len = max_len.max(enc.get_ids().len());
    }
    max_len = max_len.min(MAX_SEQ_LEN);
    if max_len == 0 {
        bail!("all inputs tokenised to zero length");
    }

    let mut input_ids = Array2::<i64>::zeros((batch_size, max_len));
    let mut attention_mask = Array2::<i64>::zeros((batch_size, max_len));
    let token_type_ids = Array2::<i64>::zeros((batch_size, max_len));

    for (row, enc) in encodings.iter().enumerate() {
        let ids = enc.get_ids();
        let mask = enc.get_attention_mask();
        let len = ids.len().min(max_len);
        for col in 0..len {
            input_ids[[row, col]] = ids[col] as i64;
            attention_mask[[row, col]] = mask[col] as i64;
        }
    }

    let attention_mask_for_pool = attention_mask.clone();

    let ids_tensor = Tensor::from_array(input_ids).map_err(|e| anyhow!("input_ids tensor: {e}"))?;
    let mask_tensor =
        Tensor::from_array(attention_mask).map_err(|e| anyhow!("attention_mask tensor: {e}"))?;
    let type_tensor =
        Tensor::from_array(token_type_ids).map_err(|e| anyhow!("token_type_ids tensor: {e}"))?;

    let mut session = st
        .session
        .lock()
        .map_err(|_| anyhow!("session lock poisoned"))?;

    let outputs = session
        .run(ort::inputs![
            "input_ids" => ids_tensor,
            "attention_mask" => mask_tensor,
            "token_type_ids" => type_tensor,
        ])
        .map_err(|e| anyhow!("ONNX session run: {e}"))?;

    let last_hidden = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| anyhow!("extract output as f32 array: {e}"))?;

    let shape = last_hidden.shape().to_vec();
    if shape.len() != 3 {
        bail!(
            "expected 3D tensor (batch, seq, dim); got shape {:?}",
            shape
        );
    }
    let seq_dim = shape[1];
    let hidden_dim = shape[2];
    if hidden_dim != EMBED_DIMS {
        bail!("model emits {} dims; expected {}", hidden_dim, EMBED_DIMS);
    }

    let data: Vec<f32> = last_hidden.iter().copied().collect();

    let mut out = Vec::with_capacity(batch_size);
    for row in 0..batch_size {
        let mut summed = vec![0f32; EMBED_DIMS];
        let mut mask_total = 0f32;
        for tok in 0..seq_dim {
            let mask_val = attention_mask_for_pool[[row, tok]] as f32;
            if mask_val == 0.0 {
                continue;
            }
            mask_total += mask_val;
            let base = (row * seq_dim + tok) * EMBED_DIMS;
            for d in 0..EMBED_DIMS {
                summed[d] += data[base + d] * mask_val;
            }
        }
        if mask_total == 0.0 {
            bail!("row {} has all-zero attention mask", row);
        }
        for d in 0..EMBED_DIMS {
            summed[d] /= mask_total;
        }
        // L2 normalise — cosine similarity callers assume unit vectors.
        let norm = summed.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for d in 0..EMBED_DIMS {
                summed[d] /= norm;
            }
        }
        out.push(Embedding {
            model_id: EMBED_MODEL_ID.to_string(),
            vector: summed,
        });
    }

    Ok(out)
}

/// Pack an f32 vector into f16 bytes. Used by the shard writer to halve
/// on-disk size (384 dims × 2 bytes = 768 bytes per embedding).
pub fn to_f16_bytes(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 2);
    for &x in vector {
        let h = f16::from_f32(x);
        out.extend_from_slice(&h.to_le_bytes());
    }
    out
}

/// Unpack f16 bytes into f32. Used by the runtime shard loader.
pub fn from_f16_bytes(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let h = f16::from_le_bytes([chunk[0], chunk[1]]);
        out.push(h.to_f32());
    }
    out
}

#[cfg(test)]
mod tests;
