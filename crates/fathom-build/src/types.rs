//! Shared types across pipeline stages, written to build-state/ as JSON.

use serde::{Deserialize, Serialize};

/// A Gutenberg catalogue row narrowed to philosophy + English. Output of stage 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub gutenberg_id: u32,
    pub title: String,
    pub language: String,
    pub locc: Vec<String>,
    pub subjects: Vec<String>,
    pub bookshelves: Vec<String>,
    /// Authors + translators raw from the CSV, semicolon-separated upstream;
    /// parsed into the Agent structure during enrichment.
    pub authors_raw: String,
    /// Translators parsed inline from the CSV `Authors` field, if any.
    /// Translator entries in the CSV look like `Surname, Given, YYYY-YYYY [Translator]`.
    pub csv_translators: Vec<Agent>,
}

/// A person involved with a work: author, translator, editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub role: AgentRole,
    pub source: AgentSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    Author,
    Translator,
    Editor,
    Illustrator,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSource {
    /// Parsed from pg_catalog.csv Authors field.
    CatalogCsv,
    /// Parsed from per-book pg{id}.rdf <marcrel:trl> block.
    Rdf,
    /// Looked up via Wikidata SPARQL.
    Wikidata,
    /// Looked up via Open Library /authors/KEY.json.
    OpenLibrary,
}

/// Output of stage 2 — translators per book, enriched with RDF lookups for the
/// CSV gaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookTranslators {
    pub gutenberg_id: u32,
    pub translators: Vec<Agent>,
    /// True if the work appears to be in its original language (no translator
    /// needed). Detected from CSV: original-language authors with no
    /// [Translator] role and language matching author's native language.
    pub is_original_language: bool,
    /// True if the RDF reports a marcrel:trl block. False if neither CSV nor
    /// RDF gives us anything — these are the gap rows for Wikidata/OpenLibrary.
    pub rdf_has_translator: bool,
}

/// Output of stage 3 — the books that pass the NZ life+50 filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filtered {
    pub gutenberg_id: u32,
    pub title: String,
    pub locc: Vec<String>,
    pub translators: Vec<Agent>,
    pub reason: FilterReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterReason {
    /// Translator d. ≤ 1975 → PD in NZ.
    TranslatorPublicDomain,
    /// Original-language work (no translator) where author d. ≤ 1975.
    OriginalLanguagePublicDomain,
}
