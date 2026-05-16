//! Paragraph-level chunking with UAX#29 sentence boundaries.
//!
//! Shared between two callers:
//! - Build pipeline (`fathom-build`): chunks EPUB-extracted text into shard rows
//! - Runtime user-text path (Tauri): chunks dropped user PDFs/EPUBs/.txt
//!
//! Both must produce byte-identical chunks for the same input or retrieval
//! semantics drift between build-time and runtime. This crate is the single
//! source of truth for that invariant.
//!
//! Rules: normalise → paragraph-split → length-guard with sentence-boundary
//! splitting for overlongs. Char offsets are into the canonical (post-normalise)
//! UTF-8 text; the runtime stores this canonical text and uses offsets for the
//! highlight-to-paraphrase flow.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

pub mod normalise;
pub mod split;

/// One row of chunked text. char offsets are into the *canonical* UTF-8 text
/// (post-normalisation), measured in UTF-8 byte positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub chunk_id: String,
    pub paragraph_id: String,
    pub section_id: Option<String>,
    pub text: String,
    pub char_offset_start: usize,
    pub char_offset_end: usize,
    pub token_count: usize,
}

#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub min_tokens: usize,
    pub max_tokens: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            min_tokens: 50,
            max_tokens: 512,
        }
    }
}

/// Approximate token count via whitespace splitting. bge-small uses a WordPiece
/// tokenizer that produces ~1.3 tokens per word on English prose. Multiplied
/// upstream by callers that care about the WordPiece-exact count.
pub fn approx_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Chunk a canonical UTF-8 text into paragraph-level chunks.
///
/// `canonical_text` must already be normalised — call `normalise::canonicalise`
/// upstream. The text passed in is the same text whose char offsets are recorded
/// in the resulting Chunks.
pub fn chunk_text(canonical_text: &str, config: &ChunkerConfig) -> Vec<Chunk> {
    let paragraphs = split::paragraphs(canonical_text);
    let mut chunks = Vec::new();
    let mut chunk_idx = 0usize;
    let mut pending: Option<(String, usize, usize)> = None;

    for (para_idx, (para_text, para_start, para_end)) in paragraphs.into_iter().enumerate() {
        let tokens = approx_tokens(&para_text);

        if let Some((acc_text, acc_start, _acc_end)) = pending.take() {
            // Accumulating a too-small previous paragraph onto this one.
            let combined = format!("{}\n\n{}", acc_text, para_text);
            let combined_end = para_end;
            let combined_tokens = approx_tokens(&combined);
            if combined_tokens >= config.min_tokens {
                if combined_tokens <= config.max_tokens {
                    chunks.push(Chunk {
                        chunk_id: format!("c{:06}", chunk_idx),
                        paragraph_id: format!("p{:06}", para_idx),
                        section_id: None,
                        text: combined,
                        char_offset_start: acc_start,
                        char_offset_end: combined_end,
                        token_count: combined_tokens,
                    });
                    chunk_idx += 1;
                } else {
                    for sub in split::split_overlong(&combined, acc_start, config.max_tokens) {
                        chunks.push(Chunk {
                            chunk_id: format!("c{:06}", chunk_idx),
                            paragraph_id: format!("p{:06}", para_idx),
                            section_id: None,
                            text: sub.text,
                            char_offset_start: sub.char_offset_start,
                            char_offset_end: sub.char_offset_end,
                            token_count: sub.token_count,
                        });
                        chunk_idx += 1;
                    }
                }
            } else {
                pending = Some((combined, acc_start, combined_end));
            }
            continue;
        }

        if tokens < config.min_tokens {
            pending = Some((para_text, para_start, para_end));
            continue;
        }

        if tokens <= config.max_tokens {
            chunks.push(Chunk {
                chunk_id: format!("c{:06}", chunk_idx),
                paragraph_id: format!("p{:06}", para_idx),
                section_id: None,
                text: para_text,
                char_offset_start: para_start,
                char_offset_end: para_end,
                token_count: tokens,
            });
            chunk_idx += 1;
        } else {
            for sub in split::split_overlong(&para_text, para_start, config.max_tokens) {
                chunks.push(Chunk {
                    chunk_id: format!("c{:06}", chunk_idx),
                    paragraph_id: format!("p{:06}", para_idx),
                    section_id: None,
                    text: sub.text,
                    char_offset_start: sub.char_offset_start,
                    char_offset_end: sub.char_offset_end,
                    token_count: sub.token_count,
                });
                chunk_idx += 1;
            }
        }
    }

    if let Some((text, start, end)) = pending {
        let tokens = approx_tokens(&text);
        chunks.push(Chunk {
            chunk_id: format!("c{:06}", chunk_idx),
            paragraph_id: format!("p{:06}", chunks.len()),
            section_id: None,
            text,
            char_offset_start: start,
            char_offset_end: end,
            token_count: tokens,
        });
    }

    chunks
}

/// Snap a (sel_start, sel_end) selection — measured in UTF-8 byte positions
/// into `chunk_text` — to the enclosing UAX#29 sentence boundaries.
/// Returns `None` if the selection lies outside any sentence span (e.g. all
/// whitespace).
pub fn snap_to_sentence(chunk_text: &str, sel_start: usize, sel_end: usize) -> Option<(usize, usize)> {
    let spans = sentence_spans(chunk_text);
    let containing_start = spans.iter().find(|&&(s, e)| s <= sel_start && sel_start < e).map(|&(s, _)| s);
    let containing_end = spans.iter().rev().find(|&&(s, e)| s < sel_end && sel_end <= e).map(|&(_, e)| e);
    match (containing_start, containing_end) {
        (Some(s), Some(e)) if s <= e => Some((s, e)),
        _ => None,
    }
}

/// UAX#29 sentence spans over `text`, with whitespace trimmed from each end.
/// Returned offsets are guaranteed to land on UTF-8 char boundaries.
pub fn sentence_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for sentence in text.unicode_sentences() {
        let leading_ws = sentence.len() - sentence.trim_start().len();
        let trailing_ws = sentence.len() - sentence.trim_end().len();
        let mut start = offset + leading_ws;
        let mut end = offset + sentence.len() - trailing_ws;
        // Defensive clamp to char boundaries — multibyte trailing chars + odd
        // whitespace counts have surfaced offsets that fail debug-mode slicing.
        while start < text.len() && !text.is_char_boundary(start) {
            start += 1;
        }
        while end > 0 && end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
        if start < end {
            spans.push((start, end));
        }
        offset += sentence.len();
    }
    spans
}

#[cfg(test)]
mod tests;
