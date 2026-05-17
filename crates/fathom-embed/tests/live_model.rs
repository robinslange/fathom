//! Integration test that loads the real bge-small ONNX from local disk.
//! Skipped unless FATHOM_BGE_MODEL_DIR is set to a directory containing
//! `bge-small.onnx` + `tokenizer.json`.

use fathom_embed::{embed, embed_batch, init_embedder, EMBED_DIMS};
use std::path::PathBuf;

fn model_dir() -> Option<PathBuf> {
    std::env::var("FATHOM_BGE_MODEL_DIR")
        .ok()
        .map(PathBuf::from)
}

fn ensure_init() -> bool {
    let Some(dir) = model_dir() else {
        eprintln!("skipping live model test: set FATHOM_BGE_MODEL_DIR to enable");
        return false;
    };
    let model = dir.join("bge-small.onnx");
    let tokenizer = dir.join("tokenizer.json");
    if !model.exists() || !tokenizer.exists() {
        eprintln!(
            "skipping: missing {} or {}",
            model.display(),
            tokenizer.display()
        );
        return false;
    }
    // OnceCell — only first init succeeds; ignore "already initialised" since
    // cargo test runs tests in parallel within one test binary.
    let _ = init_embedder(&model, &tokenizer);
    true
}

#[test]
fn live_embed_returns_unit_vector() {
    if !ensure_init() {
        return;
    }
    let emb = embed("The unexamined life is not worth living.").expect("embed");
    assert_eq!(emb.dims(), EMBED_DIMS);
    let norm = emb.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-3,
        "expected unit vector, got norm {}",
        norm
    );
}

#[test]
fn live_embed_batch_matches_per_text_embed() {
    if !ensure_init() {
        return;
    }
    let texts = [
        "Virtue is the only good.",
        "The mind is its own place.",
        "All things are full of gods.",
    ];
    let batch = embed_batch(&texts).expect("embed_batch");
    assert_eq!(batch.len(), 3);
    for (i, t) in texts.iter().enumerate() {
        let single = embed(t).expect("single embed");
        for (a, b) in batch[i].vector.iter().zip(single.vector.iter()) {
            assert!(
                (a - b).abs() < 1e-4,
                "batch[{}] vs single diverged at value: {} vs {}",
                i,
                a,
                b
            );
        }
    }
}

#[test]
fn live_embed_semantic_neighbours_are_closer() {
    if !ensure_init() {
        return;
    }
    let stoic = embed("Virtue is sufficient for happiness.").expect("embed");
    let stoic_near = embed("A good life requires only excellence of character.").expect("embed");
    let unrelated = embed("The mitochondria are the powerhouse of the cell.").expect("embed");

    fn cos(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    let sim_near = cos(&stoic.vector, &stoic_near.vector);
    let sim_far = cos(&stoic.vector, &unrelated.vector);
    assert!(
        sim_near > sim_far + 0.1,
        "expected semantic near > far by 0.1: near={} far={}",
        sim_near,
        sim_far
    );
}
