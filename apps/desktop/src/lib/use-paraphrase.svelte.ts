import { invoke } from "@tauri-apps/api/core";
import { getLoadedBook, getParagraphs } from "./use-library.svelte.js";
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

// ----- state -----
let tier: Tier = $state("standard");
let paraphraseResult: FathomResult | null = $state(null);
let paraphraseBusy = $state(false);
let paraphraseError: string | null = $state(null);
let lastSelectionText = $state("");

$effect(() => {
  getLoadedBook()?.gutenberg_id;
  paraphraseResult = null;
  paraphraseError = null;
});

// ----- getters / setters -----
export function getTier(): Tier { return tier; }
export function setTier(t: Tier): void { tier = t; }
export function getParaphraseResult(): FathomResult | null { return paraphraseResult; }
export function isParaphraseBusy(): boolean { return paraphraseBusy; }
export function getParaphraseError(): string | null { return paraphraseError; }
export function getLastSelectionText(): string { return lastSelectionText; }

// ----- action -----
export async function paraphraseSelection(): Promise<void> {
  const loadedBook = getLoadedBook();
  if (!loadedBook) return;
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) return;
  const range = selection.getRangeAt(0);
  const selText = selection.toString();
  if (selText.trim().length === 0) return;

  const paragraphs = getParagraphs();

  let startByte = endpointToByte(range.startContainer, range.startOffset, "start", paragraphs);
  let endByte = endpointToByte(range.endContainer, range.endOffset, "end", paragraphs);

  const startParaEl = (range.startContainer.nodeType === Node.TEXT_NODE
    ? range.startContainer.parentElement
    : (range.startContainer as HTMLElement)
  )?.closest("[data-byte-start]") as HTMLElement | null;
  const endParaEl = (range.endContainer.nodeType === Node.TEXT_NODE
    ? range.endContainer.parentElement
    : (range.endContainer as HTMLElement)
  )?.closest("[data-byte-start]") as HTMLElement | null;
  if (!startParaEl && endParaEl) {
    startByte = Number(endParaEl.dataset.byteStart ?? "0");
  }
  if (!endParaEl && startParaEl) {
    const spByteStart = Number(startParaEl.dataset.byteStart ?? "0");
    const sp = paragraphs.find((p) => p.byteStart === spByteStart);
    if (sp) endByte = spByteStart + utf8ByteLength(sp.text);
  }

  if (endByte <= startByte) return;

  lastSelectionText = selText;
  paraphraseBusy = true;
  paraphraseError = null;
  paraphraseResult = null;
  try {
    paraphraseResult = await invoke<FathomResult>("library_paraphrase_selection", {
      args: {
        gutenbergId: loadedBook.gutenberg_id,
        startByte,
        endByte,
        tier,
      },
    });
  } catch (e) {
    const msg =
      e instanceof Error
        ? e.message
        : typeof e === "object" && e !== null && "message" in e
          ? String((e as { message: unknown }).message)
          : typeof e === "string"
            ? e
            : JSON.stringify(e);
    paraphraseError = msg || "paraphrase failed";
  } finally {
    paraphraseBusy = false;
  }
}

function endpointToByte(
  container: Node,
  charOffset: number,
  fallback: "start" | "end",
  paragraphs: { chunkId: string; byteStart: number; text: string }[],
): number {
  const paraEl = (container.nodeType === Node.TEXT_NODE
    ? container.parentElement
    : (container as HTMLElement)
  )?.closest("[data-byte-start]") as HTMLElement | null;

  if (!paraEl) {
    if (paragraphs.length === 0) return 0;
    if (fallback === "start") return paragraphs[0].byteStart;
    const last = paragraphs[paragraphs.length - 1];
    return last.byteStart + utf8ByteLength(last.text);
  }

  const paraByteStart = Number(paraEl.dataset.byteStart ?? "0");

  if (container.nodeType === Node.TEXT_NODE) {
    let bytes = 0;
    for (const sib of Array.from(paraEl.childNodes)) {
      if (sib === container) break;
      bytes += utf8ByteLength(sib.textContent ?? "");
    }
    bytes += utf8ByteLength((container as Text).data.slice(0, charOffset));
    return paraByteStart + bytes;
  }

  const el = container as HTMLElement;
  const limit = Math.min(charOffset, el.childNodes.length);
  let bytes = 0;
  for (let i = 0; i < limit; i++) {
    bytes += utf8ByteLength(el.childNodes[i].textContent ?? "");
  }
  return paraByteStart + bytes;
}
