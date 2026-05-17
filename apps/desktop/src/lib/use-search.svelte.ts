import { invoke } from "@tauri-apps/api/core";
import { isEmbedderReady } from "./use-library.svelte.js";

export type SearchHit = {
  gutenberg_id: number;
  chunk_id: string;
  excerpt: string;
  similarity: number;
};

// ----- state -----
let query = $state("");
let searchHits: SearchHit[] = $state([]);
let searching = $state(false);

let searchTimer: ReturnType<typeof setTimeout> | null = null;

$effect(() => {
  const q = query.trim();
  if (searchTimer) clearTimeout(searchTimer);
  if (q.length === 0 || !isEmbedderReady()) {
    searchHits = [];
    searching = false;
    return;
  }
  searching = true;
  searchTimer = setTimeout(async () => {
    try {
      searchHits = await invoke<SearchHit[]>("library_search", {
        query: q,
        topN: 50,
      });
    } catch {
      searchHits = [];
    } finally {
      searching = false;
    }
  }, 200);
});

// ----- getters / setters -----
export function getQuery(): string { return query; }
export function setQuery(v: string): void { query = v; }
export function getSearchHits(): SearchHit[] { return searchHits; }
export function isSearching(): boolean { return searching; }
