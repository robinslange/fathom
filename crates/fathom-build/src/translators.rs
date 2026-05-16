//! RDF translator extraction.
//!
//! Per-book RDF at https://www.gutenberg.org/cache/epub/{id}/pg{id}.rdf
//! contains `<marcrel:trl>` blocks when the catalogue knows the translator.
//! Structure:
//!
//! ```xml
//! <marcrel:trl>
//!   <pgterms:agent rdf:about="2009/agents/94">
//!     <pgterms:name>Jowett, Benjamin</pgterms:name>
//!     <pgterms:birthdate rdf:datatype="...">1817</pgterms:birthdate>
//!     <pgterms:deathdate rdf:datatype="...">1893</pgterms:deathdate>
//!   </pgterms:agent>
//! </marcrel:trl>
//! ```

use crate::types::{Agent, AgentRole, AgentSource};
use anyhow::{Context, Result};
use roxmltree::Document;

/// Extract translator agents from a pg{id}.rdf body. Returns empty Vec if no
/// `<marcrel:trl>` blocks present (which is normal for original-language works
/// or for catalogue gaps).
pub fn parse_translators_from_rdf(xml: &str) -> Result<Vec<Agent>> {
    let doc = Document::parse(xml).context("parse RDF as XML")?;
    let mut out = Vec::new();
    for node in doc.descendants() {
        if !is_named(&node, "trl") {
            continue;
        }
        for agent_node in node.descendants() {
            if !is_named(&agent_node, "agent") {
                continue;
            }
            let mut name = String::new();
            let mut birth = None;
            let mut death = None;
            for child in agent_node.children() {
                if is_named(&child, "name") {
                    name = child.text().unwrap_or("").trim().to_string();
                } else if is_named(&child, "birthdate") {
                    birth = child.text().and_then(|s| s.trim().parse::<i32>().ok());
                } else if is_named(&child, "deathdate") {
                    death = child.text().and_then(|s| s.trim().parse::<i32>().ok());
                }
            }
            if !name.is_empty() {
                out.push(Agent {
                    name,
                    birth_year: birth,
                    death_year: death,
                    role: AgentRole::Translator,
                    source: AgentSource::Rdf,
                });
            }
        }
    }
    Ok(out)
}

fn is_named(node: &roxmltree::Node, local: &str) -> bool {
    node.is_element() && node.tag_name().name() == local
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOWETT_RDF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:pgterms="http://www.gutenberg.org/2009/pgterms/"
         xmlns:marcrel="http://id.loc.gov/vocabulary/relators/">
  <pgterms:ebook>
    <marcrel:trl>
      <pgterms:agent rdf:about="2009/agents/94">
        <pgterms:name>Jowett, Benjamin</pgterms:name>
        <pgterms:birthdate>1817</pgterms:birthdate>
        <pgterms:deathdate>1893</pgterms:deathdate>
      </pgterms:agent>
    </marcrel:trl>
  </pgterms:ebook>
</rdf:RDF>"#;

    #[test]
    fn parses_jowett_block() {
        let agents = parse_translators_from_rdf(JOWETT_RDF).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Jowett, Benjamin");
        assert_eq!(agents[0].death_year, Some(1893));
        assert_eq!(agents[0].role, AgentRole::Translator);
        assert_eq!(agents[0].source, AgentSource::Rdf);
    }

    #[test]
    fn handles_no_translator_block() {
        let no_trl = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:pgterms="http://www.gutenberg.org/2009/pgterms/">
  <pgterms:ebook>
    <pgterms:name>Marcus Aurelius</pgterms:name>
  </pgterms:ebook>
</rdf:RDF>"#;
        let agents = parse_translators_from_rdf(no_trl).unwrap();
        assert!(agents.is_empty());
    }
}
