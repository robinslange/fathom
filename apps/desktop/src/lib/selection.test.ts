import { describe, it, expect } from "vitest";
import { endpointToByteOffset } from "./selection.js";
import type { ElementLike, NodeLike } from "./selection.js";

const utf8 = (s: string) => new TextEncoder().encode(s).length;

// ─── factories ────────────────────────────────────────────────────────────────

function textNode(data: string, parent: ElementLike | null = null): NodeLike {
  return { nodeType: 3, data, textContent: data, parentElement: parent };
}

/**
 * Build a paragraph element with a single text-node child (v0.2 flat markup).
 * Sets paraEl = self so the walker can find it.
 */
function paragraph(text: string, byteStart: number): ElementLike {
  const el: ElementLike = {
    nodeType: 1,
    textContent: text,
    dataset: { byteStart: String(byteStart) },
    childNodes: [], // filled below
    paraEl: null,  // filled below (self-reference)
  };
  const child = textNode(text, el);
  (el.childNodes as NodeLike[]).push(child);
  el.paraEl = el;
  return el;
}

/**
 * Build a paragraph element with multiple inline children:
 *   textNode(before) + emNode(middle) + textNode(after)
 * emNode is an element with one text child, paraEl pointing to the paragraph.
 */
function paragraphWithInlines(
  before: string,
  middle: string,
  after: string,
  byteStart: number,
): { para: ElementLike; beforeNode: NodeLike; emNode: ElementLike; afterNode: NodeLike } {
  const para: ElementLike = {
    nodeType: 1,
    textContent: before + middle + after,
    dataset: { byteStart: String(byteStart) },
    childNodes: [],
    paraEl: null,
  };

  const emChild = textNode(middle, null); // parent set below
  const em: ElementLike = {
    nodeType: 1,
    textContent: middle,
    childNodes: [emChild],
    paraEl: para,
    parentElement: para,
  };
  (emChild as { parentElement: ElementLike | null }).parentElement = em;

  const beforeNode = textNode(before, para);
  const afterNode = textNode(after, para);

  (para.childNodes as NodeLike[]).push(beforeNode, em, afterNode);
  para.paraEl = para;

  return { para, beforeNode, emNode: em, afterNode };
}

/** A container that has no paragraph ancestor — simulates a selection in the gutter. */
function elementOutsideParagraphs(): ElementLike {
  return {
    nodeType: 1,
    textContent: "",
    childNodes: [],
    paraEl: null,
  };
}

// ─── Group 1: text-node container, single-text-node paragraph ─────────────────

describe("text-node container — single-text-node paragraph (common case)", () => {
  it("selection inside text node returns paraByteStart + utf8 prefix", () => {
    const p = paragraph("Hello, world!", 100);
    const tn = p.childNodes[0];
    expect(endpointToByteOffset(tn, 5, [], utf8, "start")).toBe(100 + utf8("Hello"));
  });

  it("selection at start of text node (charOffset=0) returns paraByteStart", () => {
    const p = paragraph("Hello", 200);
    const tn = p.childNodes[0];
    expect(endpointToByteOffset(tn, 0, [], utf8, "start")).toBe(200);
  });

  it("selection at end of text node (charOffset=text.length) returns paraByteStart + utf8(full text)", () => {
    const p = paragraph("Hello", 200);
    const tn = p.childNodes[0];
    expect(endpointToByteOffset(tn, 5, [], utf8, "start")).toBe(200 + utf8("Hello"));
  });

  it("multi-byte UTF-8 (em-dash, accented chars) counts bytes correctly", () => {
    // "Héllo" — é is 2 bytes in UTF-8
    const text = "Héllo — world";
    const p = paragraph(text, 0);
    const tn = p.childNodes[0];
    // Select up to and including "Hél" (3 chars, 4 bytes)
    const prefix = "Hél";
    expect(endpointToByteOffset(tn, prefix.length, [], utf8, "start")).toBe(utf8(prefix));
  });
});

// ─── Group 2: element container = paragraph (triple-click) ───────────────────

describe("element container = paragraph (triple-click case)", () => {
  it("triple-click start (container=p, offset=0) returns paraByteStart", () => {
    const p = paragraph("Some text here.", 50);
    expect(endpointToByteOffset(p, 0, [], utf8, "start")).toBe(50);
  });

  it("triple-click end (container=p, offset=childNodes.length) returns paraByteStart + utf8(full text)", () => {
    const text = "Some text here.";
    const p = paragraph(text, 50);
    expect(endpointToByteOffset(p, 1 /* childNodes.length */, [], utf8, "end")).toBe(
      50 + utf8(text),
    );
  });

  it("offset clamped to childNodes.length when it exceeds", () => {
    const text = "Some text.";
    const p = paragraph(text, 0);
    // Pass offset=99 — should clamp to childNodes.length (1 child) → full text
    expect(endpointToByteOffset(p, 99, [], utf8, "end")).toBe(utf8(text));
  });
});

// ─── Group 3: container is outside any paragraph (gutter) ────────────────────

describe("container outside any paragraph (gutter / fallback cases)", () => {
  const paras = [
    { byteStart: 0, text: "First paragraph." },
    { byteStart: 20, text: "Second paragraph." },
    { byteStart: 45, text: "Third paragraph." },
  ];

  it("fallback=start returns paras[0].byteStart", () => {
    const el = elementOutsideParagraphs();
    expect(endpointToByteOffset(el, 0, paras, utf8, "start")).toBe(0);
  });

  it("fallback=end returns last paragraph's byteStart + utf8(last text)", () => {
    const el = elementOutsideParagraphs();
    const last = paras[paras.length - 1];
    expect(endpointToByteOffset(el, 0, paras, utf8, "end")).toBe(
      last.byteStart + utf8(last.text),
    );
  });

  it("fallback=start with empty paras array returns 0", () => {
    const el = elementOutsideParagraphs();
    expect(endpointToByteOffset(el, 0, [], utf8, "start")).toBe(0);
  });

  it("fallback=end with empty paras array returns 0", () => {
    const el = elementOutsideParagraphs();
    expect(endpointToByteOffset(el, 0, [], utf8, "end")).toBe(0);
  });
});

// ─── Group 4: paragraph with multiple children (v0.21 future-proofing) ────────

describe("paragraph with multiple inline children (v0.21 future-proofing)", () => {
  it("container=second text node returns paraByteStart + utf8(before + em) + utf8(prefix in second text)", () => {
    const { para, afterNode } = paragraphWithInlines("Hello ", "world", " and more", 100);
    // afterNode starts after "Hello world" — selecting 4 chars into it (" and")
    expect(endpointToByteOffset(afterNode, 4, [], utf8, "start")).toBe(
      100 + utf8("Hello ") + utf8("world") + utf8(" and"),
    );
  });

  it("container=p, charOffset=2, returns paraByteStart + utf8(first two children's textContent)", () => {
    const { para } = paragraphWithInlines("Hello ", "world", " and more", 100);
    // charOffset=2 means: sum first 2 children (beforeNode + emNode)
    expect(endpointToByteOffset(para, 2, [], utf8, "start")).toBe(
      100 + utf8("Hello ") + utf8("world"),
    );
  });
});
