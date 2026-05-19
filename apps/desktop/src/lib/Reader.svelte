<script lang="ts">
  import { tick } from "svelte";
  import { library } from "./use-library.svelte.js";
  import { paraphrase } from "./use-paraphrase.svelte.js";

  let viewportEl: HTMLDivElement | null = $state(null);
  let flowEl: HTMLDivElement | null = $state(null);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let columnStride = $state(0); // column-width + column-gap, in px

  // Recompute layout each time book or viewport changes.
  $effect(() => {
    library.loadedBook?.gutenberg_id;
    library.loadingBook;
    viewportWidth;
    viewportHeight;
    tick().then(() => measure());
  });

  function measure() {
    if (!viewportEl || !flowEl) return;
    const w = viewportEl.clientWidth;
    const h = viewportEl.clientHeight;
    if (!w || !h) return;
    viewportWidth = w;
    viewportHeight = h;
    // column-width = viewportWidth - 2 * column-gap-padding pattern.
    // We use column-gap=64px (gutter) and one column per page.
    const gap = 64;
    columnStride = w + gap;
    // Apply.
    flowEl.style.height = `${h}px`;
    flowEl.style.columnWidth = `${w}px`;
    flowEl.style.columnGap = `${gap}px`;
    flowEl.style.columnFill = "auto";
    // Allow horizontal column overflow; the wrapper hides it.
    const totalWidth = flowEl.scrollWidth;
    const pageCount = Math.max(1, Math.round(totalWidth / columnStride));
    library.setPageCount(pageCount);
    // If the currentPage is out of range, clamp.
    if (library.currentPage >= pageCount) library.setPage(pageCount - 1);
    // If a chunk needs scroll-to, resolve now.
    const target = library.pendingScrollChunk;
    if (target && flowEl) {
      const el = flowEl.querySelector(`[data-chunk-id="${target}"]`) as HTMLElement | null;
      if (el) {
        const page = Math.floor(el.offsetLeft / columnStride);
        library.setPage(page);
      }
      library.clearPendingScroll();
    }
  }

  function onResize() {
    measure();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!library.loadedBook) return;
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
    if (e.key === "ArrowLeft") { e.preventDefault(); library.pageBack(); }
    else if (e.key === "ArrowRight") { e.preventDefault(); library.pageForward(); }
    else if (e.key === " " && e.shiftKey) { e.preventDefault(); library.pageBack(); }
    else if (e.key === " ") { e.preventDefault(); library.pageForward(); }
  }
</script>

<svelte:window onkeydown={onKeydown} onresize={onResize} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="reader" aria-label="Book reader" onmouseup={() => paraphrase.handleSelection()}>
  {#if !library.loadedBook}
    <div class="empty">
      <p>Pick a book on the left.</p>
    </div>
  {:else if library.loadingBook}
    <div class="empty">
      <p>Loading {library.loadedBook.title}…</p>
    </div>
  {:else}
    {@const loadedBook = library.loadedBook}
    <article>
      <header class="book-header">
        <h2>{loadedBook.title}</h2>
        {#if loadedBook.translators.length > 0}
          <p class="translators">
            tr. {loadedBook.translators.join(", ")}
          </p>
        {/if}
      </header>
      <div class="viewport" bind:this={viewportEl}>
        <div
          class="book-flow"
          bind:this={flowEl}
          style:transform="translateX(-{library.currentPage * columnStride}px)"
        >
          {#each library.paragraphs as p, i (i)}
            <p
              data-chunk-id={p.chunkId}
              data-byte-start={p.byteStart}
              class:has-chunk={p.chunkId !== ""}
            >{p.text}</p>
          {/each}
        </div>
      </div>
      <footer class="pagination">
        <button class="page-btn" onclick={() => library.pageBack()} disabled={library.currentPage === 0} aria-label="Previous page">&#x2039;</button>
        <span class="page-indicator">page {library.currentPage + 1} of {library.pageCount}</span>
        <button class="page-btn" onclick={() => library.pageForward()} disabled={library.currentPage >= library.pageCount - 1} aria-label="Next page">&#x203a;</button>
      </footer>
    </article>
  {/if}
</section>

<style>
  .reader {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 2rem 3rem 1rem;
    user-select: text;
  }
  .reader article {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }
  .reader .book-header {
    flex: 0 0 auto;
    margin-bottom: 1.25rem;
    padding-bottom: 0.85rem;
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

  .viewport {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  .book-flow {
    /* width: 100% along with column-width = clientWidth coerces a single
       column per "page"; subsequent columns overflow horizontally and are
       paged by transform on the parent. column-fill: auto stops the browser
       balancing columns. */
    transition: transform 0.18s ease;
    will-change: transform;
  }
  @media (prefers-reduced-motion: reduce) {
    .book-flow { transition: none; }
  }
  .book-flow p {
    line-height: 1.7;
    margin: 0 0 1.1rem;
    white-space: pre-wrap;
    break-inside: auto;
  }
  .book-flow p::selection {
    background: var(--accent-selection);
  }

  .pagination {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    margin-top: 0.85rem;
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
  .page-btn:hover:not(:disabled) { background: var(--accent-wash-hover); }
  .page-btn:disabled { opacity: 0.3; cursor: default; }
  .page-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .page-indicator { opacity: 0.55; letter-spacing: 0.04em; }

  .empty {
    padding: 1.5rem 1rem;
    opacity: 0.55;
    font-style: italic;
  }
</style>
