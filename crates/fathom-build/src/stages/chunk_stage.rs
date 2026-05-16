//! Stage 5 — chunk.
//!
//! For each fetched EPUB:
//! 1. Open with `rbook`, iterate spine in canonical order.
//! 2. For each spine entry: extract paragraph-level text via roxmltree.
//!    HTML entities (`&mdash;`, `&nbsp;`, etc.) are pre-resolved to Unicode
//!    so the strict XML parser doesn't reject the document.
//! 3. Concatenate paragraphs into a single canonical UTF-8 string with
//!    `\n\n` separators.
//! 4. Canonicalise via `fathom_chunker::normalise::canonicalise`.
//! 5. Chunk via `fathom_chunker::chunk_text`.
//! 6. Write `build-state/chunks/{id}.json` with the canonical text + Chunk[].
//!
//! Section IDs come from the spine index (`s{idx:04}`); paragraph IDs from
//! `fathom-chunker` as `p{idx:06}`. The runtime reader can replay the canonical
//! text directly — no live EPUB parse needed.

use crate::fs_state::{build_state_dir, ensure_dir, filtered_path, read_json, write_json};
use crate::types::Filtered;
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use fathom_chunker::{chunk_text, normalise::canonicalise, Chunk, ChunkerConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, ClapArgs, Default)]
pub struct Args {
    /// Limit to first N books.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Force re-chunk even if build-state/chunks/{id}.json exists.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkedBook {
    pub gutenberg_id: u32,
    pub title: String,
    pub canonical_text: String,
    pub chunks: Vec<Chunk>,
    pub stats: ChunkStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkStats {
    pub spine_items: usize,
    pub paragraphs_extracted: usize,
    pub canonical_text_bytes: usize,
    pub chunks_emitted: usize,
    pub parse_warnings: usize,
}

pub async fn run(args: Args) -> Result<()> {
    let filtered: Vec<Filtered> =
        read_json(&filtered_path()).context("load filtered.json — run filter first")?;
    let target: Vec<&Filtered> = match args.limit {
        Some(n) => filtered.iter().take(n).collect(),
        None => filtered.iter().collect(),
    };
    eprintln!("chunk: {} books", target.len());

    let chunks_dir = build_state_dir().join("chunks");
    ensure_dir(&chunks_dir)?;
    let corpus_dir = build_state_dir().join("corpus");

    let cfg = ChunkerConfig::default();
    let mut total_chunks = 0usize;
    let mut missing = 0usize;
    let mut failed = 0usize;

    for (i, book) in target.iter().enumerate() {
        let epub_path = corpus_dir.join(format!(
            "{}/pg{}.epub",
            book.gutenberg_id, book.gutenberg_id
        ));
        let out_path = chunks_dir.join(format!("{}.json", book.gutenberg_id));
        if !args.force && out_path.exists() {
            continue;
        }
        if !epub_path.exists() {
            missing += 1;
            continue;
        }
        match chunk_one(&epub_path, book, &cfg) {
            Ok(cb) => {
                total_chunks += cb.chunks.len();
                write_json(&out_path, &cb)?;
            }
            Err(e) => {
                eprintln!("  fail pg{}: {}", book.gutenberg_id, e);
                failed += 1;
            }
        }
        if (i + 1) % 25 == 0 {
            eprintln!("  ...{}/{} (chunks {} fail {} missing {})", i + 1, target.len(), total_chunks, failed, missing);
        }
    }

    eprintln!(
        "chunk: done. total_chunks={} failed={} missing={}",
        total_chunks, failed, missing
    );
    Ok(())
}

fn chunk_one(epub_path: &PathBuf, book: &Filtered, cfg: &ChunkerConfig) -> Result<ChunkedBook> {
    let epub = rbook::Epub::open(epub_path).map_err(|e| anyhow::anyhow!("open epub: {e}"))?;

    let mut concat = String::new();
    let mut spine_items = 0usize;
    let mut paragraphs_extracted = 0usize;
    let mut parse_warnings = 0usize;

    let reader = epub.reader();
    for item in reader {
        let content = match item {
            Ok(c) => c,
            Err(_) => {
                parse_warnings += 1;
                continue;
            }
        };
        let xhtml = content.content();
        spine_items += 1;
        let paras = extract_paragraphs(xhtml).unwrap_or_else(|_| {
            parse_warnings += 1;
            Vec::new()
        });
        if is_gutenberg_boilerplate(&paras) {
            continue;
        }
        for p in paras {
            if looks_like_pg_metadata_line(&p) {
                continue;
            }
            if !concat.is_empty() {
                concat.push_str("\n\n");
            }
            concat.push_str(&p);
            paragraphs_extracted += 1;
        }
    }

    let canonical = canonicalise(&concat);
    let chunks = chunk_text(&canonical, cfg);

    Ok(ChunkedBook {
        gutenberg_id: book.gutenberg_id,
        title: book.title.clone(),
        stats: ChunkStats {
            spine_items,
            paragraphs_extracted,
            canonical_text_bytes: canonical.len(),
            chunks_emitted: chunks.len(),
            parse_warnings,
        },
        canonical_text: canonical,
        chunks,
    })
}

/// Resolve HTML entities to Unicode before XML parse, then walk the document
/// for `<p>` elements (case-insensitive). Returns trimmed paragraph text.
pub(crate) fn extract_paragraphs(xhtml: &str) -> Result<Vec<String>> {
    let prepared = resolve_html_entities(xhtml);
    let prepared = strip_doctype(&prepared);
    let doc = roxmltree::Document::parse(&prepared)
        .map_err(|e| anyhow::anyhow!("xml parse: {e}"))?;
    let mut paras = Vec::new();
    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }
        let name = node.tag_name().name();
        if !name.eq_ignore_ascii_case("p") {
            continue;
        }
        let text: String = node
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join(" ");
        let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !trimmed.is_empty() {
            paras.push(trimmed);
        }
    }
    Ok(paras)
}

