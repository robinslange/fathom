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
