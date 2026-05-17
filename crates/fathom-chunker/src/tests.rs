use super::*;
use crate::normalise::canonicalise;

#[test]
fn normalise_unifies_line_endings() {
    let raw = "alpha\r\nbeta\r\n\r\ngamma";
    let out = canonicalise(raw);
    assert!(!out.contains('\r'));
    assert!(out.contains("alpha beta") || out.contains("alpha\nbeta"));
}

#[test]
fn normalise_rejoins_hyphen_line_breaks() {
    let raw = "the ease of conver-\nsation is partly lost";
    let out = canonicalise(raw);
    assert!(out.contains("conversation"), "got: {:?}", out);
    assert!(!out.contains("conver-"));
}

#[test]
fn normalise_strips_roman_numeral_markers() {
    let raw = "I. Of my grandfather Verus I have learned\n\nII. Of him that brought me up";
    let out = canonicalise(raw);
    assert!(out.starts_with("Of my grandfather"), "got: {:?}", out);
    assert!(out.contains("Of him that brought me up"));
}

#[test]
fn normalise_strips_numeric_markers() {
    let raw = "1. First section\n\n2. Second section\n\n§ 3. Third";
    let out = canonicalise(raw);
    assert!(out.starts_with("First section"), "got: {:?}", out);
    assert!(out.contains("Second section"));
    assert!(out.contains("Third"));
}

#[test]
fn normalise_collapses_double_spaces() {
    let raw = "self-consciousness  of  Prodicus  and  Hippias";
    let out = canonicalise(raw);
    assert_eq!(out, "self-consciousness of Prodicus and Hippias");
}

#[test]
fn normalise_preserves_paragraph_breaks() {
    let raw = "First paragraph.\n\nSecond paragraph.\n\n\n\nThird paragraph.";
    let out = canonicalise(raw);
    assert_eq!(out, "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.");
}

#[test]
fn chunk_short_paragraphs_get_merged() {
    let cfg = ChunkerConfig { min_tokens: 5, max_tokens: 100 };
    let text = "tiny one.\n\ntiny two.\n\ntiny three four five six seven eight.";
    let chunks = chunk_text(text, &cfg);
    assert_eq!(chunks.len(), 1, "tiny paragraphs should merge until min_tokens reached. got: {:#?}", chunks);
}

#[test]
fn chunk_normal_paragraphs_one_chunk_each() {
    let cfg = ChunkerConfig { min_tokens: 3, max_tokens: 100 };
    let text = "alpha beta gamma delta epsilon zeta.\n\neta theta iota kappa lambda mu.";
    let chunks = chunk_text(text, &cfg);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].text, "alpha beta gamma delta epsilon zeta.");
    assert_eq!(chunks[1].text, "eta theta iota kappa lambda mu.");
}

#[test]
fn chunk_offsets_round_trip_to_source_text() {
    let cfg = ChunkerConfig { min_tokens: 3, max_tokens: 100 };
    let text = "alpha beta gamma delta epsilon zeta.\n\neta theta iota kappa lambda mu.";
    let chunks = chunk_text(text, &cfg);
    for c in &chunks {
        assert_eq!(&text[c.byte_offset_start..c.byte_offset_end], &c.text);
    }
}

#[test]
fn chunk_overlong_splits_at_sentence_boundary() {
    // UAX#29 sentence detection requires `.` followed by space + capital to split.
    // Lowercase-after-period stays one sentence (correct behaviour for abbreviations).
    let cfg = ChunkerConfig { min_tokens: 3, max_tokens: 10 };
    let text = "Alpha beta gamma three four. Delta epsilon zeta seven eight nine. Theta iota kappa lambda mu nu twelve thirteen.";
    let chunks = chunk_text(text, &cfg);
    assert!(chunks.len() >= 2, "overlong should split. got: {:#?}", chunks);
    for c in &chunks {
        assert!(c.text.ends_with('.') || c.text.ends_with('!') || c.text.ends_with('?'),
            "split should land on sentence-ending punctuation. got: {:?}", c.text);
    }
}

#[test]
fn snap_to_sentence_inside_sentence_returns_enclosing_span() {
    let para = "First sentence here. Second sentence here. Third sentence here.";
    // Selection bytes 23..27 = "cond" inside "Second sentence here."
    let snapped = snap_to_sentence(para, 23, 27).expect("should snap");
    assert_eq!(&para[snapped.0..snapped.1], "Second sentence here.");
}

#[test]
fn snap_to_sentence_crossing_sentences_expands_outward() {
    let para = "First sentence here. Second sentence here. Third sentence here.";
    let first_end = para.find(".").unwrap() + 1; // end of "First sentence here."
    let third_start = para.rfind("Third").unwrap();
    let snapped = snap_to_sentence(para, first_end - 3, third_start + 3).expect("should snap");
    let snapped_text = &para[snapped.0..snapped.1];
    assert!(snapped_text.contains("First"));
    assert!(snapped_text.contains("Third"));
}

#[test]
fn snap_to_sentence_on_classical_prose_aurelius_section() {
    let para = "Of my grandfather Verus I have learned to be gentle and meek, and to refrain from all anger and passion. From the fame and memory of him that begot me I have learned both shamefastness and manlike behaviour. Of my mother I have learned to be religious, and bountiful.";
    let mid = para.len() / 2;
    let snapped = snap_to_sentence(para, mid - 5, mid + 5).expect("should snap");
    let snapped_text = &para[snapped.0..snapped.1];
    assert!(snapped_text.ends_with('.'), "snap should end at sentence terminator: {:?}", snapped_text);
}

#[test]
fn sentence_spans_handles_classical_prose() {
    // UAX#29 splits on `. ` + capital. Marker stripped by normalise pass upstream.
    let para = "Of my grandfather Verus I have learned. From the fame and memory I have learned.";
    let spans = sentence_spans(para);
    assert_eq!(spans.len(), 2, "got: {:?}", spans);
}
