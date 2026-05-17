//! Parse pg_catalog.csv rows into typed Candidate + Agent structures.
//!
//! Authors field example:
//!   "Plato, 428? BCE-348? BCE; Jowett, Benjamin, 1817-1893 [Translator]"
//!
//! The CSV uses semicolons to separate agents within the single Authors field.
//! Each agent is `Surname, Given, YYYY-YYYY [Role]` where the role marker is
//! optional (default = Author). Year ranges may be BCE/single-year/circa
//! ("?") and we extract only the death year where present.

use crate::types::{Agent, AgentRole, AgentSource, Candidate};
use once_cell::sync::Lazy;
use regex::Regex;

/// Years inside an agent string. We accept `YYYY-YYYY`, `YYYY BCE-YYYY BCE`,
/// `YYYY?-YYYY?`, `YYYY-`, `-YYYY`. Death year is the second number, optional.
static YEAR_RANGE: Lazy<Regex> = Lazy::new(|| {
    // Accepts: `1817-1893`, `428? BCE-348? BCE`, `121-180`, `1800-`, `-1879`
    Regex::new(
        r"(?P<birth>-?\d{1,4})\??\s*(?:BCE|CE)?\s*-\s*(?P<death>-?\d{1,4})?\??\s*(?:BCE|CE)?",
    )
    .unwrap()
});

static ROLE_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[(Translator|Editor|Illustrator|Author|Compiler|Contributor|Commentator)\]")
        .unwrap()
});

/// Parse an `Authors` cell into a list of Agents.
pub fn parse_authors_field(raw: &str) -> Vec<Agent> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    raw.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_single_agent)
        .collect()
}

fn parse_single_agent(entry: &str) -> Agent {
    let role = ROLE_TAG
        .captures(entry)
        .and_then(|c| c.get(1))
        .map(|m| match m.as_str() {
            "Translator" => AgentRole::Translator,
            "Editor" => AgentRole::Editor,
            "Illustrator" => AgentRole::Illustrator,
            "Compiler" | "Contributor" | "Commentator" => AgentRole::Other,
            _ => AgentRole::Author,
        })
        .unwrap_or(AgentRole::Author);

    // Strip the role tag and look for year range.
    let without_role = ROLE_TAG.replace(entry, "").to_string();
    let years = YEAR_RANGE.captures(&without_role).map(|c| {
        let birth = c.name("birth").and_then(|m| m.as_str().parse::<i32>().ok());
        let death = c.name("death").and_then(|m| m.as_str().parse::<i32>().ok());
        (birth, death)
    });

    // BCE detection: if "BCE" appears before the dash for birth, negate.
    let (birth_year, death_year) = match years {
        Some((b, d)) => {
            let has_bce_birth = without_role
                .splitn(2, '-')
                .next()
                .map(|s| s.contains("BCE"))
                .unwrap_or(false);
            let has_bce_death = without_role
                .splitn(2, '-')
                .nth(1)
                .map(|s| s.contains("BCE"))
                .unwrap_or(false);
            (
                b.map(|y| if has_bce_birth { -y.abs() } else { y }),
                d.map(|y| if has_bce_death { -y.abs() } else { y }),
            )
        }
        None => (None, None),
    };

    // Name: everything before the first 4-digit year, role tag stripped.
    let name_end = YEAR_RANGE
        .find(&without_role)
        .map(|m| m.start())
        .unwrap_or(without_role.len());
    let name = without_role[..name_end]
        .trim_end_matches(|c: char| c == ',' || c.is_whitespace())
        .to_string();

    Agent {
        name,
        birth_year,
        death_year,
        role,
        source: AgentSource::CatalogCsv,
    }
}

/// Parse Subjects / LoCC / Bookshelves fields: semicolon-delimited values.
pub fn parse_semi_list(raw: &str) -> Vec<String> {
    raw.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Filter LoCC values: keep B, BC, BD, BH, BJ. Drop BL/BM/BS/BR/BT/BV/BX (religion),
/// BF (psychology — debatable; exclude in v0.2 for tighter philosophy scope), BP.
pub fn is_philosophy_locc(locc_values: &[String]) -> bool {
    const PHILOSOPHY: &[&str] = &["B", "BC", "BD", "BH", "BJ"];
    locc_values
        .iter()
        .any(|v| PHILOSOPHY.iter().any(|p| v == p))
}

/// Build a typed Candidate from raw CSV fields.
pub fn candidate_from_row(
    gutenberg_id: u32,
    title: &str,
    language: &str,
    authors_raw: &str,
    subjects_raw: &str,
    locc_raw: &str,
    bookshelves_raw: &str,
) -> Candidate {
    let all_agents = parse_authors_field(authors_raw);
    let csv_translators: Vec<Agent> = all_agents
        .iter()
        .filter(|a| a.role == AgentRole::Translator)
        .cloned()
        .collect();

    Candidate {
        gutenberg_id,
        title: title.trim().to_string(),
        language: language.trim().to_string(),
        locc: parse_semi_list(locc_raw),
        subjects: parse_semi_list(subjects_raw),
        bookshelves: parse_semi_list(bookshelves_raw),
        authors_raw: authors_raw.to_string(),
        csv_translators,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plato_translator_jowett() {
        let agents = parse_authors_field(
            "Plato, 428? BCE-348? BCE; Jowett, Benjamin, 1817-1893 [Translator]",
        );
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "Plato");
        assert_eq!(agents[0].role, AgentRole::Author);
        // BCE handling: birth ~ -428, death ~ -348
        assert!(agents[0].birth_year.unwrap_or(0) < 0);
        assert!(agents[0].death_year.unwrap_or(0) < 0);

        assert_eq!(agents[1].name, "Jowett, Benjamin");
        assert_eq!(agents[1].role, AgentRole::Translator);
        assert_eq!(agents[1].birth_year, Some(1817));
        assert_eq!(agents[1].death_year, Some(1893));
    }

    #[test]
    fn parses_long_marcus_aurelius() {
        let agents = parse_authors_field(
            "Aurelius, Marcus, Emperor of Rome, 121-180; Long, George, 1800-1879 [Translator]",
        );
        assert_eq!(agents.len(), 2);
        let trans = agents
            .iter()
            .find(|a| a.role == AgentRole::Translator)
            .unwrap();
        assert_eq!(trans.death_year, Some(1879));
    }

    #[test]
    fn parses_descartes_veitch() {
        let agents =
            parse_authors_field("Descartes, René, 1596-1650; Veitch, John, 1829-1894 [Translator]");
        let trans = agents
            .iter()
            .find(|a| a.role == AgentRole::Translator)
            .unwrap();
        assert_eq!(trans.death_year, Some(1894));
        assert_eq!(trans.name, "Veitch, John");
    }

    #[test]
    fn handles_no_translator() {
        let agents = parse_authors_field("Burnett, Frances Hodgson, 1849-1924");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].role, AgentRole::Author);
    }

    #[test]
    fn handles_empty_authors() {
        assert!(parse_authors_field("").is_empty());
    }

    #[test]
    fn philosophy_locc_filter() {
        assert!(is_philosophy_locc(&["B".to_string()]));
        assert!(is_philosophy_locc(&["BJ".to_string()]));
        assert!(is_philosophy_locc(&["BD".to_string()]));
        assert!(!is_philosophy_locc(&["BL".to_string()])); // religion
        assert!(!is_philosophy_locc(&["BS".to_string()])); // bible
        assert!(!is_philosophy_locc(&["PR".to_string()])); // literature
    }
}
