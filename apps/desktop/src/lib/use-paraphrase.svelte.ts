import { invoke } from "@tauri-apps/api/core";
import { library } from "./use-library.svelte.js";
import { snapToSentences } from "./snap-sentence.js";
import { utf8ByteLength } from "./utf8.js";

export type Tier = "simple" | "standard" | "scholarly";

export type GlossaryEntry = {
  term: string;
  gloss: string;
  substrate_term?: string | null;
};

export type FaithfulnessScore = {
  support: number;
  contradiction_max: number;
  introductions: string[];
};

export type FaithfulnessVerdict = {
  faithful: boolean;
  support_floor: number;
  contradiction_ceiling: number;
};

export type FathomResult = {
  paraphrase: string;
  glossary: GlossaryEntry[];
  tier: Tier;
  resolution: string;
  model: string;
  identified_terms: string[];
  faithfulness?: FaithfulnessScore | null;
  faithfulness_verdict?: FaithfulnessVerdict | null;
};

type SelectionAnchor = {
  paraEl: HTMLElement;
  paraText: string;
  paraByteStart: number;
  snappedStartChar: number;
  snappedEndChar: number;
  selText: string;
  rect: DOMRect;
  gutenbergId: number;
  startByte: number;
  endByte: number;
};

class ParaphraseStore {
  tier = $state<Tier>("standard");
  paraphraseResult = $state<FathomResult | null>(null);
  paraphraseBusy = $state(false);
  paraphraseError = $state<string | null>(null);
  lastSelectionText = $state("");
  selectionRect = $state<DOMRect | null>(null);
  popoverOpen = $state(false);

  /**
   * Identity of the last fired request, used to dedupe and to discard stale
   * completions. Keyed by (gutenbergId, byteStart, byteEnd, tier).
   */
  private lastRequestKey: string | null = null;
  private requestSeq = 0;
  private effectsInitialised = false;
  private lastAnchor: SelectionAnchor | null = null;

  initEffects(): void {
    if (this.effectsInitialised) return;
    this.effectsInitialised = true;
    $effect.root(() => {
      $effect(() => {
        library.loadedBook?.gutenberg_id;
        this.reset();
      });
    });
  }

  reset(): void {
    this.paraphraseResult = null;
    this.paraphraseError = null;
    this.lastSelectionText = "";
    this.selectionRect = null;
    this.popoverOpen = false;
    this.lastRequestKey = null;
    this.lastAnchor = null;
  }

  closePopover(): void {
    this.popoverOpen = false;
    this.selectionRect = null;
  }

  /**
   * Entry point from the reader's mouseup. Reads the current window selection,
   * snaps it to whole-sentence boundaries, opens the popover, and fires a
   * paraphrase if the snapped range is new. If the selection is collapsed or
   * empty, dismisses the popover.
   */
  async handleSelection(): Promise<void> {
    const loadedBook = library.loadedBook;
    if (!loadedBook) return;
    const anchor = computeAnchor(loadedBook.gutenberg_id);
    if (!anchor) {
      // Click without drag — dismiss the popover.
      if (this.popoverOpen) this.closePopover();
      return;
    }
    applySnappedRange(anchor);
    await this.fireFromAnchor(anchor);
  }

  /**
   * Fire a paraphrase from an already-computed (and already-snapped) anchor.
   * Used by handleSelection after fresh mouseup, and by retryWithCurrentTier
   * to re-fire after a tier change without re-reading window.getSelection
   * (which may have been collapsed by clicking a popover button).
   */
  private async fireFromAnchor(anchor: SelectionAnchor): Promise<void> {
    this.lastAnchor = anchor;
    this.lastSelectionText = anchor.selText;
    this.selectionRect = anchor.rect;
    this.popoverOpen = true;

    if (anchor.endByte <= anchor.startByte) return;

    const key = `${anchor.gutenbergId}:${anchor.startByte}:${anchor.endByte}:${this.tier}`;
    if (key === this.lastRequestKey && this.paraphraseResult) {
      return;
    }
    this.lastRequestKey = key;

    const myRequestId = ++this.requestSeq;
    this.paraphraseBusy = true;
    this.paraphraseError = null;
    this.paraphraseResult = null;

    try {
      const result = await invoke<FathomResult>("library_paraphrase_selection", {
        args: {
          gutenbergId: anchor.gutenbergId,
          startByte: anchor.startByte,
          endByte: anchor.endByte,
          tier: this.tier,
        },
      });
      if (myRequestId !== this.requestSeq) return;
      this.paraphraseResult = result;
    } catch (e) {
      if (myRequestId !== this.requestSeq) return;
      const msg =
        e instanceof Error
          ? e.message
          : typeof e === "object" && e !== null && "message" in e
            ? String((e as { message: unknown }).message)
            : typeof e === "string"
              ? e
              : JSON.stringify(e);
      this.paraphraseError = msg || "paraphrase failed";
    } finally {
      if (myRequestId === this.requestSeq) this.paraphraseBusy = false;
    }
  }

