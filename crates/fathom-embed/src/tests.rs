use super::*;

#[test]
fn f16_round_trip_preserves_to_known_tolerance() {
    let original: Vec<f32> = (0..EMBED_DIMS)
        .map(|i| (i as f32 / EMBED_DIMS as f32 - 0.5) * 0.5)
        .collect();
    let bytes = to_f16_bytes(&original);
    assert_eq!(bytes.len(), EMBED_DIMS * 2);
    let recovered = from_f16_bytes(&bytes);
    assert_eq!(recovered.len(), EMBED_DIMS);
    for (a, b) in original.iter().zip(recovered.iter()) {
        let diff = (a - b).abs();
        // f16 mantissa is 10 bits → ~3 decimal places at magnitude ~0.25.
        assert!(
            diff < 1e-3,
            "f16 round trip exceeded tolerance: {} vs {} (diff {})",
            a,
            b,
            diff
        );
    }
}

#[test]
fn f16_round_trip_preserves_cosine_similarity() {
    let a: Vec<f32> = (0..EMBED_DIMS).map(|i| (i as f32).sin() * 0.05).collect();
    let b: Vec<f32> = (0..EMBED_DIMS)
        .map(|i| (i as f32 * 0.7).cos() * 0.05)
        .collect();

    fn cos(x: &[f32], y: &[f32]) -> f32 {
        let dot: f32 = x.iter().zip(y).map(|(a, b)| a * b).sum();
        let nx: f32 = x.iter().map(|v| v * v).sum::<f32>().sqrt();
        let ny: f32 = y.iter().map(|v| v * v).sum::<f32>().sqrt();
        dot / (nx * ny + 1e-12)
    }

    let cos_orig = cos(&a, &b);
    let a16 = from_f16_bytes(&to_f16_bytes(&a));
    let b16 = from_f16_bytes(&to_f16_bytes(&b));
    let cos_f16 = cos(&a16, &b16);
    assert!(
        (cos_orig - cos_f16).abs() < 1e-3,
        "{} vs {}",
        cos_orig,
        cos_f16
    );
}

#[test]
fn embed_without_init_returns_error() {
    // This test only runs cleanly when STATE has not been initialised by other
    // tests in the same test binary. cargo's default test runner runs tests in
    // the same binary; we treat this assertion as best-effort.
    // If init_embedder has already been called by another test we can't reliably
    // test the uninit path; skip the assertion in that case.
    if STATE.get().is_none() {
        let result = embed("hello");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not initialised"), "got: {}", msg);
    }
}
