export const PARAS_PER_PAGE = 8;

export type Paragraph = { chunkId: string; byteStart: number; text: string };

export type PageBounds = {
  pageIndex: number;
  pageCount: number;
  paragraphs: Paragraph[];
  startParaIndex: number;
  endParaIndex: number;
};

/**
 * Compute the page bounds for a given page index over the full paragraph list.
 * Clamps pageIndex to [0, pageCount-1]. Empty paragraphs array -> pageCount=0,
 * pageIndex=0, empty paragraphs slice.
 */
export function getPage(paragraphs: readonly Paragraph[], pageIndex: number): PageBounds {
  if (paragraphs.length === 0) {
    return {
      pageIndex: 0,
      pageCount: 0,
      paragraphs: [],
      startParaIndex: 0,
      endParaIndex: 0,
    };
  }

  const pageCount = Math.ceil(paragraphs.length / PARAS_PER_PAGE);
  const clamped = Math.max(0, Math.min(pageIndex, pageCount - 1));
  const startParaIndex = clamped * PARAS_PER_PAGE;
  const endParaIndex = Math.min(startParaIndex + PARAS_PER_PAGE, paragraphs.length);

  return {
    pageIndex: clamped,
    pageCount,
    paragraphs: paragraphs.slice(startParaIndex, endParaIndex) as Paragraph[],
    startParaIndex,
    endParaIndex,
  };
}

/**
 * Find which page contains the chunk with `chunkId`. Returns 0 if not found.
 */
export function pageForChunk(paragraphs: readonly Paragraph[], chunkId: string): number {
  const idx = paragraphs.findIndex((p) => p.chunkId === chunkId);
  if (idx === -1) return 0;
  return Math.floor(idx / PARAS_PER_PAGE);
}

/**
 * Find which page contains the byte offset. Returns 0 if not found.
 */
export function pageForByteOffset(paragraphs: readonly Paragraph[], byteOffset: number): number {
  if (paragraphs.length === 0) return 0;
  // Find the last paragraph whose byteStart <= byteOffset
  let bestIdx = -1;
  for (let i = 0; i < paragraphs.length; i++) {
    if (paragraphs[i].byteStart <= byteOffset) {
      bestIdx = i;
    } else {
      break;
    }
  }
  if (bestIdx === -1) return 0;
  return Math.floor(bestIdx / PARAS_PER_PAGE);
}
