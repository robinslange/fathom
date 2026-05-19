import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getPage, pageForChunk, type Paragraph } from "./pagination.js";
import { utf8ByteLength } from "./utf8.js";
import { isBoilerplate } from "./boilerplate.js";

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

class LibraryStore {
  manifest = $state<ManifestBook[]>([]);
  manifestLoading = $state(true);
  manifestError = $state<string | null>(null);

  embedderReady = $state(false);
  embedderError = $state<string | null>(null);

  judgeReady = $state(false);
  llamaReady = $state(false);
  warmupError = $state<string | null>(null);

  downloadProgress = $state<Record<string, DownloadProgress>>({});

  loadedBook = $state<BookView | null>(null);
  loadingBook = $state(false);
  loadBookError = $state<string | null>(null);

  currentPage = $state(0);

  paragraphs = $derived.by((): Paragraph[] => {
    if (!this.loadedBook) return [];
    const text = this.loadedBook.canonical_text;
    const result: Paragraph[] = [];
    let offset = 0;
    const SEPARATOR_BYTES = 2;
    for (const para of text.split("\n\n")) {
      const chunk = this.loadedBook.chunks.find(
        (c) => c.byte_offset_start <= offset && offset < c.byte_offset_end,
      );
      if (!isBoilerplate(para)) {
        result.push({
          chunkId: chunk?.chunk_id ?? "",
          byteStart: offset,
          text: para,
        });
      }
      offset += utf8ByteLength(para) + SEPARATOR_BYTES;
    }
    return result;
  });

  currentPageBounds = $derived(getPage(this.paragraphs, this.currentPage));

  private effectsInitialised = false;

  initEffects(): void {
    if (this.effectsInitialised) return;
    this.effectsInitialised = true;
    $effect.root(() => {
      $effect(() => {
        this.loadedBook?.gutenberg_id;
        this.currentPage = 0;
      });
    });
  }

  async init(): Promise<void> {
    listen<DownloadProgress>("fathom://download-progress", (e) => {
      this.downloadProgress = { ...this.downloadProgress, [e.payload.model]: e.payload };
    });

    try {
      this.manifest = await invoke<ManifestBook[]>("library_manifest");
    } catch (e) {
      this.manifestError = e instanceof Error ? e.message : String(e);
    } finally {
      this.manifestLoading = false;
    }

    if (this.manifestError) return;

    const embedderPromise = (async () => {
      try {
        await invoke("library_ensure_embedder");
        this.embedderReady = true;
      } catch (e) {
        this.embedderError = e instanceof Error ? e.message : String(e);
      }
    })();

    const warmupPromise = (async () => {
      try {
        await invoke("library_warmup_models");
        this.judgeReady = true;
        this.llamaReady = true;
      } catch (e) {
        this.warmupError = e instanceof Error ? e.message : String(e);
      }
    })();

    try {
      await invoke<number>("library_prewarm_shards", { limit: 64 });
    } catch (e) {
      console.warn("prewarm failed:", e);
    }

    await Promise.all([embedderPromise, warmupPromise]);
  }

  async retryManifest(): Promise<void> {
    this.manifestLoading = true;
    this.manifestError = null;
    try {
      this.manifest = await invoke<ManifestBook[]>("library_manifest");
    } catch (e) {
      this.manifestError = e instanceof Error ? e.message : String(e);
    } finally {
      this.manifestLoading = false;
    }
  }

  async openBook(gutenberg_id: number, chunkIdToScrollTo?: string): Promise<void> {
    if (this.loadedBook?.gutenberg_id === gutenberg_id) {
      if (chunkIdToScrollTo) scrollToChunk(chunkIdToScrollTo);
      return;
    }
    this.loadingBook = true;
    this.loadBookError = null;
    try {
      this.loadedBook = await invoke<BookView>("library_load_book", {
        gutenbergId: gutenberg_id,
      });
      if (chunkIdToScrollTo) {
        setTimeout(() => {
          this.currentPage = pageForChunk(this.paragraphs, chunkIdToScrollTo);
          scrollToChunk(chunkIdToScrollTo);
        }, 0);
      }
    } catch (e) {
      this.loadBookError =
        e instanceof Error
          ? e.message
          : typeof e === "object" && e !== null && "message" in e
            ? String((e as { message: unknown }).message)
            : typeof e === "string"
              ? e
              : JSON.stringify(e);
    } finally {
      this.loadingBook = false;
    }
  }

  pageBack(): void {
    if (this.currentPage > 0) {
      this.currentPage -= 1;
      window.getSelection()?.removeAllRanges();
      setTimeout(scrollReaderToTop, 0);
    }
  }

  pageForward(): void {
    if (this.currentPage < this.currentPageBounds.pageCount - 1) {
      this.currentPage += 1;
      window.getSelection()?.removeAllRanges();
      setTimeout(scrollReaderToTop, 0);
    }
  }
}

function scrollReaderToTop() {
  const el = document.querySelector(".reader") as HTMLElement | null;
  if (el) el.scrollTop = 0;
}

function scrollToChunk(chunkId: string) {
  const el = document.querySelector(`[data-chunk-id="${chunkId}"]`);
  if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
}

export const library = new LibraryStore();
