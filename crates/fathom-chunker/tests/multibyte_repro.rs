//! Repro for the multibyte-boundary panic from the full-corpus build.

use fathom_chunker::{chunk_text, normalise::canonicalise, ChunkerConfig};

#[test]
fn handles_smart_quote_inside_overlong_paragraph() {
    // The failing fragment from pg1572 (Plato Timaeus, Jowett).
    // The closing smart quote is U+201D = 3 UTF-8 bytes.
    let para = r#"Hector called him Scamandrius, but the others Astyanax” [continues with many more sentences to force an overlong split].

Now, if the men called him Astyanax, is it not probable that the other name was conferred by the women? And which are more likely to be right—the wiser or the less wise, the men or the women? You will admit that they are; and which one of them is more inclined to use the rational rules of nature, the wisdom of the wise, or the unreasoning impulse of the foolish? Surely the wise know what they speak about, but the unreasoning do not, hence we should trust them more on such matters. And so we should look to those names which the wise have given. And yet a name is no light or insignificant thing—the giver of it has a power that few possess. So we should look to those who can give names properly. And then we may proceed to find the names, and the natures of which they are reflective. And so the philosopher proceeds by careful method. Or by inquiry of those who appear most knowing.

This continues for many more words to make sure the paragraph triggers the overlong path. So I am adding more sentences. And more. And more. Until we are well past the max_tokens limit and the split_overlong path must process this multibyte-containing paragraph.

Final sentence here."#;
    let cfg = ChunkerConfig::default();
    let canonical = canonicalise(para);
    let _chunks = chunk_text(&canonical, &cfg);
}
