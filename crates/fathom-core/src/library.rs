//! Lexicon browse API for the v0.1 library.
//!
//! Wraps `lexicon::all_entries()` with summary views and theme/tradition
//! grouping. Read-only; the Tauri commands + CLI library subcommand both
//! consume this.

use crate::lexicon::{all_entries, LexiconEntry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Stable slugs for the v0.1 theme taxonomy. Treat as enum-like: the CLI and
/// Svelte UI compare against these strings.
pub const THEMES: &[(&str, &str)] = &[
    ("freedom-and-fate", "Freedom and fate"),
    ("virtue-and-character", "Virtue and character"),
    ("knowledge-and-doubt", "Knowledge and doubt"),
    ("mind-and-self", "Mind and self"),
    ("suffering-and-loss", "Suffering and loss"),
    ("action-and-impulse", "Action and impulse"),
    ("language-and-meaning", "Language and meaning"),
    ("society-and-justice", "Society and justice"),
    ("transcendence-and-the-absolute", "Transcendence and the absolute"),
];

pub fn theme_label(slug: &str) -> Option<&'static str> {
    THEMES.iter().find(|(s, _)| *s == slug).map(|(_, l)| *l)
}

pub fn known_themes() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|(s, _)| *s)
}

/// Lightweight view of a passage for browse lists. The fingerprint snippet
/// gives the user enough context to pick; full passage data is fetched
/// separately via `get_passage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassageSummary {
    pub id: String,
    pub fingerprint: String,
    pub author: String,
    pub title: String,
    pub tradition: String,
    pub themes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraditionSummary {
    pub tradition: String,
    pub passage_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSummary {
    pub slug: String,
    pub label: String,
    pub passage_count: usize,
}

fn summarise(entry: &LexiconEntry) -> PassageSummary {
    PassageSummary {
        id: entry.passage.id.clone(),
        fingerprint: entry.passage.fingerprint.clone(),
        author: entry.source.author.clone(),
        title: entry.source.title.clone(),
        tradition: entry.source.tradition.clone(),
        themes: entry.passage.themes.clone(),
    }
}

/// All traditions present in the lexicon, with passage counts. Sorted by
/// tradition name for stable display.
pub fn list_traditions() -> Vec<TraditionSummary> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in all_entries() {
        if entry.source.tradition.is_empty() {
            continue;
        }
        *counts.entry(entry.source.tradition.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(tradition, passage_count)| TraditionSummary {
            tradition,
            passage_count,
        })
        .collect()
}

/// All v0.1 themes that have at least one tagged passage. Themes with zero
/// passages are still listed (count 0) so the UI can render the complete
/// taxonomy. Sorted to match `THEMES` declaration order.
pub fn list_themes() -> Vec<ThemeSummary> {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for &(slug, _) in THEMES {
        counts.insert(slug, 0);
    }
    for entry in all_entries() {
        for tag in &entry.passage.themes {
            if let Some((slug, _)) = THEMES.iter().find(|(s, _)| *s == tag.as_str()) {
                *counts.entry(slug).or_insert(0) += 1;
            }
        }
    }
    THEMES
        .iter()
        .map(|(slug, label)| ThemeSummary {
            slug: (*slug).to_string(),
            label: (*label).to_string(),
            passage_count: counts.get(slug).copied().unwrap_or(0),
        })
        .collect()
}

/// Passages belonging to a tradition. Matches `source.tradition` exactly
/// (case-sensitive) — use the slug returned from `list_traditions`.
pub fn list_passages_by_tradition(tradition: &str) -> Vec<PassageSummary> {
    all_entries()
        .iter()
        .filter(|e| e.source.tradition == tradition)
        .map(summarise)
        .collect()
}

/// Passages tagged with a theme slug. Passages without any theme tags are
/// invisible to this view.
pub fn list_passages_by_theme(theme: &str) -> Vec<PassageSummary> {
    all_entries()
        .iter()
        .filter(|e| e.passage.themes.iter().any(|t| t == theme))
        .map(summarise)
        .collect()
}

/// Look up a passage by its YAML `id`. The first match wins; ids are
/// expected to be unique across the lexicon.
pub fn get_passage(id: &str) -> Option<&'static LexiconEntry> {
    all_entries().iter().find(|e| e.passage.id == id)
}

/// All passages in the lexicon, sorted by tradition then author. Useful for
/// "show me everything" views and for the CLI library sweep.
pub fn list_all_passages() -> Vec<PassageSummary> {
    let mut entries: Vec<PassageSummary> = all_entries().iter().map(summarise).collect();
    entries.sort_by(|a, b| {
        a.tradition
            .cmp(&b.tradition)
            .then_with(|| a.author.cmp(&b.author))
            .then_with(|| a.id.cmp(&b.id))
    });
    entries
}

/// Sanity check that every theme tag on every passage matches a known slug.
/// Returns the set of unknown tags found, paired with the passage ids that
/// used them. Empty result means the lexicon is well-tagged.
pub fn audit_unknown_themes() -> BTreeMap<String, Vec<String>> {
    let known: BTreeSet<&str> = THEMES.iter().map(|(s, _)| *s).collect();
    let mut unknown: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in all_entries() {
        for tag in &entry.passage.themes {
            if !known.contains(tag.as_str()) {
                unknown
                    .entry(tag.clone())
                    .or_default()
                    .push(entry.passage.id.clone());
            }
        }
    }
    unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_constants_have_unique_slugs() {
        let mut seen = BTreeSet::new();
        for (slug, _) in THEMES {
            assert!(seen.insert(*slug), "duplicate theme slug: {slug}");
        }
    }

    #[test]
    fn theme_label_round_trips() {
        assert_eq!(
            theme_label("freedom-and-fate"),
            Some("Freedom and fate")
        );
        assert_eq!(theme_label("not-a-real-theme"), None);
    }

    #[test]
    fn list_traditions_finds_some() {
        let traditions = list_traditions();
        assert!(!traditions.is_empty(), "lexicon should have traditions");
        assert!(traditions.iter().any(|t| t.tradition == "Stoic"));
    }

    #[test]
    fn list_themes_includes_full_taxonomy() {
        let themes = list_themes();
        assert_eq!(themes.len(), THEMES.len());
    }

    #[test]
    fn audit_passes_on_clean_lexicon() {
        let unknown = audit_unknown_themes();
        assert!(
            unknown.is_empty(),
            "lexicon contains unknown theme tags: {unknown:?}"
        );
    }
}
