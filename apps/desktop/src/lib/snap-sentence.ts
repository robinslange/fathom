/**
 * Snap a character range within a paragraph to whole-sentence boundaries
 * using Intl.Segmenter (UAX#29). The backend does the canonical UAX#29 snap
 * over UTF-8 bytes; this client-side version exists so the user can SEE the
 * snapped range — the visible selection grows to match what we're about to
 * paraphrase, instead of the user dragging "Athe" and getting a paraphrase
 * of the whole sentence with no visual feedback.
 *
 * Inputs are char (UTF-16 code-unit) offsets into a paragraph string. Outputs
 * are likewise char offsets. The DOM mapping (Range ↔ char offsets) lives at
 * the call site.
 */

export type SnappedRange = { start: number; end: number };

/**
 * Find the sentence segment that contains `charOffset`, returning its [start, end)
 * char range. If no segment contains the offset (e.g. trailing whitespace), the
 * nearest segment by char distance wins.
 */
function sentenceContaining(
  text: string,
  charOffset: number,
  segmenter: Intl.Segmenter,
): SnappedRange | null {
  let bestFallback: SnappedRange | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const seg of segmenter.segment(text)) {
    const start = seg.index;
    const end = seg.index + seg.segment.length;
    if (start <= charOffset && charOffset < end) {
      return { start, end };
    }
    const distance =
      charOffset < start ? start - charOffset : charOffset - end;
    if (distance < bestDistance) {
      bestDistance = distance;
      bestFallback = { start, end };
    }
  }
  return bestFallback;
}

/**
 * Snap [startChar, endChar) within `text` outward to whole-sentence boundaries.
 * If startChar === endChar, returns the single sentence containing that point.
 * If Intl.Segmenter is unavailable, returns the raw input unchanged.
 */
export function snapToSentences(
  text: string,
  startChar: number,
  endChar: number,
): SnappedRange {
  if (typeof Intl === "undefined" || typeof Intl.Segmenter === "undefined") {
    return { start: startChar, end: endChar };
  }
  if (text.length === 0) return { start: 0, end: 0 };

  const segmenter = new Intl.Segmenter(undefined, { granularity: "sentence" });

  const lo = Math.max(0, Math.min(startChar, text.length));
  const hi = Math.max(lo, Math.min(endChar, text.length));

  const startSeg = sentenceContaining(text, lo, segmenter);
  // For the end-anchor, use the char before `hi` so that a selection ending
  // exactly at a sentence boundary doesn't grab the next sentence.
  const endAnchor = hi > lo ? hi - 1 : hi;
  const endSeg = sentenceContaining(text, endAnchor, segmenter);

  if (!startSeg && !endSeg) return { start: lo, end: hi };
  const start = startSeg ? startSeg.start : lo;
  const end = endSeg ? endSeg.end : hi;
  return { start, end: Math.max(start, end) };
}
