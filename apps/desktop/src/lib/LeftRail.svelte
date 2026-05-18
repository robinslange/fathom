<script lang="ts">
  import { onMount } from "svelte";
  import { library } from "./use-library.svelte.js";
  import { themes, type ThemeBookSummary } from "./use-themes.svelte.js";
  import { search } from "./use-search.svelte.js";
  import { isBoilerplate } from "./boilerplate.js";

  onMount(() => {
    if (themes.themes.length === 0 && !themes.themesError) {
      themes.init();
    }
  });

  function snippet(s: string, n = 90): string {
    return s.length > n ? s.slice(0, n).trimEnd() + "…" : s;
  }

  let searchHits = $derived.by(() => {
    if (search.query.trim().length === 0) return null;
    return search.searchHits
      .filter((h) => !isBoilerplate(h.excerpt))
      .map((h) => {
        const book = library.manifest.find((b) => b.gutenberg_id === h.gutenberg_id);
        return {
          gutenberg_id: h.gutenberg_id,
          title: book?.title ?? `pg${h.gutenberg_id}`,
          author: book?.translators[0]?.name ?? "",
          excerpt: h.excerpt,
          similarity: h.similarity,
          chunk_id: h.chunk_id,
        };
      });
  });

  function openBook(book: ThemeBookSummary) {
    library.openBook(book.gutenberg_id);
  }
</script>

<aside class="left-column">
  {#if searchHits}
    {#if searchHits.length === 0}
      <p class="empty">No hits for this query.</p>
    {:else}
      <ul aria-label="Search results">
        {#each searchHits as h (h.gutenberg_id + ":" + h.chunk_id)}
          <li>
            <button
              class="hit"
              class:active={library.loadedBook?.gutenberg_id === h.gutenberg_id}
              onclick={() => library.openBook(h.gutenberg_id, h.chunk_id || undefined)}
            >
              <div class="hit-meta">
                <span class="author">{h.author}</span>
                {#if h.author && h.title}<span class="dot">·</span>{/if}
                <span class="title">{h.title}</span>
              </div>
              <div class="hit-excerpt">{snippet(h.excerpt, 160)}</div>
              <div class="hit-sim">sim {h.similarity.toFixed(2)}</div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if themes.themesLoading}
    <p class="empty">Loading themes…</p>
  {:else if themes.themesError}
    <div class="offline">
      <p>Library offline.</p>
      <button class="retry" onclick={() => themes.retry()}>Retry</button>
    </div>
  {:else}
    <ul aria-label="Themes">
      {#each themes.themes as theme (theme.slug)}
        <li class:other={theme.slug === "other"}>
          <button
            class="theme"
            class:expanded={themes.expandedSlug === theme.slug}
            aria-expanded={themes.expandedSlug === theme.slug}
            onclick={() => themes.expand(theme.slug)}
          >
            <span class="theme-label">{theme.label}</span>
            <span class="theme-count">{theme.count}</span>
          </button>
          {#if themes.expandedSlug === theme.slug}
            <ul class="theme-books" aria-label="Books in {theme.label}">
              {#if themes.loadingTheme === theme.slug}
                <li class="loading">Loading…</li>
              {:else if (themes.booksByTheme[theme.slug] ?? []).length === 0}
                <li class="empty">No books in this theme yet.</li>
              {:else}
                {#each themes.booksByTheme[theme.slug] ?? [] as book (book.gutenberg_id)}
                  <li>
                    <button
                      class="book"
                      class:active={library.loadedBook?.gutenberg_id === book.gutenberg_id}
                      onclick={() => openBook(book)}
                    >
                      <span class="author">{book.translators[0] ?? ""}</span>
                      {#if book.translators[0]}<span class="dot">·</span>{/if}
                      <span class="title">{book.title}</span>
                    </button>
                  </li>
                {/each}
              {/if}
            </ul>
          {/if}
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .theme {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    text-align: left;
    background: transparent;
    color: inherit;
    border: 0;
    padding: 0.7rem 0.9rem;
    cursor: pointer;
    border-bottom: 1px solid var(--rule-faint);
    font: inherit;
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.92rem;
  }
  .theme:hover {
    background: var(--accent-wash);
  }
  .theme.expanded {
    background: var(--accent-wash-active);
    font-weight: 500;
  }
  .theme:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .theme-count {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.78rem;
    opacity: 0.55;
  }
  li.other > .theme {
    font-size: 0.84rem;
    opacity: 0.75;
    border-top: 1px solid var(--rule);
  }
  .theme-books {
    background: var(--paper);
  }
  .book {
    width: 100%;
    text-align: left;
    background: transparent;
    color: inherit;
    border: 0;
    padding: 0.5rem 0.9rem 0.5rem 1.6rem;
    cursor: pointer;
    border-bottom: 1px solid var(--rule-faint);
    font: inherit;
    font-size: 0.88rem;
  }
  .book:hover {
    background: var(--accent-wash);
  }
  .book.active {
    background: var(--accent-wash-active);
  }
  .book .author { opacity: 0.7; }
  .book .dot { margin: 0 0.35em; opacity: 0.5; }
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
  .hit:hover { background: var(--accent-wash); }
  .hit.active { background: var(--accent-wash-active); }
  .hit-meta { font-family: "IBM Plex Sans", sans-serif; font-size: 0.85rem; opacity: 0.75; }
  .hit-meta .title { font-weight: 500; }
  .hit-meta .dot { margin: 0 0.4em; opacity: 0.5; }
  .hit-excerpt { margin-top: 0.35rem; font-size: 0.9rem; line-height: 1.45; opacity: 0.85; }
  .hit-sim { margin-top: 0.25rem; font-family: "IBM Plex Mono", monospace; font-size: 0.75rem; opacity: 0.55; }
  .empty { padding: 1.5rem 1rem; opacity: 0.55; font-style: italic; }
  .loading { padding: 0.6rem 1.6rem; opacity: 0.55; font-style: italic; font-size: 0.85rem; }
  .offline { padding: 1.5rem 1rem; display: flex; flex-direction: column; align-items: flex-start; gap: 0.5rem; }
  .offline p { margin: 0; opacity: 0.7; }
  .retry { background: transparent; color: inherit; border: 1px solid var(--rule-strong); padding: 0.3rem 0.7rem; font: inherit; font-size: 0.85rem; border-radius: 3px; cursor: pointer; }
  .retry:hover { background: var(--accent-wash-hover); }
</style>
