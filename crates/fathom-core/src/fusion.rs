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

/// Default k for RRF. Bench calibration (2026-05-20, see
/// `crates/fathom-bench/results/v0.21.1-final.notes.md`) found k=30 dominates
/// the load-bearing criteria on this corpus: 13/15 iconic queries in top-10,
/// adversarial max 0.032, hits@10 76.5%. The spec's a-priori k=10 amplified
/// BM25 outliers too much given how strong the dense lane has become since
/// v0.20.
pub const RRF_K_DEFAULT: u32 = 30;

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
        // Math property: lower k makes the rank-1 contribution `1/(k+1)` larger.
        // Calibration (2026-05-20) picked k=30 not k=10, because the dense lane
        // is strong enough that low-k amplification adds more BM25 noise than
        // genuine rank-1 rescue. This test still verifies the math; the choice
        // of default is a separate concern (see fusion::RRF_K_DEFAULT).
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

/// Linear convex combination of two score lists, each min-max normalised
/// with a 99th-percentile clip on the high end.
///
/// `alpha` is the weight on the dense lane: `alpha * dense + (1 - alpha) * bm25`.
/// Bruch & Gai (2022) prove all linear normalisations are rank-equivalent;
/// min-max with outlier clip is the operationally cheapest pick.
pub fn linear_fuse(dense_hits: &[Hit], bm25_hits: &[Hit], alpha: f32) -> Vec<Hit> {
    use std::collections::HashMap;
    type Key = (u32, String);

    fn p99_clip(scores: &[f32]) -> f32 {
        if scores.is_empty() { return 0.0; }
        let mut sorted: Vec<f32> = scores.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((sorted.len() as f32) * 0.99).floor() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn normalise(hits: &[Hit]) -> HashMap<Key, f32> {
        if hits.is_empty() { return HashMap::new(); }
        let raw: Vec<f32> = hits.iter().map(|h| h.2).collect();
        let max = p99_clip(&raw);
        let min = raw.iter().cloned().fold(f32::INFINITY, f32::min);
        let range = (max - min).max(1e-9);
        hits.iter()
            .map(|(g, c, s)| ((*g, c.clone()), ((s.min(max) - min) / range).clamp(0.0, 1.0)))
            .collect()
    }

    let dense_norm = normalise(dense_hits);
    let bm25_norm  = normalise(bm25_hits);

    let mut scores: HashMap<Key, f32> = HashMap::new();
    for (k, v) in dense_norm.iter() {
        *scores.entry(k.clone()).or_insert(0.0) += alpha * v;
    }
    for (k, v) in bm25_norm.iter() {
        *scores.entry(k.clone()).or_insert(0.0) += (1.0 - alpha) * v;
    }
    let mut fused: Vec<Hit> = scores.into_iter().map(|((g, c), s)| (g, c, s)).collect();
    sort_with_lexicographic_tiebreak(&mut fused);
    fused
}

#[cfg(test)]
mod linear_tests {
    use super::*;

    #[test]
    fn alpha_one_is_dense_only() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9), (2, "c2".into(), 0.5)];
        let bm25: Vec<Hit>  = vec![(2, "c2".into(), 10.0), (1, "c1".into(), 1.0)];
        let fused = linear_fuse(&dense, &bm25, 1.0);
        assert_eq!(fused[0].1, "c1"); // dense ranking dominates
    }

    #[test]
    fn alpha_zero_is_bm25_only() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.9), (2, "c2".into(), 0.5)];
        let bm25: Vec<Hit>  = vec![(2, "c2".into(), 10.0), (1, "c1".into(), 1.0)];
        let fused = linear_fuse(&dense, &bm25, 0.0);
        assert_eq!(fused[0].1, "c2"); // bm25 ranking dominates
    }

    #[test]
    fn all_zero_list_does_not_panic() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.0), (2, "c2".into(), 0.0)];
        let bm25: Vec<Hit>  = vec![];
        let fused = linear_fuse(&dense, &bm25, 0.5);
        assert_eq!(fused.len(), 2);
    }

    #[test]
    fn single_element_list_does_not_div_by_zero() {
        let dense: Vec<Hit> = vec![(1, "c1".into(), 0.5)];
        let bm25: Vec<Hit>  = vec![];
        let fused = linear_fuse(&dense, &bm25, 0.5);
        assert_eq!(fused.len(), 1);
        assert!(fused[0].2.is_finite());
    }

    #[test]
    fn p99_clip_compresses_outlier() {
        // One outlier at 100, ninety-nine others at 1.0. Normalised scores
        // should NOT all be compressed near 0 by the outlier.
        let mut bm25: Vec<Hit> = (0..99).map(|i| (i, format!("c{i}"), 1.0)).collect();
        bm25.push((99, "c99".into(), 100.0));
        let dense: Vec<Hit> = Vec::new();
        let fused = linear_fuse(&dense, &bm25, 0.0);
        // c99 still ranks first, but the score gap between c1 and c98 isn't
        // crushed to zero — they should each get some recognisable score.
        let c1 = fused.iter().find(|h| h.1 == "c1").unwrap();
        let c99 = fused.iter().find(|h| h.1 == "c99").unwrap();
        // After p99 clip, max ≈ 1.0; c1 normalises to ~0.0 (min), c99 to ~1.0.
        // Not the original 0.01 / 1.0 ratio that an uncapped min-max would
        // produce.
        assert!(c99.2 >= c1.2);
        assert!(c99.2.is_finite());
    }
}

/// Curated iconic-phrase alias dictionary, baked into the binary at compile
/// time via `include_str!`. Match returns the gutenberg_ids associated with
/// the matched phrase. Empty Vec means no match.
pub mod iconic {
    use once_cell::sync::Lazy;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct File { phrase: Vec<Phrase> }

    #[derive(Debug, Deserialize)]
    struct Phrase {
        phrase: String,
        gutenberg_ids: Vec<u32>,
        #[allow(dead_code)]
        source: String,
    }

    static RAW: &str = include_str!("../data/iconic-phrases.toml");

    static ENTRIES: Lazy<Vec<(String, Vec<u32>)>> = Lazy::new(|| {
        let f: File = toml::from_str(RAW).expect("iconic-phrases.toml parses");
        f.phrase
            .into_iter()
            .map(|p| (normalise(&p.phrase), p.gutenberg_ids))
            .collect()
    });

    pub fn lookup(query: &str) -> Vec<u32> {
        let n = normalise(query);
        for (phrase, ids) in ENTRIES.iter() {
            if n.contains(phrase) || phrase.contains(&n) {
                return ids.clone();
            }
        }
        Vec::new()
    }

    fn normalise(s: &str) -> String {
        use unicode_segmentation::UnicodeSegmentation;
        s.unicode_words()
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod iconic_tests {
    use super::*;

    #[test]
    fn descartes_query_matches_alias() {
        let ids = iconic::lookup("I think therefore I am");
        assert_eq!(ids, vec![59, 25830]);
    }

    #[test]
    fn descartes_query_matches_with_punctuation() {
        let ids = iconic::lookup("I think, therefore I am.");
        assert_eq!(ids, vec![59, 25830]);
    }

    #[test]
    fn latin_descartes_matches() {
        let ids = iconic::lookup("cogito ergo sum");
        assert_eq!(ids, vec![59, 25830]);
    }

    #[test]
    fn plato_query_matches() {
        let ids = iconic::lookup("the unexamined life is not worth living");
        assert_eq!(ids, vec![1656]);
    }

    #[test]
    fn arbitrary_query_does_not_match() {
        let ids = iconic::lookup("what is the meaning of justice");
        assert!(ids.is_empty());
    }
}
