<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { getPage, pageForChunk } from "./lib/pagination.js";

  type Tier = "simple" | "standard" | "scholarly";

  type TranslatorEntry = {
    name: string;
    birth_year: number | null;
    death_year: number | null;
  };

  type ManifestBook = {
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

  type SearchHit = {
    gutenberg_id: number;
    chunk_id: string;
    excerpt: string;
    similarity: number;
  };

  type ChunkRefView = {
    chunk_id: string;
    char_offset_start: number;
    char_offset_end: number;
  };

  type BookView = {
    gutenberg_id: number;
    title: string;
    translators: string[];
    canonical_text: string;
    chunks: ChunkRefView[];
  };

  type GlossaryEntry = {
    term: string;
    gloss: string;
    substrate_term?: string | null;
  };
  type FaithfulnessScore = {
    support: number;
    contradiction_max: number;
    introductions: string[];
  };
  type FaithfulnessVerdict = {
    faithful: boolean;
    support_floor: number;
    contradiction_ceiling: number;
  };
  type FathomResult = {
    paraphrase: string;
    glossary: GlossaryEntry[];
    tier: Tier;
    resolution: string;
    model: string;
    identified_terms: string[];
    faithfulness?: FaithfulnessScore | null;
    faithfulness_verdict?: FaithfulnessVerdict | null;
  };

  type DownloadProgress = {
    model: string;
    bytes: number;
    total: number | null;
  };

  // ----- state -----
  let manifest: ManifestBook[] = $state([]);
  let manifestLoading = $state(true);
  let manifestError: string | null = $state(null);

  let embedderReady = $state(false);
  let embedderError: string | null = $state(null);
  let prewarmedCount = $state(0);

  let query = $state("");
  let searchHits: SearchHit[] = $state([]);
  let searching = $state(false);

  let loadedBook: BookView | null = $state(null);
  let loadingBook = $state(false);

  let tier: Tier = $state("standard");
  let paraphraseResult: FathomResult | null = $state(null);
  let paraphraseBusy = $state(false);
  let paraphraseError: string | null = $state(null);

  let downloadProgress: Record<string, DownloadProgress> = $state({});

  let lastSelectionText = $state("");

  let currentPage = $state(0);

  // Temporary diagnostic HUD — captures console.error lines for in-app visibility
  // while we debug the silent-paraphrase issue. Remove with the diagnostics.
  let debugLog: string[] = $state([]);
  const origConsoleError = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    origConsoleError(...args);
    const line = args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
    debugLog = [...debugLog.slice(-19), line];
  };

  const modelLabels: Record<string, string> = {
    "gemma3-4b": "Loading paraphrase model (Gemma 3 4B)",
    "deberta-nli": "Loading faithfulness model (DeBERTa NLI)",
    "deberta-nli-tokenizer": "Loading faithfulness tokenizer",
    "bge-small": "Loading embedding model (bge-small)",
    "bge-small-tokenizer": "Loading embedding tokenizer",
  };

  const encoder = new TextEncoder();
  function utf8ByteLength(s: string): number {
    return encoder.encode(s).length;
  }

  let leftListItems = $derived.by(() => {
    if (query.trim().length > 0) {
      return searchHits.map((h) => {
        const book = manifest.find((b) => b.gutenberg_id === h.gutenberg_id);
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
    return [...manifest]
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

  // Paragraph view of the loaded book: split canonical_text on '\n\n',
  // advancing a UTF-8 byte cursor. The cursor ties DOM selections back to
  // document-absolute byte offsets the Rust side indexes into canonical_text.
  let paragraphs = $derived.by(() => {
    if (!loadedBook) return [];
    const text = loadedBook.canonical_text;
    const result: { chunkId: string; byteStart: number; text: string }[] = [];
    let offset = 0;
    const SEPARATOR_BYTES = 2; // '\n\n' is two ASCII bytes
    for (const para of text.split("\n\n")) {
      const chunk = loadedBook.chunks.find(
        (c) => c.char_offset_start <= offset && offset < c.char_offset_end,
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

  $effect(() => {
    // Reset to first page whenever a different book is loaded.
    loadedBook?.gutenberg_id;
    currentPage = 0;
  });

  let currentPageBounds = $derived(getPage(paragraphs, currentPage));

  onMount(async () => {
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
      prewarmedCount = await invoke<number>("library_prewarm_shards", { limit: 64 });
    } catch (e) {
      console.warn("prewarm failed:", e);
    }

    await embedderPromise;
  });

  async function retryManifest() {
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

  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const q = query.trim();
    if (searchTimer) clearTimeout(searchTimer);
    if (q.length === 0 || !embedderReady) {
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
      } catch (e) {
        searchHits = [];
      } finally {
        searching = false;
      }
    }, 200);
  });

  async function openBook(gutenberg_id: number, chunkIdToScrollTo?: string) {
    if (loadedBook?.gutenberg_id === gutenberg_id) {
      if (chunkIdToScrollTo) scrollToChunk(chunkIdToScrollTo);
      return;
    }
    loadingBook = true;
    paraphraseResult = null;
    paraphraseError = null;
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
      paraphraseError = e instanceof Error ? e.message : String(e);
    } finally {
      loadingBook = false;
    }
  }

  function scrollToChunk(chunkId: string) {
    const el = document.querySelector(`[data-chunk-id="${chunkId}"]`);
    if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  function scrollReaderToTop() {
    const el = document.querySelector(".reader") as HTMLElement | null;
    if (el) el.scrollTop = 0;
  }

  function pageBack() {
    if (currentPage > 0) {
      currentPage -= 1;
      window.getSelection()?.removeAllRanges();
      setTimeout(scrollReaderToTop, 0);
    }
  }

  function pageForward() {
    if (currentPage < currentPageBounds.pageCount - 1) {
      currentPage += 1;
      window.getSelection()?.removeAllRanges();
      setTimeout(scrollReaderToTop, 0);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (!loadedBook) return;
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
    if (e.key === "ArrowLeft") { e.preventDefault(); pageBack(); }
    else if (e.key === "ArrowRight") { e.preventDefault(); pageForward(); }
    else if (e.key === " " && e.shiftKey) { e.preventDefault(); pageBack(); }
    else if (e.key === " ") { e.preventDefault(); pageForward(); }
  }

  /**
   * Direct DOM endpoint → document-absolute UTF-8 byte offset.
   * Handles the v0.2 flat <p>{text}</p> markup. For v0.21 rich markup
   * (inline <em>/<strong>) the pure module in lib/selection.ts is the
   * reference implementation; we'll port back to it then.
   */
  function endpointToByte(
    container: Node,
    charOffset: number,
    fallback: "start" | "end",
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
      // Text-node container: charOffset is a UTF-16 code-unit index into
      // (container as Text).data. Sum preceding siblings within paraEl,
      // then add the prefix within this text node.
      let bytes = 0;
      for (const sib of Array.from(paraEl.childNodes)) {
        if (sib === container) break;
        bytes += utf8ByteLength(sib.textContent ?? "");
      }
      bytes += utf8ByteLength((container as Text).data.slice(0, charOffset));
      return paraByteStart + bytes;
    }

    // Element container: charOffset is a child index. Triple-click case
    // (container === paraEl) is the only one we need today.
    const el = container as HTMLElement;
    const limit = Math.min(charOffset, el.childNodes.length);
    let bytes = 0;
    for (let i = 0; i < limit; i++) {
      bytes += utf8ByteLength(el.childNodes[i].textContent ?? "");
    }
    return paraByteStart + bytes;
  }

  async function paraphraseSelection() {
    if (!loadedBook) {
      console.error("[fathom:paraphrase] bail: no loadedBook");
      return;
    }
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) {
      console.error("[fathom:paraphrase] bail: no/collapsed selection");
      return;
    }
    const range = selection.getRangeAt(0);
    const selText = selection.toString();
    if (selText.trim().length === 0) {
      console.error("[fathom:paraphrase] bail: empty selText");
      return;
    }
    console.error("[fathom:paraphrase] selText:", JSON.stringify(selText.slice(0, 80)));
    console.error("[fathom:paraphrase] startContainer:", range.startContainer.nodeType, range.startContainer.nodeName, "offset:", range.startOffset);
    console.error("[fathom:paraphrase] endContainer:", range.endContainer.nodeType, range.endContainer.nodeName, "offset:", range.endOffset);

    let startByte = endpointToByte(range.startContainer, range.startOffset, "start");
    let endByte = endpointToByte(range.endContainer, range.endOffset, "end");

    // Cross-endpoint snap: if one side fell back to document bounds because
    // it was outside any paragraph, but the other side IS inside a paragraph,
    // snap the gutter side to that paragraph instead of to the document.
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

    console.error("[fathom:paraphrase] startByte:", startByte, "endByte:", endByte, "startParaEl?", !!startParaEl, "endParaEl?", !!endParaEl);

    if (endByte <= startByte) {
      console.error("[fathom:paraphrase] bail: endByte <= startByte");
      return;
    }

    console.error("[fathom:paraphrase] invoking library_paraphrase_selection");

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
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[fathom:paraphrase] invoke FAILED:", msg);
      paraphraseError = msg || "paraphrase failed";
    } finally {
      paraphraseBusy = false;
    }
  }

  function pctOrNull(p?: DownloadProgress) {
    if (!p || !p.total) return null;
    return Math.min(100, Math.round((p.bytes / p.total) * 100));
  }

  function snippet(s: string, n = 90): string {
    return s.length > n ? s.slice(0, n).trimEnd() + "…" : s;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<header class="app-header">
  <div class="brand">
    <h1>Fathom</h1>
    <p>Read philosophy at your depth without losing the words.</p>
  </div>
  <div class="search">
    <input
      type="search"
      bind:value={query}
      placeholder={embedderReady ? "Search the library…" : "Loading embedding model…"}
      aria-label="Search the library"
      disabled={!embedderReady}
    />
    {#if searching}
      <span class="searching">searching…</span>
    {:else if !embedderReady && !embedderError}
      {@const m = downloadProgress["bge-small"]}
      <span class="searching">
        loading model{m && m.total ? ` · ${pctOrNull(m) ?? 0}%` : "…"}
      </span>
    {:else if embedderError}
      <span class="searching error" title={embedderError}>embedder offline</span>
    {/if}
  </div>
</header>

<main class="library">
  <aside class="left-column">
    {#if manifestLoading}
      <p class="empty">Loading library…</p>
    {:else if manifestError}
      <div class="offline">
        <p>Library offline.</p>
        <button class="retry" onclick={retryManifest}>Retry</button>
      </div>
    {:else if leftListItems.length === 0}
      <p class="empty">
        {query.trim().length > 0 ? "No hits for this query." : "Library is empty."}
      </p>
    {:else}
      <ul>
        {#each leftListItems as item (item.gutenberg_id + ":" + item.chunk_id)}
          <li>
            <button
              class="hit"
              class:active={loadedBook?.gutenberg_id === item.gutenberg_id}
              onclick={() => openBook(item.gutenberg_id, item.chunk_id || undefined)}
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

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <section class="reader" aria-label="Book reader" onmouseup={paraphraseSelection}>
    {#if !loadedBook}
      <div class="empty">
        <p>Pick a book on the left.</p>
      </div>
    {:else if loadingBook}
      <div class="empty">
        <p>Loading {loadedBook.title}…</p>
      </div>
    {:else}
      <article>
        <header class="book-header">
          <h2>{loadedBook.title}</h2>
          {#if loadedBook.translators.length > 0}
            <p class="translators">
              tr. {loadedBook.translators.join(", ")}
            </p>
          {/if}
          <div class="pagination">
            <button class="page-btn" onclick={pageBack} disabled={currentPage === 0} aria-label="Previous page">&#x2039;</button>
            <span class="page-indicator">page {currentPage + 1} of {currentPageBounds.pageCount}</span>
            <button class="page-btn" onclick={pageForward} disabled={currentPage >= currentPageBounds.pageCount - 1} aria-label="Next page">&#x203a;</button>
          </div>
        </header>
        <div class="paragraphs">
          {#each currentPageBounds.paragraphs as p, i (currentPageBounds.startParaIndex + i)}
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

  <aside class="paraphrase-pane">
    <div class="tier-control">
      <span class="control-label">Depth</span>
      <div class="tier-buttons">
        {#each ["simple", "standard", "scholarly"] as t (t)}
          <button
            class="tier-btn"
            class:active={tier === t}
            onclick={() => (tier = t as Tier)}
          >
            {t}
          </button>
        {/each}
      </div>
    </div>

    {#if lastSelectionText}
      <section class="selection-preview">
        <h3>Selection</h3>
        <p>{lastSelectionText}</p>
      </section>
    {/if}

    {#if paraphraseBusy}
      <p class="busy">fathoming…</p>
    {/if}

    {#if paraphraseBusy && Object.keys(downloadProgress).length > 0}
      <section class="downloads">
        {#each Object.values(downloadProgress) as p}
          {#if p.bytes < (p.total ?? Infinity)}
            <div class="download">
              <div class="download-label">
                {modelLabels[p.model] ?? p.model}
              </div>
              <div class="download-meta">
                {Math.round(p.bytes / 1_000_000)} MB
                {#if p.total}
                  / {Math.round(p.total / 1_000_000)} MB
                {/if}
                {#if pctOrNull(p) !== null}
                  · {pctOrNull(p)}%
                {/if}
              </div>
              <div class="bar">
                <div class="bar-fill" style="width: {pctOrNull(p) ?? 0}%"></div>
              </div>
            </div>
          {/if}
        {/each}
      </section>
    {/if}

    {#if paraphraseError}
      <section class="error-box">{paraphraseError}</section>
    {/if}

    {#if paraphraseResult}
      <section class="paraphrase-block">
        <header>
          <h3>Paraphrase</h3>
          <div class="paraphrase-meta">
            <span>{paraphraseResult.resolution}</span>
            <span class="dot">·</span>
            <span>{paraphraseResult.tier}</span>
            <span class="dot">·</span>
            <span class="model">{paraphraseResult.model}</span>
          </div>
        </header>
        <p class="paraphrase-text">{paraphraseResult.paraphrase}</p>

        {#if paraphraseResult.faithfulness}
          {@const f = paraphraseResult.faithfulness}
          {@const v = paraphraseResult.faithfulness_verdict}
          <div
            class="faithfulness"
            class:warn={v ? !v.faithful : false}
            title={v ? `passes when support > ${v.support_floor} and contradiction < ${v.contradiction_ceiling}` : ""}
          >
            <div class="faithfulness-summary">
              <span>support {f.support.toFixed(2)}</span>
              <span class="dot">·</span>
              <span>contradiction {f.contradiction_max.toFixed(2)}</span>
              {#if f.introductions.length > 0}
                <span class="dot">·</span>
                <span>{f.introductions.length} unsupported {f.introductions.length === 1 ? "sentence" : "sentences"}</span>
              {/if}
            </div>
            {#if f.introductions.length > 0}
              <details>
                <summary>Show unsupported sentences</summary>
                <ul>
                  {#each f.introductions as s}
                    <li>{s}</li>
                  {/each}
                </ul>
              </details>
            {/if}
          </div>
        {/if}

        {#if paraphraseResult.glossary.length > 0}
          <h4>Glossary</h4>
          <dl class="glossary">
            {#each paraphraseResult.glossary as g}
              <div class="term">
                <dt>
                  <span class="term-name">{g.term}</span>
                  {#if g.substrate_term}
                    <span class="term-substrate">{g.substrate_term}</span>
                  {/if}
                </dt>
                <dd>{g.gloss}</dd>
              </div>
            {/each}
          </dl>
        {/if}
      </section>
    {/if}
  </aside>
</main>

{#if debugLog.length > 0}
  <div class="debug-hud">
    <div class="debug-hud-header">
      debug ({debugLog.length})
      <button onclick={() => (debugLog = [])}>clear</button>
    </div>
    <pre>{debugLog.join("\n")}</pre>
  </div>
{/if}

<style>
  :global(:root) {
    color-scheme: light dark;
  }
  :global(body) {
    margin: 0;
    font-family: "Iowan Old Style", "Charter", "Georgia", serif;
    background: #faf8f3;
    color: #1f1a13;
  }
  :global(*) {
    box-sizing: border-box;
  }

  .app-header {
    display: flex;
    align-items: center;
    gap: 2rem;
    padding: 0.75rem 1.25rem;
    border-bottom: 1px solid rgba(31, 26, 19, 0.12);
    background: #fffdf8;
  }
  .brand h1 {
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 1.1rem;
    margin: 0;
    letter-spacing: 0.02em;
  }
  .brand p {
    margin: 0;
    font-size: 0.85rem;
    opacity: 0.6;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .search input {
    flex: 1;
    padding: 0.55rem 0.8rem;
    font: inherit;
    font-size: 0.95rem;
    background: #fff;
    border: 1px solid rgba(31, 26, 19, 0.18);
    border-radius: 4px;
  }
  .search input:focus {
    outline: 2px solid #b3793e;
    outline-offset: -1px;
    border-color: transparent;
  }
  .search input:disabled {
    background: rgba(31, 26, 19, 0.04);
    color: rgba(31, 26, 19, 0.5);
    cursor: progress;
  }
  .searching {
    font-size: 0.8rem;
    opacity: 0.6;
    font-family: "IBM Plex Mono", monospace;
  }
  .searching.error {
    color: #6c2f10;
    opacity: 0.85;
  }

  .library {
    display: grid;
    grid-template-columns: 22rem 1fr 24rem;
    height: calc(100vh - 4rem);
    overflow: hidden;
  }

  .left-column {
    overflow-y: auto;
    border-right: 1px solid rgba(31, 26, 19, 0.12);
    background: #fcfaf5;
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
    border: 0;
    padding: 0.65rem 0.9rem;
    cursor: pointer;
    border-bottom: 1px solid rgba(31, 26, 19, 0.05);
    font: inherit;
  }
  .hit:hover {
    background: rgba(179, 121, 62, 0.06);
  }
  .hit.active {
    background: rgba(179, 121, 62, 0.14);
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
    border: 1px solid rgba(31, 26, 19, 0.25);
    padding: 0.3rem 0.7rem;
    font: inherit;
    font-size: 0.85rem;
    border-radius: 3px;
    cursor: pointer;
  }
  .retry:hover {
    background: rgba(179, 121, 62, 0.08);
  }

  .reader {
    overflow-y: auto;
    padding: 2rem 3rem;
    user-select: text;
  }
  .reader .book-header {
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid rgba(31, 26, 19, 0.12);
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
    border: 1px solid rgba(31, 26, 19, 0.2);
    padding: 0.15rem 0.55rem;
    font: inherit;
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    border-radius: 3px;
    color: inherit;
  }
  .page-btn:hover:not(:disabled) {
    background: rgba(179, 121, 62, 0.08);
  }
  .page-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .page-indicator {
    opacity: 0.55;
    letter-spacing: 0.04em;
  }
  .paragraphs p {
    line-height: 1.7;
    margin: 0 0 1.1rem;
    /* Preserve source-string whitespace so el.innerText matches
       utf8ByteLength(para) byte-for-byte. Gutenberg canonical_text
       contains no in-paragraph newlines today, but user-text in v0.21 may. */
    white-space: pre-wrap;
  }
  .paragraphs p::selection {
    background: rgba(179, 121, 62, 0.28);
  }

  .paraphrase-pane {
    overflow-y: auto;
    padding: 1.25rem 1.25rem 2rem;
    border-left: 1px solid rgba(31, 26, 19, 0.12);
    background: #fffdf8;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .tier-control {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .control-label {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.55;
  }
  .tier-buttons {
    display: flex;
    gap: 0.25rem;
  }
  .tier-btn {
    background: transparent;
    border: 1px solid rgba(31, 26, 19, 0.18);
    padding: 0.25rem 0.55rem;
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
    border-radius: 3px;
  }
  .tier-btn.active {
    background: #b3793e;
    color: #fffdf8;
    border-color: #b3793e;
  }

  .selection-preview h3 {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: 0 0 0.4rem;
    opacity: 0.55;
  }
  .selection-preview p {
    margin: 0;
    font-style: italic;
    line-height: 1.45;
    font-size: 0.9rem;
  }

  .busy {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.85rem;
    opacity: 0.6;
  }

  .downloads {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .download-label {
    font-size: 0.85rem;
  }
  .download-meta {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.75rem;
    opacity: 0.6;
    margin-bottom: 0.25rem;
  }
  .bar {
    height: 3px;
    background: rgba(31, 26, 19, 0.1);
    overflow: hidden;
    border-radius: 1.5px;
  }
  .bar-fill {
    height: 100%;
    background: #b3793e;
    transition: width 0.2s;
  }

  .error-box {
    background: #f6e7e0;
    color: #6c2f10;
    padding: 0.65rem 0.85rem;
    border-radius: 4px;
    font-size: 0.88rem;
  }

  .paraphrase-block header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 0.5rem;
  }
  .paraphrase-block h3 {
    font-family: "IBM Plex Sans", sans-serif;
    margin: 0;
    font-size: 1rem;
  }
  .paraphrase-meta {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.75rem;
    opacity: 0.55;
  }
  .paraphrase-meta .dot {
    margin: 0 0.35em;
  }
  .paraphrase-text {
    line-height: 1.55;
    margin: 0 0 0.9rem;
  }

  .faithfulness {
    border: 1px solid rgba(31, 26, 19, 0.15);
    border-radius: 4px;
    padding: 0.5rem 0.7rem;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.78rem;
    margin-bottom: 0.9rem;
  }
  .faithfulness.warn {
    border-color: #b35a3e;
    background: #fbf0e7;
  }
  .faithfulness-summary {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    opacity: 0.85;
  }
  .faithfulness-summary .dot {
    opacity: 0.5;
  }
  .faithfulness details {
    margin-top: 0.45rem;
  }
  .faithfulness details summary {
    cursor: pointer;
    opacity: 0.65;
  }
  .faithfulness details ul {
    padding-left: 1.1rem;
    font-family: inherit;
  }

  .glossary {
    margin: 0;
  }
  .term {
    margin-bottom: 0.7rem;
  }
  .term dt {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.9rem;
  }
  .term .term-substrate {
    font-style: italic;
    opacity: 0.6;
    font-size: 0.85rem;
  }
  .term dd {
    margin: 0.15rem 0 0 0;
    font-size: 0.88rem;
    line-height: 1.5;
    opacity: 0.9;
  }

  /* Temporary diagnostic HUD — remove with the console.error traces. */
  .debug-hud {
    position: fixed;
    bottom: 0.5rem;
    left: 0.5rem;
    max-width: 50vw;
    max-height: 35vh;
    background: rgba(0, 0, 0, 0.82);
    color: #fffdf8;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.7rem;
    line-height: 1.35;
    padding: 0.4rem 0.6rem;
    border-radius: 4px;
    overflow: auto;
    z-index: 9999;
    user-select: text;
  }
  .debug-hud-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    opacity: 0.6;
    margin-bottom: 0.25rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .debug-hud-header button {
    background: transparent;
    color: inherit;
    border: 1px solid rgba(255, 255, 255, 0.3);
    padding: 0.1rem 0.4rem;
    font: inherit;
    font-size: 0.65rem;
    cursor: pointer;
    border-radius: 2px;
  }
  .debug-hud pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
