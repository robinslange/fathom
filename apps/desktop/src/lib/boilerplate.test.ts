import { describe, expect, it } from "vitest";
import { isBoilerplate } from "./boilerplate.js";

describe("isBoilerplate", () => {
  const cruft = [
    "Other information and formats : www.gutenberg.org/ebooks/10214",
    "This eBook was prepared by Les Bowler, St. Ives, Dorset.",
    "Contributor : George Creel",
    "Author of introduction, etc. : F. C. S. Schiller",
    "*** END OF THE PROJECT GUTENBERG EBOOK THE REPUBLIC ***",
    "*** START OF THIS PROJECT GUTENBERG EBOOK PHAEDO ***",
  ];
  const content = [
    "Socrates was a man who held that the unexamined life is not worth living.",
    "CHAPTER I — Of the State of Nature",
    "BOOK I",
    "Now, whether human life corresponds, or could possibly correspond, to this conception",
    "It is impossible for one in a single volume",
    "I have often wondered by what arguments those who indicted Socrates",
  ];

  for (const s of cruft) {
    it(`detects boilerplate: ${s.slice(0, 40)}…`, () => {
      expect(isBoilerplate(s)).toBe(true);
    });
  }

  for (const s of content) {
    it(`leaves content alone: ${s.slice(0, 40)}…`, () => {
      expect(isBoilerplate(s)).toBe(false);
    });
  }
});
