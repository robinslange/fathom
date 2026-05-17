import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getPage, pageForChunk, type Paragraph } from "./pagination.js";
import { utf8ByteLength } from "./utf8.js";

export type TranslatorEntry = {
  name: string;
  birth_year: number | null;
  death_year: number | null;
};

export type ManifestBook = {
  gutenberg_id: number;
  title: string;
  translators: TranslatorEntry[];
  locc: string[];
  tradition: string;
  shard_filename: string;
  shard_sha256: string;
  shard_size_bytes: number;
  chunk_count: number;
};

export type ChunkRefView = {
  chunk_id: string;
  byte_offset_start: number;
  byte_offset_end: number;
};

export type BookView = {
  gutenberg_id: number;
  title: string;
  translators: string[];
  canonical_text: string;
  chunks: ChunkRefView[];
};

export type DownloadProgress = {
  model: string;
  bytes: number;
  total: number | null;
};

export const modelLabels: Record<string, string> = {
  "gemma3-4b": "Loading paraphrase model (Gemma 3 4B)",
  "deberta-nli": "Loading faithfulness model (DeBERTa NLI)",
  "deberta-nli-tokenizer": "Loading faithfulness tokenizer",
  "bge-small": "Loading embedding model (bge-small)",
  "bge-small-tokenizer": "Loading embedding tokenizer",
};

// ----- state -----
let manifest: ManifestBook[] = $state([]);
let manifestLoading = $state(true);
let manifestError: string | null = $state(null);

let embedderReady = $state(false);
let embedderError: string | null = $state(null);

let downloadProgress: Record<string, DownloadProgress> = $state({});

let loadedBook: BookView | null = $state(null);
let loadingBook = $state(false);
let loadBookError: string | null = $state(null);

let currentPage = $state(0);

let paragraphs = $derived.by((): Paragraph[] => {
  if (!loadedBook) return [];
  const text = loadedBook.canonical_text;
  const result: Paragraph[] = [];
  let offset = 0;
  const SEPARATOR_BYTES = 2;
  for (const para of text.split("\n\n")) {
    const chunk = loadedBook.chunks.find(
      (c) => c.byte_offset_start <= offset && offset < c.byte_offset_end,
    );
    result.push({
      chunkId: chunk?.chunk_id ?? "",
      byteStart: offset,
      text: para,
    });
    offset += utf8ByteLength(para) + SEPARATOR_BYTES;
  }
  return result;
});

let currentPageBounds = $derived(getPage(paragraphs, currentPage));

$effect(() => {
  loadedBook?.gutenberg_id;
  currentPage = 0;
});

// ----- getters -----
export function getManifest(): ManifestBook[] { return manifest; }
export function isManifestLoading(): boolean { return manifestLoading; }
export function getManifestError(): string | null { return manifestError; }
export function isEmbedderReady(): boolean { return embedderReady; }
export function getEmbedderError(): string | null { return embedderError; }
export function getDownloadProgress(): Record<string, DownloadProgress> { return downloadProgress; }
export function getLoadedBook(): BookView | null { return loadedBook; }
export function isLoadingBook(): boolean { return loadingBook; }
export function getLoadBookError(): string | null { return loadBookError; }
export function getParagraphs(): Paragraph[] { return paragraphs; }
export function getCurrentPage(): number { return currentPage; }
export function setCurrentPage(n: number): void { currentPage = n; }
export function getCurrentPageBounds() { return currentPageBounds; }

// ----- actions -----
export async function initLibrary(): Promise<void> {
  listen<DownloadProgress>("fathom://download-progress", (e) => {
    downloadProgress = { ...downloadProgress, [e.payload.model]: e.payload };
  });

  try {
    manifest = await invoke<ManifestBook[]>("library_manifest");
  } catch (e) {
    manifestError = e instanceof Error ? e.message : String(e);
  } finally {
    manifestLoading = false;
  }

  if (manifestError) return;

  const embedderPromise = (async () => {
    try {
      await invoke("library_ensure_embedder");
      embedderReady = true;
    } catch (e) {
      embedderError = e instanceof Error ? e.message : String(e);
    }
  })();

  try {
    await invoke<number>("library_prewarm_shards", { limit: 64 });
  } catch (e) {
    console.warn("prewarm failed:", e);
  }

  await embedderPromise;
}

export async function retryManifest(): Promise<void> {
  manifestLoading = true;
  manifestError = null;
  try {
    manifest = await invoke<ManifestBook[]>("library_manifest");
  } catch (e) {
    manifestError = e instanceof Error ? e.message : String(e);
  } finally {
    manifestLoading = false;
  }
}

export async function openBook(gutenberg_id: number, chunkIdToScrollTo?: string): Promise<void> {
  if (loadedBook?.gutenberg_id === gutenberg_id) {
    if (chunkIdToScrollTo) scrollToChunk(chunkIdToScrollTo);
    return;
  }
  loadingBook = true;
  loadBookError = null;
  try {
    loadedBook = await invoke<BookView>("library_load_book", {
      gutenbergId: gutenberg_id,
    });
    if (chunkIdToScrollTo) {
      setTimeout(() => {
        currentPage = pageForChunk(paragraphs, chunkIdToScrollTo);
        scrollToChunk(chunkIdToScrollTo);
      }, 0);
    }
  } catch (e) {
    loadBookError = e instanceof Error ? e.message : String(e);
  } finally {
    loadingBook = false;
  }
}

export function scrollReaderToTop() {
  const el = document.querySelector(".reader") as HTMLElement | null;
  if (el) el.scrollTop = 0;
}

export function pageBack() {
  if (currentPage > 0) {
    currentPage -= 1;
    window.getSelection()?.removeAllRanges();
    setTimeout(scrollReaderToTop, 0);
  }
}

export function pageForward() {
  if (currentPage < currentPageBounds.pageCount - 1) {
    currentPage += 1;
    window.getSelection()?.removeAllRanges();
    setTimeout(scrollReaderToTop, 0);
  }
}

function scrollToChunk(chunkId: string) {
  const el = document.querySelector(`[data-chunk-id="${chunkId}"]`);
  if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
}