/// Detect Gutenberg front/back matter spine items by looking at their paragraphs.
/// A spine item is boilerplate if more than half its paragraphs match the PG
/// header pattern (lots of metadata lines like "Title :", "Author :", "eBook #",
/// "Project Gutenberg", "Release date", "Most recently updated") or licence text.
fn is_gutenberg_boilerplate(paras: &[String]) -> bool {
    if paras.is_empty() {
        return false;
    }
    let hits = paras.iter().filter(|p| looks_like_pg_metadata_line(p)).count();
    hits * 2 > paras.len()
}

fn looks_like_pg_metadata_line(p: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "Project Gutenberg",
        "eBook #",
        "Release date :",
        "Most recently updated :",
        "Language :",
        "Title :",
        "Author :",
        "Translator :",
        "Editor :",
        "Credits :",
        "*** START OF",
        "*** END OF",
        "START OF THE PROJECT",
        "END OF THE PROJECT",
        "PROJECT GUTENBERG LICENSE",
        "Updated editions will replace",
        "Section 1.",
        "Section 2.",
        "Section 3.",
        "THE FULL PROJECT GUTENBERG LICENSE",
    ];
    SIGNALS.iter().any(|s| p.contains(s))
}

/// Strip the `<!DOCTYPE ...>` declaration. roxmltree refuses any document
/// containing a DTD, and Gutenberg-generated EPUB XHTML always ships with
/// `<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.1//EN' '...'>`. Since we don't
/// validate against the DTD anyway, removing the declaration is safe.
fn strip_doctype(s: &str) -> String {
    let Some(start) = s.find("<!DOCTYPE") else {
        return s.to_string();
    };
    let after_open = &s[start..];
    let Some(end_rel) = after_open.find('>') else {
        return s.to_string();
    };
    let mut out = String::with_capacity(s.len());
    out.push_str(&s[..start]);
    out.push_str(&after_open[end_rel + 1..]);
    out
}

/// Replace HTML named entities with their Unicode characters so roxmltree
/// (strict XML) doesn't reject the document. We only handle the common ones
/// Gutenberg uses; other named entities pass through as-is and may fail the
/// strict parse — we capture that as a parse warning upstream.
fn resolve_html_entities(s: &str) -> String {
    // Order matters for & — must run last so we don't double-decode.
    const PAIRS: &[(&str, &str)] = &[
        ("&nbsp;", "\u{00A0}"),
        ("&mdash;", "—"),
        ("&ndash;", "–"),
        ("&hellip;", "…"),
        ("&lsquo;", "‘"),
        ("&rsquo;", "’"),
        ("&ldquo;", "“"),
        ("&rdquo;", "”"),
        ("&copy;", "©"),
        ("&trade;", "™"),
        ("&reg;", "®"),
        ("&times;", "×"),
        ("&divide;", "÷"),
        ("&deg;", "°"),
        ("&para;", "¶"),
        ("&sect;", "§"),
        ("&middot;", "·"),
        ("&laquo;", "«"),
        ("&raquo;", "»"),
        ("&iexcl;", "¡"),
        ("&iquest;", "¿"),
        ("&pound;", "£"),
        ("&euro;", "€"),
        ("&yen;", "¥"),
        ("&Agrave;", "À"),
        ("&aacute;", "á"),
        ("&Aacute;", "Á"),
        ("&eacute;", "é"),
        ("&Eacute;", "É"),
        ("&iacute;", "í"),
        ("&oacute;", "ó"),
        ("&uacute;", "ú"),
        ("&ntilde;", "ñ"),
        ("&Ntilde;", "Ñ"),
        ("&ouml;", "ö"),
        ("&uuml;", "ü"),
        ("&auml;", "ä"),
        ("&szlig;", "ß"),
        ("&aring;", "å"),
        ("&oslash;", "ø"),
        ("&aelig;", "æ"),
        ("&ccedil;", "ç"),
    ];
    let mut out = s.to_string();
    for (from, to) in PAIRS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_xhtml_doctype() {
        let s = "<?xml version='1.0'?>\n<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.1//EN' 'foo'>\n<html/>";
        let out = strip_doctype(s);
        assert!(!out.contains("<!DOCTYPE"));
        assert!(out.contains("<html/>"));
    }

    #[test]
    fn passes_through_when_no_doctype() {
        let s = "<?xml version='1.0'?>\n<html/>";
        assert_eq!(strip_doctype(s), s);
    }

    #[test]
    fn resolves_common_entities() {
        let s = "alpha &mdash; beta &nbsp; gamma";
        let out = resolve_html_entities(s);
        assert!(out.contains("—"));
        assert!(out.contains("\u{00A0}"));
        assert!(!out.contains("&mdash;"));
    }

    #[test]
    fn extracts_paragraphs_from_simple_xhtml() {
        let xhtml = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
<h1>Chapter 1</h1>
<p>First paragraph here.</p>
<p>Second paragraph with <em>emphasis</em>.</p>
<p>   </p>
<div><p>Nested paragraph.</p></div>
</body></html>"#;
        let paras = extract_paragraphs(xhtml).unwrap();
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0], "First paragraph here.");
        assert_eq!(paras[1], "Second paragraph with emphasis .");
        assert_eq!(paras[2], "Nested paragraph.");
    }

    #[test]
    fn tolerates_html_entities_in_xhtml() {
        let xhtml = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
<p>Alpha &mdash; beta.</p>
</body></html>"#;
        let paras = extract_paragraphs(xhtml).unwrap();
        assert_eq!(paras.len(), 1);
        assert!(paras[0].contains("—"));
    }
}
