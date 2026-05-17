import { describe, it, expect } from "vitest";
import {
  getPage,
  pageForChunk,
  pageForByteOffset,
  PARAS_PER_PAGE,
} from "./pagination.js";
import type { Paragraph } from "./pagination.js";

// ─── factories ────────────────────────────────────────────────────────────────

function makeParagraphs(count: number): Paragraph[] {
  const paras: Paragraph[] = [];
  let byteStart = 0;
  for (let i = 0; i < count; i++) {
    const text = `Paragraph ${i}.`;
    paras.push({ chunkId: `chunk-${i}`, byteStart, text });
    byteStart += text.length + 2; // +2 for \n\n separator
  }
  return paras;
}

// ─── getPage ──────────────────────────────────────────────────────────────────

describe("getPage", () => {
  it("empty paragraphs returns pageCount=0, empty slice", () => {
    const result = getPage([], 0);
    expect(result.pageCount).toBe(0);
    expect(result.paragraphs).toEqual([]);
    expect(result.pageIndex).toBe(0);
    expect(result.startParaIndex).toBe(0);
    expect(result.endParaIndex).toBe(0);
  });

  it("single page when paragraphs.length <= PARAS_PER_PAGE", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE);
    const result = getPage(paras, 0);
    expect(result.pageCount).toBe(1);
    expect(result.paragraphs).toHaveLength(PARAS_PER_PAGE);
    expect(result.pageIndex).toBe(0);
  });

  it("exactly 2 pages when paragraphs.length = PARAS_PER_PAGE * 2", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    const page0 = getPage(paras, 0);
    const page1 = getPage(paras, 1);
    expect(page0.pageCount).toBe(2);
    expect(page0.paragraphs).toHaveLength(PARAS_PER_PAGE);
    expect(page1.pageCount).toBe(2);
    expect(page1.paragraphs).toHaveLength(PARAS_PER_PAGE);
  });

  it("partial last page when paragraphs.length = PARAS_PER_PAGE + 1 (5 paragraphs = 2 pages, last has 1)", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE + 1);
    const page1 = getPage(paras, 1);
    expect(page1.pageCount).toBe(2);
    expect(page1.paragraphs).toHaveLength(1);
  });

  it("pageIndex clamped to 0 when negative", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 3);
    const result = getPage(paras, -5);
    expect(result.pageIndex).toBe(0);
    expect(result.paragraphs[0]).toEqual(paras[0]);
  });

  it("pageIndex clamped to pageCount-1 when too large", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 3);
    const result = getPage(paras, 999);
    expect(result.pageIndex).toBe(2);
    expect(result.paragraphs[0]).toEqual(paras[PARAS_PER_PAGE * 2]);
  });

  it("startParaIndex + endParaIndex correct for middle page", () => {
    // 3 full pages = 12 paragraphs, middle page is index 1
    const paras = makeParagraphs(PARAS_PER_PAGE * 3);
    const result = getPage(paras, 1);
    expect(result.startParaIndex).toBe(PARAS_PER_PAGE);
    expect(result.endParaIndex).toBe(PARAS_PER_PAGE * 2);
    expect(result.paragraphs).toHaveLength(PARAS_PER_PAGE);
    expect(result.paragraphs[0]).toEqual(paras[PARAS_PER_PAGE]);
  });
});

// ─── pageForChunk ─────────────────────────────────────────────────────────────

describe("pageForChunk", () => {
  it("returns the page containing the chunk_id", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    // chunk-5 is index 5, which is on page 1 (indices 4-7)
    expect(pageForChunk(paras, "chunk-5")).toBe(1);
  });

  it("returns 0 when chunk_id not found", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    expect(pageForChunk(paras, "chunk-missing")).toBe(0);
  });

  it("first chunk on a page boundary lands on that page (not the previous one)", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 3);
    // chunk-4 is the first on page 1 (index 4 = PARAS_PER_PAGE)
    expect(pageForChunk(paras, `chunk-${PARAS_PER_PAGE}`)).toBe(1);
    // chunk-3 is the last on page 0
    expect(pageForChunk(paras, `chunk-${PARAS_PER_PAGE - 1}`)).toBe(0);
  });
});

// ─── pageForByteOffset ────────────────────────────────────────────────────────

describe("pageForByteOffset", () => {
  it("returns page containing the byte", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    // paragraph at index 5 (page 1), check middle of that paragraph
    const targetPara = paras[5];
    const midByte = targetPara.byteStart + Math.floor(targetPara.text.length / 2);
    expect(pageForByteOffset(paras, midByte)).toBe(1);
  });

  it("byte before any paragraph returns 0", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    // paras[0].byteStart is 0, so -1 is before any paragraph
    // Use a fresh set with byteStart starting > 0
    const offsetParas: Paragraph[] = paras.map((p) => ({
      ...p,
      byteStart: p.byteStart + 100,
    }));
    expect(pageForByteOffset(offsetParas, 0)).toBe(0);
  });

  it("byte after all paragraphs returns last page", () => {
    const paras = makeParagraphs(PARAS_PER_PAGE * 2);
    const last = paras[paras.length - 1];
    const wayPast = last.byteStart + last.text.length + 999;
    const lastPage = Math.floor((paras.length - 1) / PARAS_PER_PAGE);
    expect(pageForByteOffset(paras, wayPast)).toBe(lastPage);
  });
});
