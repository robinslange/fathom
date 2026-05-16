use crate::types::GlossaryEntry;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

static PARAPHRASE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?si)PARAPHRASE[^:]*:\s*(.+?)(?:GLOSSARY:|$)").unwrap()
});
static GLOSSARY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?si)GLOSSARY:\s*(.+)").unwrap()
});
static BACKTICK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
static PARENS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(([^)]*)\)").unwrap());

/// Parse a model response into `(paraphrase, glossary)`.
///
/// Handles multiple Gemma output variants — substrate-first, term-first,
/// substrate-only, substrate + parenthetical-english. If a curated
/// `substrate_to_english` map is supplied, backticked terms are resolved
/// to their canonical English from the lexicon.
pub fn parse_response(
    text: &str,
    substrate_to_english: Option<&HashMap<String, String>>,
) -> (String, Vec<GlossaryEntry>) {
    let paraphrase = PARAPHRASE_RE
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| text.trim().to_string());

    let glossary = GLOSSARY_RE
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| parse_glossary(m.as_str(), substrate_to_english))
        .unwrap_or_default();

    (paraphrase, glossary)
}

fn parse_glossary(
    text: &str,
    substrate_to_english: Option<&HashMap<String, String>>,
) -> Vec<GlossaryEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let stripped = line
            .trim()
            .trim_start_matches(['-', '*', '•'])
            .trim();
        if stripped.is_empty() || !stripped.contains(':') {
            continue;
        }
        let (before, after) = match stripped.split_once(':') {
            Some(parts) => parts,
            None => continue,
        };
        let gloss = after.trim().trim_matches(['*', '"', '\'']).to_string();
        if gloss.is_empty() {
            continue;
        }

        let substrate = BACKTICK_RE
            .captures(before)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let term = match (&substrate, substrate_to_english) {
            (Some(sub), Some(map)) if map.contains_key(sub) => map[sub].clone(),
            _ => {
                let stripped_backticks = BACKTICK_RE.replace_all(before, "");
                let stripped_parens = PARENS_RE.replace_all(&stripped_backticks, "");
                let cleaned: String = stripped_parens
                    .trim()
                    .trim_matches(['*', '"', '\''])
                    .to_string();
                if !cleaned.is_empty() {
                    cleaned
                } else {
                    substrate.clone().unwrap_or_default()
                }
            }
        };

        if !term.is_empty() || substrate.is_some() {
            entries.push(GlossaryEntry {
                term,
                gloss,
                substrate_term: substrate,
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_response() {
        let raw = "PARAPHRASE:\nSome things are within our control.\n\nGLOSSARY:\n- in our power (`eph' hēmin`): what is genuinely up to us\n";
        let (para, gloss) = parse_response(raw, None);
        assert!(para.contains("Some things"));
        assert_eq!(gloss.len(), 1);
        assert_eq!(gloss[0].substrate_term.as_deref(), Some("eph' hēmin"));
        assert_eq!(gloss[0].term, "in our power");
    }

    #[test]
    fn parses_substrate_first_gemma_variant() {
        let raw = "PARAPHRASE:\nFoo.\n\nGLOSSARY:\n- `eph' hēmin` (in our power): what is genuinely up to us\n";
        let mut map = HashMap::new();
        map.insert("eph' hēmin".to_string(), "in our power".to_string());
        let (_, gloss) = parse_response(raw, Some(&map));
        assert_eq!(gloss[0].term, "in our power");
        assert_eq!(gloss[0].substrate_term.as_deref(), Some("eph' hēmin"));
    }
}
