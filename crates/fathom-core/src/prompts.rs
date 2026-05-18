//! Prompt templates ported from the v3/v5 spike work.
//!
//! Three resolution paths share audience-anchored render style. Substrate
//! handling differs: curated reads from lexicon, JIT identifies then glosses
//! with anti-fabrication guard, no-substrate falls through model-only.
//!
//! Spike v5 confirmed concrete audience targets produce three cleanly
//! separated complexity bands (Flesch-Kincaid 4.5 / 12.2 / 20.6) vs the
//! unusable step function from numeric scales.

pub const CURATED_PROMPT: &str = r#"You are rewriting a passage from {author} for {audience}

The substrate for this passage (use these exactly — do NOT invent others):

{substrate}

PRESERVE every English term-of-art exactly as written in the substrate list. Render the surrounding prose for the audience above.

Respond as:

PARAPHRASE:
[paraphrase for the audience, terms-of-art preserved]

GLOSSARY:
- term (`substrate_term`): philosophical meaning

PASSAGE:
"""
{passage}
"""
"#;

pub const IDENTIFY_PROMPT: &str = r#"You are a philosophy reader marking up a passage. Identify English phrases that are translating a technical philosophical concept rather than serving as ordinary English.

A phrase qualifies if a knowledgeable reader would recognise it as load-bearing: removing or paraphrasing it would distort the argument.

Examples that qualify:
- "in our power" (translates Greek `eph' hēmin`)
- "function" (translates Greek `ergon` in Aristotle)
- "Dasein" (German technical term, untranslated)

Examples that do NOT qualify:
- ordinary English filler ("presumably", "perhaps")
- proper nouns
- everyday nouns unless used in a technical sense in this passage

Return ONLY valid JSON in this exact shape:
{"terms": ["phrase 1", "phrase 2", ...]}

PASSAGE:
"""
{passage}
"""
"#;

pub const GLOSS_WITH_IDENTIFIED_TERMS_PROMPT: &str = r#"You are rewriting a passage for {audience}

The terms-of-art in this passage have been identified for you:
{terms_list}

PRESERVE every identified term-of-art exactly as written. Render the surrounding prose for the audience above. For each identified term, give a one-line PHILOSOPHICAL explanation (not a dictionary definition).

IMPORTANT: if you are NOT confident about the underlying Greek, Latin, or German term, OMIT it rather than guess. Wrong Greek is worse than no Greek. Write the gloss in English alone if you are uncertain about the substrate term.

Respond as:

PARAPHRASE:
[paraphrase for the audience, identified terms preserved]

GLOSSARY:
- term: philosophical meaning (substrate term in backticks ONLY if confident)

PASSAGE:
"""
{passage}
"""
"#;

pub const GLOSS_NO_SUBSTRATE_PROMPT: &str = r#"You are rewriting a passage for {audience}

PRESERVE every term-of-art exactly as written. Render the surrounding prose for the audience above. For each preserved term-of-art, give a one-line PHILOSOPHICAL explanation (not a dictionary definition).

ANTI-FABRICATION RULES (these override audience adaptation):
1. Every sentence in your PARAPHRASE must correspond to content in the PASSAGE. Do not introduce new characters, relationships, locations, or events.
2. Do not write meta-commentary about the passage (e.g. "the passage stops there", "this excerpt describes…"). Just rewrite the content.
3. If you are uncertain about a detail, drop it rather than guess. A shorter faithful paraphrase beats a longer one with invented detail.
4. If the passage references a person or relationship (mother, friend, teacher), keep that exact relationship. Do not substitute related ones (mother → father, friend → brother).
5. If you are NOT confident about the underlying Greek, Latin, or German term, OMIT it rather than guess. Wrong Greek is worse than no Greek.

Respond as:

PARAPHRASE:
[paraphrase for the audience, terms-of-art preserved]

GLOSSARY:
- term: philosophical meaning (substrate term in backticks ONLY if confident)

PASSAGE:
"""
{passage}
"""
"#;

/// Tiny template renderer. Replaces `{key}` with `value` for each pair.
/// Lighter than pulling in a templating crate for four prompts.
pub fn render(template: &str, params: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in params {
        s = s.replace(&format!("{{{}}}", k), v);
    }
    s
}
