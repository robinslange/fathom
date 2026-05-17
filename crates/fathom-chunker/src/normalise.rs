//! Canonicalise raw text from heterogeneous sources (Gutenberg plain text,
//! Standard Ebooks XHTML, SuttaCentral JSON, user PDFs) into a single UTF-8
//! representation that chunking + reader rendering both work against.
//!
//! Rules:
//! 1. Unify line endings (CRLF → LF).
//! 2. Re-join words split across line breaks: `conver-\nsation` → `conversation`.
//! 3. Strip leading paragraph markers (`I.`, `1.`, `§5.`).
//! 4. Collapse runs of horizontal whitespace inside a paragraph to single spaces.
//! 5. Preserve `\n\n` paragraph breaks (chunker depends on them).

use once_cell::sync::Lazy;
use regex::Regex;

static MARKER_LEADING: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?:[IVXLCDM]+|\d+|§\s*\d+)\.\s+").unwrap());

static HYPHEN_LINE_BREAK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\w)-\n\s*(\w)").unwrap());

static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());

static MULTI_NEWLINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

/// Canonicalise heterogeneous source text. Output is the text whose UTF-8 byte
/// offsets will be recorded in the chunks.
pub fn canonicalise(raw: &str) -> String {
    // Step 1: unify line endings.
    let s = raw.replace("\r\n", "\n");

    // Step 2: re-join hyphen-line-breaks. This must run before paragraph splitting
    // would otherwise treat the soft hyphen as a meaningful boundary.
    let s = HYPHEN_LINE_BREAK.replace_all(&s, "$1$2").into_owned();

    // Step 3: strip leading markers from each paragraph. Operate per-paragraph
    // so we don't accidentally strip mid-paragraph numerals.
    let stripped: Vec<String> = s
        .split("\n\n")
        .map(|p| MARKER_LEADING.replace(p, "").into_owned())
        .collect();
    let s = stripped.join("\n\n");

    // Step 4: collapse double-spaces (Tesseract OCR convention) within lines.
    let s = MULTI_SPACE.replace_all(&s, " ").into_owned();

    // Step 5: collapse runs of >2 newlines to exactly 2.
    let s = MULTI_NEWLINE.replace_all(&s, "\n\n").into_owned();

    s.trim().to_string()
}
