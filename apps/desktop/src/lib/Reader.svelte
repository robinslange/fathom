<script lang="ts">
  import {
    getLoadedBook,
    isLoadingBook,
    getCurrentPage,
    getCurrentPageBounds,
    pageBack,
    pageForward,
  } from "./use-library.svelte.js";
  import { paraphraseSelection } from "./use-paraphrase.svelte.js";

  function onKeydown(e: KeyboardEvent) {
    if (!getLoadedBook()) return;
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
    if (e.key === "ArrowLeft") { e.preventDefault(); pageBack(); }
    else if (e.key === "ArrowRight") { e.preventDefault(); pageForward(); }
    else if (e.key === " " && e.shiftKey) { e.preventDefault(); pageBack(); }
    else if (e.key === " ") { e.preventDefault(); pageForward(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="reader" aria-label="Book reader" onmouseup={paraphraseSelection}>
  {#if !getLoadedBook()}
    <div class="empty">
      <p>Pick a book on the left.</p>
    </div>
  {:else if isLoadingBook()}
    <div class="empty">
      <p>Loading {getLoadedBook()!.title}…</p>
    </div>
  {:else}
    {@const loadedBook = getLoadedBook()!}
    {@const pageBounds = getCurrentPageBounds()}
    <article>
      <header class="book-header">
        <h2>{loadedBook.title}</h2>
        {#if loadedBook.translators.length > 0}
          <p class="translators">
            tr. {loadedBook.translators.join(", ")}
          </p>
        {/if}
        <div class="pagination">
          <button class="page-btn" onclick={pageBack} disabled={getCurrentPage() === 0} aria-label="Previous page">&#x2039;</button>
          <span class="page-indicator">page {getCurrentPage() + 1} of {pageBounds.pageCount}</span>
          <button class="page-btn" onclick={pageForward} disabled={getCurrentPage() >= pageBounds.pageCount - 1} aria-label="Next page">&#x203a;</button>
        </div>
      </header>
      <div class="paragraphs">
        {#each pageBounds.paragraphs as p, i (pageBounds.startParaIndex + i)}
          <p
            data-chunk-id={p.chunkId}
            data-byte-start={p.byteStart}
            class:has-chunk={p.chunkId !== ""}
          >{p.text}</p>
        {/each}
      </div>
    </article>
  {/if}
</section>

<style>
  .reader {
    overflow-y: auto;
    padding: 2rem 3rem;
    user-select: text;
  }
  .reader .book-header {
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--rule);
  }
  .reader h2 {
    font-family: "IBM Plex Sans", sans-serif;
    margin: 0;
  }
  .reader .translators {
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
    opacity: 0.6;
    font-family: "IBM Plex Sans", sans-serif;
  }
  .pagination {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 0.8rem;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.78rem;
  }
  .page-btn {
    background: transparent;
    border: 1px solid var(--ink-fade-strong);
    padding: 0.15rem 0.55rem;
    font: inherit;
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    border-radius: 3px;
    color: inherit;
  }
  .page-btn:hover:not(:disabled) {
    background: var(--accent-wash-hover);
  }
  .page-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .page-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .page-indicator {
    opacity: 0.55;
    letter-spacing: 0.04em;
  }
  .paragraphs p {
    line-height: 1.7;
    margin: 0 0 1.1rem;
    white-space: pre-wrap;
  }
  .paragraphs p::selection {
    background: var(--accent-selection);
  }
  .empty {
    padding: 1.5rem 1rem;
    opacity: 0.55;
    font-style: italic;
  }
</style>
