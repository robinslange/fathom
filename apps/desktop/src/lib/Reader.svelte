<script lang="ts">
  import { tick } from "svelte";
  import { library } from "./use-library.svelte.js";
  import { paraphrase } from "./use-paraphrase.svelte.js";

  let viewportEl: HTMLDivElement | null = $state(null);
  let pageWindowEl: HTMLDivElement | null = $state(null);
  let flowEl: HTMLDivElement | null = $state(null);
  let columnStride = $state(0);

  // Remeasure when book changes or paragraphs change shape.
  $effect(() => {
    library.loadedBook?.gutenberg_id;
    library.loadingBook;
    library.paragraphs.length;
    tick().then(() => measure());
  });

  // Watch the viewport for size changes (window resize, sidebar collapse,
  // dev-tools open). ResizeObserver fires once on attach and again on each
  // change.
  $effect(() => {
    if (!viewportEl) return;
    const ro = new ResizeObserver(() => measure());
    ro.observe(viewportEl);
    return () => ro.disconnect();
  });

  // Reading-measure constants:
  // - MAX_MEASURE_PX: max column width (~65ch readability sweet spot)
  // - MIN_MEASURE_PX: floor so we never crush the column unreadable
  // - MIN_GUTTER_PX: room left for popover on at least one side
  const MAX_MEASURE_PX = 680;
  const MIN_MEASURE_PX = 480;
  const MIN_GUTTER_PX = 120;

  function measure() {
    if (!viewportEl || !pageWindowEl || !flowEl) return;
    const viewportW = viewportEl.clientWidth;
    const h = viewportEl.clientHeight;
    if (!viewportW || !h) return;

    const widthAfterGutters = viewportW - MIN_GUTTER_PX * 2;
    let columnW = Math.max(
      MIN_MEASURE_PX,
      Math.min(MAX_MEASURE_PX, widthAfterGutters),
    );
    // Narrow viewport: take everything; popover falls back to overlap.
    if (columnW > viewportW) columnW = viewportW;

    const gap = 64;
    columnStride = columnW + gap;

    // Clip tight to the active column so adjacent multi-column overflow
    // can't bleed into the gutters.
    pageWindowEl.style.width = `${columnW}px`;
    pageWindowEl.style.height = `${h}px`;

    flowEl.style.height = `${h}px`;
    flowEl.style.columnWidth = `${columnW}px`;
    flowEl.style.columnGap = `${gap}px`;
    flowEl.style.width = `${columnW}px`;
    void flowEl.offsetHeight;

    const totalWidth = flowEl.scrollWidth;
    const usableWidth = Math.max(totalWidth - gap, columnW);
    const pageCount = Math.max(1, Math.ceil(usableWidth / columnStride));
    library.setPageCount(pageCount);
    if (library.currentPage >= pageCount) library.setPage(pageCount - 1);
    const target = library.pendingScrollChunk;
    if (target) {
      const el = flowEl.querySelector(`[data-chunk-id="${target}"]`) as HTMLElement | null;
      if (el) {
        const page = Math.floor(el.offsetLeft / columnStride);
        library.setPage(page);
      }
      library.clearPendingScroll();
    }
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

<svelte:window onkeydown={onKeydown} />

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
        <div class="page-window" bind:this={pageWindowEl}>
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
    max-width: 680px;
    width: 100%;
    align-self: center;
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
    position: relative;
    display: flex;
    justify-content: center;
    /* No overflow:hidden here so the popover (positioned outside the column)
       isn't clipped. Clipping happens on .page-window. */
  }

  .page-window {
    /* Visible page-sized window. Clips multi-column overflow tightly to
       the column so adjacent columns can't bleed into the gutters. */
    overflow: hidden;
    position: relative;
  }

  .book-flow {
    /* Multi-column horizontal pagination: column-width set in JS; columns
       that don't fit overflow horizontally to the right of .page-window,
       which clips them. We translateX between pages. */
    column-fill: auto;
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