  /**
   * Re-fire the cached anchor with the current tier. No-op if no anchor.
   */
  async retryWithCurrentTier(): Promise<void> {
    if (!this.lastAnchor) return;
    this.lastRequestKey = null;
    await this.fireFromAnchor(this.lastAnchor);
  }
}

function computeAnchor(gutenbergId: number): SelectionAnchor | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (range.toString().trim().length === 0) return null;

  const startParaEl = nearestParagraph(range.startContainer);
  const endParaEl = nearestParagraph(range.endContainer);
  // For now we only handle selections within a single paragraph element.
  // A cross-paragraph selection collapses to whichever endpoint we can
  // resolve a paragraph for; both endpoints get snapped to that paragraph.
  const paraEl = startParaEl ?? endParaEl;
  if (!paraEl) return null;

  const paraText = paraEl.textContent ?? "";
  const paraByteStart = Number(paraEl.dataset.byteStart ?? "0");

  const rawStart = startParaEl === paraEl
    ? charOffsetInParagraph(paraEl, range.startContainer, range.startOffset)
    : 0;
  const rawEnd = endParaEl === paraEl
    ? charOffsetInParagraph(paraEl, range.endContainer, range.endOffset)
    : paraText.length;

  const snapped = snapToSentences(paraText, rawStart, rawEnd);
  const selText = paraText.slice(snapped.start, snapped.end);
  const rect = range.getBoundingClientRect();

  const startByte = paraByteStart + utf8ByteLength(paraText.slice(0, snapped.start));
  const endByte = paraByteStart + utf8ByteLength(paraText.slice(0, snapped.end));

  return {
    paraEl,
    paraText,
    paraByteStart,
    snappedStartChar: snapped.start,
    snappedEndChar: snapped.end,
    selText,
    rect,
    gutenbergId,
    startByte,
    endByte,
  };
}

function nearestParagraph(node: Node): HTMLElement | null {
  const el = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement);
  return (el?.closest("[data-byte-start]") as HTMLElement | null) ?? null;
}

/**
 * Map a (container, offset) pair from a DOM Range to a char (UTF-16) offset
 * within `paraEl`'s textContent. Walks the paragraph's descendants in order
 * accumulating textContent lengths.
 */
function charOffsetInParagraph(
  paraEl: HTMLElement,
  container: Node,
  offset: number,
): number {
  if (container === paraEl) {
    // Element container: offset is a child index.
    let chars = 0;
    const limit = Math.min(offset, paraEl.childNodes.length);
    for (let i = 0; i < limit; i++) {
      chars += paraEl.childNodes[i].textContent?.length ?? 0;
    }
    return chars;
  }

  let chars = 0;
  const walker = document.createTreeWalker(paraEl, NodeFilter.SHOW_TEXT);
  let n: Node | null = walker.nextNode();
  while (n) {
    if (n === container) {
      return chars + offset;
    }
    chars += (n as Text).data.length;
    n = walker.nextNode();
  }
  return chars;
}

/**
 * Replace the current window selection with a Range that spans
 * [snappedStartChar, snappedEndChar) within `paraEl`. This is the "selection
 * visibly grows to match what we're paraphrasing" UX moment.
 */
function applySnappedRange(anchor: SelectionAnchor): void {
  const sel = window.getSelection();
  if (!sel) return;
  const range = document.createRange();
  const start = locateCharOffset(anchor.paraEl, anchor.snappedStartChar);
  const end = locateCharOffset(anchor.paraEl, anchor.snappedEndChar);
  if (!start || !end) return;
  try {
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
  } catch {
    return;
  }
  sel.removeAllRanges();
  sel.addRange(range);
  anchor.rect = range.getBoundingClientRect();
}

function locateCharOffset(
  paraEl: HTMLElement,
  charOffset: number,
): { node: Node; offset: number } | null {
  const walker = document.createTreeWalker(paraEl, NodeFilter.SHOW_TEXT);
  let n: Node | null = walker.nextNode();
  let remaining = charOffset;
  let last: Text | null = null;
  while (n) {
    const len = (n as Text).data.length;
    if (remaining <= len) {
      return { node: n, offset: remaining };
    }
    remaining -= len;
    last = n as Text;
    n = walker.nextNode();
  }
  if (last) return { node: last, offset: last.data.length };
  return null;
}

export const paraphrase = new ParaphraseStore();
