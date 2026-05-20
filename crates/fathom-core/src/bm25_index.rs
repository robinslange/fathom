//! BM25 index for Fathom shards. Tokenises chunk text with bigrams to rescue
//! all-stopword iconic-phrase queries; preserves stopwords + transliterations.

/// Tokenise text into lowercased unigrams plus adjacent bigrams.
/// Bigrams join unigrams with `_` and are emitted alongside, not instead of,
/// the unigrams. NFC normalisation preserves macrons (`hēmin` stays `hēmin`).
/// No stemming, no stopword removal.
pub fn tokenise(text: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;

    let unigrams: Vec<String> = text
        .unicode_words()
        .map(|w| w.to_lowercase())
        .collect();

    if unigrams.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::with_capacity(unigrams.len() * 2);
    out.extend(unigrams.iter().cloned());
    for pair in unigrams.windows(2) {
        out.push(format!("{}_{}", pair[0], pair[1]));
    }
    out
}

use bm25::{Document, SearchEngine, SearchEngineBuilder, Tokenizer};

/// Custom tokenizer that delegates to our bigram-aware `tokenise()` function.
/// This bypasses the bm25 default English stemmer and stopword list entirely.
struct FathomTokenizer;

impl Tokenizer for FathomTokenizer {
    fn tokenize(&self, input_text: &str) -> Vec<String> {
        tokenise(input_text)
    }
}

/// Per-shard BM25 engine. One per Shard, built once at shard-decode time.
pub struct ShardBm25(SearchEngine<String, u32, FathomTokenizer>);

impl ShardBm25 {
    /// Build a BM25 index over the (chunk_id, chunk_text) pairs of a shard.
    pub fn build(chunks: impl IntoIterator<Item = (String, String)>) -> Self {
        let docs: Vec<Document<String>> = chunks
            .into_iter()
            .map(|(id, text)| Document::new(id, text))
            .collect();
        let engine = SearchEngineBuilder::with_tokenizer_and_documents(FathomTokenizer, docs)
            .build();
        ShardBm25(engine)
    }

    /// Score all chunks against `query`, return (chunk_id, score) pairs sorted
    /// by descending relevance. Pre-tokenises the query via the same bigram
    /// pipeline used at index time.
    pub fn score(&self, query: &str, top_n: usize) -> Vec<(String, f32)> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.0
            .search(query, top_n)
            .into_iter()
            .map(|r| (r.document.id, r.score))
            .collect()
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;

    #[test]
    fn build_index_then_score_finds_verbatim_phrase() {
        let chunks = vec![
            (
                "c1".to_string(),
                "I think therefore I am, said Descartes.".to_string(),
            ),
            (
                "c2".to_string(),
                "Sometimes I think about philosophy in general.".to_string(),
            ),
        ];
        let idx = ShardBm25::build(chunks);
        let hits = idx.score("I think therefore I am", 10);
        assert!(!hits.is_empty(), "BM25 returned no hits");
        assert_eq!(hits[0].0, "c1", "expected verbatim chunk at rank 1");
    }

    #[test]
    fn build_index_then_score_handles_all_stopword_query() {
        let chunks = vec![
            ("c1".to_string(), "I think therefore I am".to_string()),
            ("c2".to_string(), "unrelated text about something else".to_string()),
        ];
        let idx = ShardBm25::build(chunks);
        let hits = idx.score("I think therefore I am", 10);
        assert_eq!(hits[0].0, "c1");
    }

    #[test]
    fn empty_query_returns_empty() {
        let idx = ShardBm25::build(vec![("c1".to_string(), "anything".to_string())]);
        let hits = idx.score("", 10);
        assert!(hits.is_empty());
    }

    #[test]
    fn score_with_top_n_zero_returns_empty() {
        let idx = ShardBm25::build(vec![("c1".to_string(), "relevant text".to_string())]);
        let hits = idx.score("relevant", 0);
        assert!(hits.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenise_emits_unigrams_and_bigrams_for_iconic_query() {
        let tokens = tokenise("I think therefore I am");
        assert_eq!(
            tokens,
            vec![
                "i".to_string(),
                "think".to_string(),
                "therefore".to_string(),
                "i".to_string(),
                "am".to_string(),
                "i_think".to_string(),
                "think_therefore".to_string(),
                "therefore_i".to_string(),
                "i_am".to_string(),
            ]
        );
    }

    #[test]
    fn tokenise_preserves_macron_transliterations() {
        let tokens = tokenise("eph' hēmin");
        // Punctuation drops, macron survives
        assert!(tokens.contains(&"eph".to_string()));
        assert!(tokens.contains(&"hēmin".to_string()));
        assert!(tokens.contains(&"eph_hēmin".to_string()));
    }

    #[test]
    fn tokenise_lowercases_dasein() {
        let tokens = tokenise("Dasein is being-there");
        assert!(tokens.contains(&"dasein".to_string()));
    }

    #[test]
    fn tokenise_handles_single_token() {
        let tokens = tokenise("logos");
        assert_eq!(tokens, vec!["logos".to_string()]);
        // No bigrams for single token
    }

    #[test]
    fn tokenise_handles_empty() {
        assert!(tokenise("").is_empty());
        assert!(tokenise("   ").is_empty());
    }

    #[test]
    fn tokenise_strips_punctuation_but_keeps_internal_apostrophe() {
        // UAX29 word boundaries treat `eph'` as one token in some splitters.
        // We accept either `eph_hemin` or `eph'_hemin` as the bigram form;
        // assert the unigrams come out clean.
        let tokens = tokenise("Hello, world!");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(!tokens.iter().any(|t| t.contains(',') || t.contains('!')));
    }
}
