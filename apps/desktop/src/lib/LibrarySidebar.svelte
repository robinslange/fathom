<script lang="ts">
  import { library } from "./use-library.svelte.js";
  import { search } from "./use-search.svelte.js";
  import { isBoilerplate } from "./boilerplate.js";

  let leftListItems = $derived.by(() => {
    if (search.query.trim().length > 0) {
      return search.searchHits
        .filter((h) => !isBoilerplate(h.excerpt))
        .map((h) => {
          const book = library.manifest.find((b) => b.gutenberg_id === h.gutenberg_id);
          return {
            kind: "hit" as const,
            gutenberg_id: h.gutenberg_id,
            title: book?.title ?? `pg${h.gutenberg_id}`,
            author: book?.translators[0]?.name ?? "",
            excerpt: h.excerpt,
            similarity: h.similarity,
            chunk_id: h.chunk_id,
          };
        });
    }
    return [...library.manifest]
      .sort((a, b) => {
        const aAuth = a.translators[0]?.name ?? "";
        const bAuth = b.translators[0]?.name ?? "";
        if (aAuth !== bAuth) return aAuth.localeCompare(bAuth);
        return a.title.localeCompare(b.title);
      })
      .map((b) => ({
        kind: "book" as const,
        gutenberg_id: b.gutenberg_id,
        title: b.title,
        author: b.translators[0]?.name ?? "",
        excerpt: "",
        similarity: 0,
        chunk_id: "",
      }));
  });

  function snippet(s: string, n = 90): string {
    return s.length > n ? s.slice(0, n).trimEnd() + "…" : s;
  }
</script>

<aside class="left-column">
  {#if library.manifestLoading}
    <p class="empty">Loading library…</p>
  {:else if library.manifestError}
    <div class="offline">
      <p>Library offline.</p>
      <button class="retry" onclick={() => library.retryManifest()}>Retry</button>
    </div>
  {:else if leftListItems.length === 0}
    <p class="empty">
      {search.query.trim().length > 0 ? "No hits for this query." : "Library is empty."}
    </p>
  {:else}
    <ul aria-label="Library">
      {#each leftListItems as item (item.gutenberg_id + ":" + item.chunk_id)}
        <li>
          <button
            class="hit"
            class:active={library.loadedBook?.gutenberg_id === item.gutenberg_id}
            onclick={() => library.openBook(item.gutenberg_id, item.chunk_id || undefined)}
          >
            <div class="hit-meta">
              <span class="author">{item.author}</span>
              {#if item.author && item.title}
                <span class="dot">·</span>
              {/if}
              <span class="title">{item.title}</span>
            </div>
            {#if item.kind === "hit"}
              <div class="hit-excerpt">{snippet(item.excerpt, 160)}</div>
              <div class="hit-sim">sim {item.similarity.toFixed(2)}</div>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .left-column {
    overflow-y: auto;
    border-right: 1px solid var(--rule);
    background: var(--panel-soft);
  }
  .left-column ul {
    list-style: none;
    margin: 0;
    padding: 0.25rem 0;
  }
  .hit {
    width: 100%;
    text-align: left;
    background: transparent;
    color: inherit;
    border: 0;
    padding: 0.65rem 0.9rem;
    cursor: pointer;
    border-bottom: 1px solid var(--rule-faint);
    font: inherit;
  }
  .hit:hover {
    background: var(--accent-wash);
  }
  .hit.active {
    background: var(--accent-wash-active);
  }
  .hit:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .hit-meta {
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.85rem;
    opacity: 0.75;
  }
  .hit-meta .title {
    font-weight: 500;
  }
  .hit-meta .dot {
    margin: 0 0.4em;
    opacity: 0.5;
  }
  .hit-excerpt {
    margin-top: 0.35rem;
    font-size: 0.9rem;
    line-height: 1.45;
    opacity: 0.85;
  }
  .hit-sim {
    margin-top: 0.25rem;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.75rem;
    opacity: 0.55;
  }
  .empty {
    padding: 1.5rem 1rem;
    opacity: 0.55;
    font-style: italic;
  }
  .offline {
    padding: 1.5rem 1rem;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }
  .offline p {
    margin: 0;
    opacity: 0.7;
  }
  .retry {
    background: transparent;
    color: inherit;
    border: 1px solid var(--rule-stronger);
    padding: 0.3rem 0.7rem;
    font: inherit;
    font-size: 0.85rem;
    border-radius: 3px;
    cursor: pointer;
  }
  .retry:hover {
    background: var(--accent-wash-hover);
  }
  .retry:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
