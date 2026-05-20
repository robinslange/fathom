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

/// Default k for RRF. Lower than the literature default (60) to amplify
/// the rank-1 signal in asymmetric retrieval — see spec for the math.
pub const RRF_K_DEFAULT: u32 = 10;

/// Reciprocal rank fusion. Ranks are **1-indexed**: the top doc in a list
/// contributes `1 / (k + 1)`, not `1 / k`. A doc missing from a list
/// contributes 0 from that list.
///
/// `dense_hits` and `bm25_hits` must each be sorted (caller's responsibility);
/// rank within each list is the position in the vector + 1.
pub fn rrf_fuse(dense_hits: &[Hit], bm25_hits: &[Hit], k: u32) -> Vec<Hit> {
    use std::collections::HashMap;
    type Key = (u32, String);
    let k_f = k as f32;
    let mut scores: HashMap<Key, f32> = HashMap::new();
    for (rank0, (gid, cid, _)) in dense_hits.iter().enumerate() {
        let rank = rank0 as f32 + 1.0; // 1-indexed
        *scores.entry((*gid, cid.clone())).or_insert(0.0) += 1.0 / (k_f + rank);
    }
    for (rank0, (gid, cid, _)) in bm25_hits.iter().enumerate() {
        let rank = rank0 as f32 + 1.0;
        *scores.entry((*gid, cid.clone())).or_insert(0.0) += 1.0 / (k_f + rank);
    }
    let mut fused: Vec<Hit> = scores.into_iter().map(|((g, c), s)| (g, c, s)).collect();
    sort_with_lexicographic_tiebreak(&mut fused);
    fused
}

#[cfg(test)]
mod rrf_tests {
    use super::*;

    #[test]
    fn top_doc_in_one_list_scores_one_over_k_plus_one() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9)];
        let bm25: Vec<Hit> = vec![];
        let fused = rrf_fuse(&dense, &bm25, 10);
        assert_eq!(fused.len(), 1);
        let expected = 1.0_f32 / 11.0;
        assert!((fused[0].2 - expected).abs() < 1e-6, "got {} expected {}", fused[0].2, expected);
    }

    #[test]
    fn missing_from_list_contributes_zero() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9), (2, "c2".into(), 0.8)];
        let bm25: Vec<Hit> = vec![(1, "c1".into(), 5.0)]; // only c1 in bm25
        let fused = rrf_fuse(&dense, &bm25, 10);
        // c1: 1/11 + 1/11 = 2/11
        // c2: 1/12 + 0    = 1/12
        let c1 = fused.iter().find(|h| h.0 == 1 && h.1 == "c1").unwrap();
        let c2 = fused.iter().find(|h| h.0 == 2 && h.1 == "c2").unwrap();
        assert!((c1.2 - 2.0_f32 / 11.0).abs() < 1e-6);
        assert!((c2.2 - 1.0_f32 / 12.0).abs() < 1e-6);
        assert_eq!(fused[0].1, "c1"); // c1 ranks first
    }

    #[test]
    fn low_k_amplifies_rank_one_signal() {
        // The whole reason we ship k=10 not k=60.
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9)];
        let bm25: Vec<Hit> = vec![(2, "c2".into(), 5.0)];
        let fused_k10 = rrf_fuse(&dense, &bm25, 10);
        let fused_k60 = rrf_fuse(&dense, &bm25, 60);
        // Both have c1 and c2 at rank 1 in different lists; their scores
        // are equal within a given k. But the absolute score is higher at k=10.
        assert!(fused_k10[0].2 > fused_k60[0].2);
    }

    #[test]
    fn rrf_is_deterministic_across_runs() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9), (2, "c2".into(), 0.8)];
        let bm25: Vec<Hit> = vec![(2, "c2".into(), 5.0), (1, "c1".into(), 4.0)];
        let f1 = rrf_fuse(&dense, &bm25, 10);
        let f2 = rrf_fuse(&dense, &bm25, 10);
        assert_eq!(f1, f2);
    }
}
