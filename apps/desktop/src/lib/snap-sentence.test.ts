import { describe, it, expect } from "vitest";
import { snapToSentences } from "./snap-sentence.js";

const PARA = "First sentence. Second one is here. Third sentence ends.";
//            0         1         2         3         4         5
//            0123456789012345678901234567890123456789012345678901234567

describe("snapToSentences", () => {
  it("widens a mid-word selection to the surrounding sentence", () => {
    // "irst" inside "First sentence."
    const s = snapToSentences(PARA, 1, 5);
    expect(PARA.slice(s.start, s.end).trim()).toBe("First sentence.");
  });

  it("widens a selection that spans two sentences to cover both", () => {
    // "tence. Second"
    const s = snapToSentences(PARA, 9, 22);
    expect(PARA.slice(s.start, s.end).trim()).toBe(
      "First sentence. Second one is here.",
    );
  });

  it("collapsed caret returns the containing sentence", () => {
    const s = snapToSentences(PARA, 20, 20);
    expect(PARA.slice(s.start, s.end).trim()).toBe("Second one is here.");
  });

  it("selection exactly aligned with a sentence boundary stays put", () => {
    // "First sentence. " is [0, 16)
    const s = snapToSentences(PARA, 0, 16);
    expect(s.start).toBe(0);
    expect(PARA.slice(s.start, s.end).trim()).toBe("First sentence.");
  });

  it("empty text returns the zero range", () => {
    const s = snapToSentences("", 0, 0);
    expect(s).toEqual({ start: 0, end: 0 });
  });

  it("out-of-bounds offsets clamp to text bounds", () => {
    const s = snapToSentences(PARA, -10, PARA.length + 50);
    expect(s.start).toBe(0);
    expect(s.end).toBe(PARA.length);
  });
});
