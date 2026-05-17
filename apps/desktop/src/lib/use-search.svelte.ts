import { invoke } from "@tauri-apps/api/core";
import { library } from "./use-library.svelte.js";

export type SearchHit = {
  gutenberg_id: number;
  chunk_id: string;
  excerpt: string;
  similarity: number;
};

class SearchStore {
  query = $state("");
  searchHits = $state<SearchHit[]>([]);
  searching = $state(false);

  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private effectsInitialised = false;

  initEffects(): void {
    if (this.effectsInitialised) return;
    this.effectsInitialised = true;
    $effect.root(() => {
      $effect(() => {
        const q = this.query.trim();
        if (this.searchTimer) clearTimeout(this.searchTimer);
        if (q.length === 0 || !library.embedderReady) {
          this.searchHits = [];
          this.searching = false;
          return;
        }
        this.searching = true;
        this.searchTimer = setTimeout(async () => {
          try {
            this.searchHits = await invoke<SearchHit[]>("library_search", {
              query: q,
              topN: 50,
            });
          } catch {
            this.searchHits = [];
          } finally {
            this.searching = false;
          }
        }, 200);
      });
    });
  }
}

export const search = new SearchStore();
