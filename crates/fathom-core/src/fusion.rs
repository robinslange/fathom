//! Fusion of dense + BM25 retrieval lists.

/// (gutenberg_id, chunk_id, score) record. We need gutenberg_id in the tiebreak
/// to match the dense path's existing determinism (runtime.rs:388-394).
pub type Hit = (u32, String, f32);

/// Sort hits by score DESC, then gutenberg_id ASC, then chunk_id ASC.
/// Same tiebreak as the existing dense path so fused output is deterministic.
pub fn sort_with_lexicographic_tiebreak(hits: &mut Vec<Hit>) {
    hits.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
}

#[cfg(test)]
mod tiebreak_tests {
    use super::*;

    #[test]
    fn ties_break_by_gutenberg_id_then_chunk_id() {
        let mut hits: Vec<Hit> = vec![
            (10, "c2".into(), 1.0),
            (5,  "c1".into(), 1.0),
            (5,  "c0".into(), 1.0),
            (3,  "c3".into(), 2.0),
        ];
        sort_with_lexicographic_tiebreak(&mut hits);
        assert_eq!(hits[0], (3, "c3".into(), 2.0));
        assert_eq!(hits[1], (5, "c0".into(), 1.0));
        assert_eq!(hits[2], (5, "c1".into(), 1.0));
        assert_eq!(hits[3], (10, "c2".into(), 1.0));
    }

    #[test]
    fn empty_input_is_noop() {
        let mut hits: Vec<Hit> = Vec::new();
        sort_with_lexicographic_tiebreak(&mut hits);
        assert!(hits.is_empty());
    }
}
