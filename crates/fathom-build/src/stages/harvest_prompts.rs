//! Single-turn gloss prompt for the harvest stage.
//!
//! The model receives a candidate English phrase + the paragraph it appears in
//! and is asked to return a substrate term + one-line gloss. Output is parsed
//! as strict JSON; parse failures are dropped silently (the harvest's NLI gate
//! later wouldn't accept noise anyway).

use serde::Deserialize;

pub const HARVEST_GLOSS_PROMPT: &str = r#"You are a philosophy translator. Given an English phrase and the paragraph it appears in, identify the original-language technical term (in romanised transliteration if non-Latin script) and write a one-line gloss.

Respond ONLY in JSON with this exact shape:
{"substrate": "<term>", "gloss": "<one-line gloss>"}

If the English phrase is NOT a translation of a substrate technical term (e.g. it's plain English with no specialised philosophical meaning), respond:
{"substrate": null, "gloss": null}

Paragraph:
{{paragraph}}

English phrase: {{phrase}}

JSON:"#;

#[derive(Debug, Clone, Deserialize)]
pub struct HarvestGlossResponse {
    pub substrate: Option<String>,
    pub gloss: Option<String>,
}

pub fn render_gloss_prompt(paragraph: &str, phrase: &str) -> String {
    HARVEST_GLOSS_PROMPT
        .replace("{{paragraph}}", paragraph)
        .replace("{{phrase}}", phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_placeholders() {
        let out = render_gloss_prompt("In our power are opinion and impulse.", "in our power");
        assert!(out.contains("In our power are opinion and impulse."));
        assert!(out.contains("English phrase: in our power"));
        assert!(!out.contains("{{paragraph}}"));
        assert!(!out.contains("{{phrase}}"));
    }

    #[test]
    fn parses_well_formed_json() {
        let r: HarvestGlossResponse =
            serde_json::from_str(r#"{"substrate": "eph' hēmin", "gloss": "up to us"}"#).unwrap();
        assert_eq!(r.substrate.as_deref(), Some("eph' hēmin"));
        assert_eq!(r.gloss.as_deref(), Some("up to us"));
    }

    #[test]
    fn parses_null_response_when_phrase_is_not_substrate() {
        let r: HarvestGlossResponse =
            serde_json::from_str(r#"{"substrate": null, "gloss": null}"#).unwrap();
        assert!(r.substrate.is_none());
        assert!(r.gloss.is_none());
    }
}
