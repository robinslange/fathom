use include_dir::{include_dir, Dir};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

static LEXICON_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../lexicon");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Source {
    pub title: String,
    pub author: String,
    pub translation: String,
    pub language: String,
    #[serde(default)]
    pub tradition: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TermEntry {
    pub substrate: String,
    pub gloss: String,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassageEntry {
    pub id: String,
    pub fingerprint: String,
    pub terms: BTreeMap<String, TermEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LexiconFile {
    pub source: Source,
    pub passages: Vec<PassageEntry>,
}

#[derive(Debug, Clone)]
pub struct LexiconEntry {
    pub source: Source,
    pub passage: PassageEntry,
    pub file: String,
}

fn normalise(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

static LEXICON: Lazy<Vec<LexiconEntry>> = Lazy::new(|| {
    let mut entries = Vec::new();
    for file in LEXICON_DIR.files() {
        let path = file.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if ext != "yaml" && ext != "yml" {
            continue;
        }
        let Some(contents) = file.contents_utf8() else { continue };
        let parsed: LexiconFile = match serde_yaml::from_str(contents) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("warning: failed to parse {}: {}", path.display(), e);
                continue;
            }
        };
        let file_name = path.to_string_lossy().to_string();
        for passage in parsed.passages {
            entries.push(LexiconEntry {
                source: parsed.source.clone(),
                passage,
                file: file_name.clone(),
            });
        }
    }
    entries
});

/// Match a passage against the bundled lexicon by fingerprint substring.
///
/// Whitespace-normalised, case-folded substring check against the first
/// 300 chars of the input. Robust to translation variants that share a
/// leading phrase; misses if the fingerprint phrase is paraphrased.
pub fn lookup_canonical(passage: &str) -> Option<LexiconEntry> {
    let normalised = normalise(passage);
    let needle = if normalised.len() > 300 {
        &normalised[..300]
    } else {
        &normalised
    };
    for entry in LEXICON.iter() {
        let fp = normalise(&entry.passage.fingerprint);
        if needle.contains(&fp) {
            return Some(entry.clone());
        }
    }
    None
}

/// All loaded entries — useful for stats, the desktop app's "browse lexicon" view, and tests.
pub fn all_entries() -> &'static [LexiconEntry] {
    &LEXICON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexicon_loads_at_least_one_entry() {
        assert!(!all_entries().is_empty(), "lexicon should embed at least one entry");
    }

    #[test]
    fn enchiridion_one_is_findable() {
        let passage = "Of things some are in our power, and others are not. \
            In our power are opinion, movement towards a thing, desire, aversion.";
        let entry = lookup_canonical(passage);
        assert!(entry.is_some(), "Enchiridion §1 should be findable by fingerprint");
        let entry = entry.unwrap();
        assert_eq!(entry.source.author, "Epictetus");
    }
}
