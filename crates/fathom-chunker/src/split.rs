//! Paragraph + sentence splitting against canonical text.

use crate::approx_tokens;
use unicode_segmentation::UnicodeSegmentation;

/// Returns paragraphs as (text, byte_start, byte_end) tuples — offsets into the
/// canonical text.
pub fn paragraphs(canonical_text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for raw_para in canonical_text.split("\n\n") {
        let start = cursor;
        let end = cursor + raw_para.len();

        let leading = raw_para.len() - raw_para.trim_start().len();
        let trailing = raw_para.len() - raw_para.trim_end().len();
        let trimmed_start = start + leading;
        let trimmed_end = end - trailing;
        let trimmed = &canonical_text[trimmed_start..trimmed_end];

        if !trimmed.is_empty() {
            out.push((trimmed.to_string(), trimmed_start, trimmed_end));
        }
        cursor = end + 2; // skip "\n\n"
    }
    out
}

/// A chunk-like sub-paragraph produced by sentence-boundary splitting of an
/// overlong paragraph.
pub struct Sub {
    pub text: String,
    pub char_offset_start: usize,
    pub char_offset_end: usize,
    pub token_count: usize,
}

/// Split an overlong paragraph at UAX#29 sentence boundaries, packing sentences
/// greedily until just before max_tokens. The base offset is the paragraph's
/// char_offset_start in the parent canonical text.
pub fn split_overlong(paragraph: &str, base_offset: usize, max_tokens: usize) -> Vec<Sub> {
    let mut out = Vec::new();
    let sentences: Vec<(usize, usize, &str)> = sentence_spans(paragraph)
        .into_iter()
        .filter_map(|(s, e)| {
            // Defensive: clamp to char boundaries. unicode-segmentation should
            // give char-aligned offsets but edge cases around trim_end with
            // multibyte trailing chars have surfaced on the full corpus run.
            let s = floor_char_boundary(paragraph, s);
            let e = ceil_char_boundary(paragraph, e);
            if s >= e {
                return None;
            }
            Some((s, e, &paragraph[s..e]))
        })
        .collect();

    if sentences.is_empty() {
        // No sentences detected; fall back to whole paragraph as one sub.
        let tokens = approx_tokens(paragraph);
        out.push(Sub {
            text: paragraph.to_string(),
            char_offset_start: base_offset,
            char_offset_end: base_offset + paragraph.len(),
            token_count: tokens,
        });
        return out;
    }

    let mut buf_start = sentences[0].0;
    let mut buf_end = sentences[0].1;
    let mut buf_tokens = approx_tokens(sentences[0].2);

    for (s, e, t) in sentences.iter().skip(1) {
        let t_tokens = approx_tokens(t);
        if buf_tokens + t_tokens > max_tokens && buf_tokens > 0 {
            out.push(Sub {
                text: paragraph[buf_start..buf_end].to_string(),
                char_offset_start: base_offset + buf_start,
                char_offset_end: base_offset + buf_end,
                token_count: buf_tokens,
            });
            buf_start = *s;
            buf_end = *e;
            buf_tokens = t_tokens;
        } else {
            buf_end = *e;
            buf_tokens += t_tokens;
        }
    }

    out.push(Sub {
        text: paragraph[buf_start..buf_end].to_string(),
        char_offset_start: base_offset + buf_start,
        char_offset_end: base_offset + buf_end,
        token_count: buf_tokens,
    });

    out
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// UAX#29 sentence spans within a single paragraph — offsets are relative to
/// the paragraph string.
fn sentence_spans(paragraph: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for sentence in paragraph.unicode_sentences() {
        let leading_ws = sentence.len() - sentence.trim_start().len();
        let trailing_ws = sentence.len() - sentence.trim_end().len();
        let start = offset + leading_ws;
        let end = offset + sentence.len() - trailing_ws;
        if start < end {
            spans.push((start, end));
        }
        offset += sentence.len();
    }
    spans
}
