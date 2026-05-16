<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type Tier = "simple" | "standard" | "scholarly";
  type Mode = "auto" | "curated" | "jit" | "no-substrate";

  type TraditionSummary = { tradition: string; passage_count: number };
  type ThemeSummary = { slug: string; label: string; passage_count: number };
  type PassageSummary = {
    id: string;
    fingerprint: string;
    author: string;
    title: string;
    tradition: string;
    themes: string[];
  };
  type TermView = { term: string; substrate: string; gloss: string };
  type PassageDetail = {
    id: string;
    fingerprint: string;
    author: string;
    title: string;
    translation: string;
    language: string;
    tradition: string;
    themes: string[];
    terms: TermView[];
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
  type FathomResult = {
    paraphrase: string;
    glossary: GlossaryEntry[];
    tier: Tier;
    resolution: "curated" | "jit" | "no-substrate";
    model: string;
    identified_terms: string[];
    faithfulness?: FaithfulnessScore | null;
  };

  type DownloadProgress = {
    model: string;
    bytes: number;
    total: number | null;
  };

  // ----- state -----
  let traditions: TraditionSummary[] = $state([]);
  let themes: ThemeSummary[] = $state([]);
  let activeFilter: { kind: "theme" | "tradition"; value: string } | null = $state(null);
  let passages: PassageSummary[] = $state([]);
  let selectedPassage: PassageDetail | null = $state(null);

  let tier: Tier = $state("standard");
  let mode: Mode = $state("curated");
  let result: FathomResult | null = $state(null);
  let error: string | null = $state(null);
  let busy = $state(false);
  let downloadProgress: Record<string, DownloadProgress> = $state({});

  const modelLabels: Record<string, string> = {
    "gemma3-4b": "Loading paraphrase model (Gemma 3 4B)",
    "deberta-nli": "Loading faithfulness model (DeBERTa NLI)",
  };

  // Glossary entries the model surfaced that weren't already shown in the
  // passage's "Terms of art" panel. On the curated path the model just
  // recites the lexicon, so this is usually empty; on JIT/no-substrate
  // paths it's where newly-identified terms appear.
  let newGlossaryTerms = $derived.by(() => {
    if (!result || !selectedPassage) return [];
    const known = new Set(
      selectedPassage.terms.map((t) => t.term.toLowerCase().trim()),
    );
    return result.glossary.filter(
      (g) => !known.has(g.term.toLowerCase().trim()),
    );
  });

  // ----- lifecycle -----
  onMount(async () => {
    [traditions, themes] = await Promise.all([
      invoke<TraditionSummary[]>("library_traditions"),
      invoke<ThemeSummary[]>("library_themes"),
    ]);
    // Default to a curated theme so the right pane is never empty
    await selectFilter("theme", "freedom-and-fate");
    // Listen for model download progress
    await listen<DownloadProgress>("fathom://download-progress", (e) => {
      downloadProgress = { ...downloadProgress, [e.payload.model]: e.payload };
    });
  });

  async function selectFilter(kind: "theme" | "tradition", value: string) {
    activeFilter = { kind, value };
    selectedPassage = null;
    result = null;
    error = null;
    const args = kind === "theme" ? { theme: value } : { tradition: value };
    passages = await invoke<PassageSummary[]>("library_passages", args);
  }

  async function openPassage(id: string) {
    result = null;
    error = null;
    selectedPassage = await invoke<PassageDetail | null>("library_get_passage", { id });
  }

  async function fathomThis() {
    if (!selectedPassage) return;
    busy = true;
    error = null;
    result = null;
    try {
      // Reconstruct the passage text from fingerprint + terms is lossy; use the
      // fingerprint itself as the text Robin paraphrases (lookup_canonical will
      // match it back to the same lexicon entry).
      const text = selectedPassage.fingerprint;
      result = await invoke<FathomResult>("paraphrase", {
        args: { text, tier, mode },
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      error = msg || "paraphrase failed";
    } finally {
      busy = false;
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

<main>
  <aside>
    <header>
      <h1>Fathom</h1>
      <p>Read philosophy at your depth without losing the words.</p>
    </header>

    <nav>
      <h2>Themes</h2>
      <ul class="filters">
        {#each themes as t}
          <li>
            <button
              class="filter"
              class:active={activeFilter?.kind === "theme" && activeFilter.value === t.slug}
              onclick={() => selectFilter("theme", t.slug)}
              disabled={t.passage_count === 0}
            >
              <span>{t.label}</span>
              <em>{t.passage_count}</em>
            </button>
          </li>
        {/each}
      </ul>

      <h2>Traditions</h2>
      <ul class="filters">
        {#each traditions as t}
          <li>
            <button
              class="filter"
              class:active={activeFilter?.kind === "tradition" && activeFilter.value === t.tradition}
              onclick={() => selectFilter("tradition", t.tradition)}
            >
              <span>{t.tradition}</span>
              <em>{t.passage_count}</em>
            </button>
          </li>
        {/each}
      </ul>
    </nav>
  </aside>

  <section class="content">
    <!-- Passage list column -->
    <div class="passages">
      {#if activeFilter}
        <h3>
          {#if activeFilter.kind === "theme"}
            {themes.find((t) => t.slug === activeFilter?.value)?.label ?? activeFilter.value}
          {:else}
            {activeFilter.value}
          {/if}
          <span class="count">· {passages.length}</span>
        </h3>
      {/if}
      <ul>
        {#each passages as p}
          <li>
            <button
              class="passage-card"
              class:active={selectedPassage?.id === p.id}
              onclick={() => openPassage(p.id)}
            >
              <div class="passage-meta">
                <span class="author">{p.author}</span>
                <span class="dot">·</span>
                <span class="title">{p.title}</span>
              </div>
              <div class="passage-snippet">{snippet(p.fingerprint, 80)}</div>
            </button>
          </li>
        {/each}
      </ul>
    </div>

    <!-- Passage detail + paraphrase column -->
    <div class="reader">
      {#if !selectedPassage}
        <div class="empty">
          <p>Pick a passage on the left.</p>
        </div>
      {:else}
        <article>
          <header class="passage-header">
            <div class="passage-author">{selectedPassage.author}</div>
            <h2>{selectedPassage.title}</h2>
            <div class="passage-source">
              <span>{selectedPassage.tradition}</span>
              <span class="dot">·</span>
              <span>{selectedPassage.language}</span>
            </div>
          </header>

          <section class="fingerprint">
            <p>{selectedPassage.fingerprint}</p>
          </section>

          {#if selectedPassage.terms.length > 0}
            <section class="terms">
              <h3>Terms of art in this passage</h3>
              <dl>
                {#each selectedPassage.terms as t}
                  <div class="term">
                    <dt>
                      <span class="term-name">{t.term}</span>
                      <span class="term-substrate">{t.substrate}</span>
                    </dt>
                    <dd>{t.gloss}</dd>
                  </div>
                {/each}
              </dl>
            </section>
          {/if}

          <section class="actions">
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
            <button
              class="fathom-btn"
              onclick={fathomThis}
              disabled={busy}
            >
              {busy ? "fathoming…" : "Fathom this passage"}
            </button>
          </section>

          {#if busy && Object.keys(downloadProgress).length > 0}
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

          {#if error}
            <section class="error-box">{error}</section>
          {/if}

          {#if result}
            <section class="paraphrase-block">
              <header>
                <h3>Paraphrase</h3>
                <div class="paraphrase-meta">
                  <span>{result.resolution}</span>
                  <span class="dot">·</span>
                  <span>{result.tier}</span>
                  <span class="dot">·</span>
                  <span class="model">{result.model}</span>
                </div>
              </header>
              <p class="paraphrase-text">{result.paraphrase}</p>

              {#if result.faithfulness}
                {@const f = result.faithfulness}
                <div
                  class="faithfulness"
                  class:warn={f.support < 0.5 || f.contradiction_max > 0.1}
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

              {#if newGlossaryTerms.length > 0}
                <h4>New terms surfaced</h4>
                <dl class="glossary">
                  {#each newGlossaryTerms as g}
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
        </article>
      {/if}
    </div>
  </section>
</main>

<style>
  main {
    display: grid;
    grid-template-columns: 260px 1fr;
    min-height: 100vh;
  }

  /* ---- Sidebar ---- */
  aside {
    background: var(--paper-deep);
    border-right: 1px solid var(--rule);
    padding: 1.5rem 1rem;
    overflow-y: auto;
    height: 100vh;
    position: sticky;
    top: 0;
  }
  aside header h1 {
    font-family: var(--serif);
    font-weight: 600;
    font-size: 1.7rem;
    letter-spacing: -0.01em;
    margin: 0 0 0.2rem;
    color: var(--ink);
  }
  aside header p {
    margin: 0 0 1.6rem;
    color: var(--ink-faint);
    font-size: 0.82rem;
    font-style: italic;
  }
  nav h2 {
    font-size: 0.7rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-faint);
    margin: 1.2rem 0 0.4rem;
  }
  nav h2:first-child {
    margin-top: 0;
  }
  ul.filters {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  button.filter {
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    padding: 0.32rem 0.5rem;
    border-radius: 4px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    color: var(--ink-soft);
    font-size: 0.86rem;
    transition: background 0.12s ease;
  }
  button.filter em {
    font-style: normal;
    font-size: 0.72rem;
    color: var(--ink-faint);
    font-variant-numeric: tabular-nums;
  }
  button.filter:hover {
    background: rgba(0, 0, 0, 0.04);
  }
  button.filter.active {
    background: var(--ink);
    color: var(--paper);
  }
  button.filter.active em {
    color: var(--paper-deep);
  }
  button.filter:disabled {
    opacity: 0.3;
    cursor: default;
  }

  /* ---- Content area ---- */
  .content {
    display: grid;
    grid-template-columns: 320px 1fr;
    min-height: 100vh;
  }
  .passages {
    border-right: 1px solid var(--rule);
    padding: 1.5rem 1rem;
    overflow-y: auto;
    height: 100vh;
    position: sticky;
    top: 0;
  }
  .passages h3 {
    font-family: var(--serif);
    font-weight: 600;
    font-size: 1.05rem;
    margin: 0 0 0.8rem;
    color: var(--ink);
  }
  .passages h3 .count {
    color: var(--ink-faint);
    font-weight: 400;
    font-size: 0.85rem;
  }
  .passages ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  button.passage-card {
    width: 100%;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-bottom: 1px solid var(--rule);
    padding: 0.7rem 0.6rem;
    color: inherit;
    transition: background 0.12s ease;
  }
  button.passage-card:hover {
    background: rgba(0, 0, 0, 0.025);
  }
  button.passage-card.active {
    background: var(--paper-deep);
    border-color: var(--rule);
    border-radius: 4px;
  }
  .passage-meta {
    font-size: 0.7rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-faint);
    margin-bottom: 0.3rem;
  }
  .passage-meta .author {
    font-weight: 600;
    color: var(--accent);
  }
  .passage-snippet {
    font-family: var(--serif);
    font-size: 0.92rem;
    line-height: 1.45;
    color: var(--ink-soft);
  }

  /* ---- Reader ---- */
  .reader {
    padding: 3rem 2.5rem;
    max-width: 760px;
  }
  .empty {
    color: var(--ink-faint);
    font-style: italic;
  }
  .passage-header {
    margin-bottom: 2rem;
  }
  .passage-author {
    font-size: 0.78rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--accent);
    font-weight: 600;
  }
  .passage-header h2 {
    font-family: var(--serif);
    font-weight: 600;
    font-size: 2rem;
    line-height: 1.15;
    margin: 0.2rem 0 0.4rem;
    letter-spacing: -0.01em;
  }
  .passage-source {
    color: var(--ink-faint);
    font-size: 0.82rem;
  }
  .fingerprint {
    border-left: 3px solid var(--accent-soft);
    padding: 0.2rem 0 0.2rem 1.3rem;
    margin-bottom: 2rem;
  }
  .fingerprint p {
    font-family: var(--serif);
    font-size: 1.18rem;
    line-height: 1.6;
    color: var(--ink);
    margin: 0;
    font-style: italic;
  }

  /* ---- Terms section ---- */
  .terms h3,
  .paraphrase-block h3 {
    font-family: var(--serif);
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--ink);
    margin: 0 0 0.8rem;
    padding-bottom: 0.3rem;
    border-bottom: 1px solid var(--rule);
  }
  .paraphrase-block h4 {
    font-family: var(--serif);
    font-weight: 600;
    font-size: 0.95rem;
    margin: 1.6rem 0 0.7rem;
    color: var(--ink);
  }
  dl {
    margin: 0;
  }
  .term {
    padding: 0.55rem 0;
    border-bottom: 1px dotted var(--rule);
  }
  .term:last-child {
    border-bottom: none;
  }
  dt {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    margin-bottom: 0.2rem;
  }
  .term-name {
    font-family: var(--serif);
    font-weight: 600;
    color: var(--ink);
  }
  .term-substrate {
    font-family: var(--mono);
    font-size: 0.78rem;
    color: var(--accent);
    background: rgba(138, 90, 43, 0.07);
    padding: 0.04rem 0.4rem;
    border-radius: 3px;
  }
  dd {
    margin: 0;
    color: var(--ink-soft);
    font-size: 0.9rem;
    line-height: 1.5;
  }

  /* ---- Actions ---- */
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin: 2.4rem 0 1.5rem;
    padding: 1rem;
    background: var(--paper-deep);
    border-radius: 6px;
    flex-wrap: wrap;
  }
  .control-label {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-faint);
    margin-right: 0.6rem;
  }
  .tier-control {
    display: flex;
    align-items: center;
  }
  .tier-buttons {
    display: inline-flex;
    border: 1px solid var(--rule);
    border-radius: 4px;
    background: var(--paper);
    overflow: hidden;
  }
  button.tier-btn {
    background: transparent;
    border: none;
    padding: 0.42rem 0.85rem;
    font-size: 0.86rem;
    color: var(--ink-soft);
    border-right: 1px solid var(--rule);
    text-transform: capitalize;
  }
  button.tier-btn:last-child {
    border-right: none;
  }
  button.tier-btn.active {
    background: var(--ink);
    color: var(--paper);
  }
  button.fathom-btn {
    background: var(--ink);
    color: var(--paper);
    border: none;
    padding: 0.55rem 1.4rem;
    border-radius: 4px;
    font-size: 0.9rem;
    font-weight: 500;
    letter-spacing: 0.01em;
  }
  button.fathom-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ---- Downloads ---- */
  .downloads {
    margin: 1rem 0;
    padding: 1rem;
    background: var(--paper-deep);
    border: 1px dashed var(--rule);
    border-radius: 6px;
  }
  .download + .download {
    margin-top: 0.8rem;
  }
  .download-label {
    font-size: 0.88rem;
    color: var(--ink-soft);
    margin-bottom: 0.2rem;
  }
  .download-meta {
    font-family: var(--mono);
    font-size: 0.76rem;
    color: var(--ink-faint);
    margin-bottom: 0.3rem;
  }
  .bar {
    height: 4px;
    background: var(--rule);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }

  /* ---- Paraphrase result ---- */
  .paraphrase-block {
    margin-top: 2rem;
  }
  .paraphrase-block header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .paraphrase-meta {
    font-family: var(--mono);
    font-size: 0.74rem;
    color: var(--ink-faint);
    text-transform: lowercase;
  }
  .paraphrase-text {
    font-family: var(--serif);
    font-size: 1.08rem;
    line-height: 1.7;
    color: var(--ink);
    white-space: pre-wrap;
    margin: 0.8rem 0 1rem;
  }

  /* ---- Faithfulness ---- */
  .faithfulness {
    padding: 0.6rem 0.9rem;
    background: rgba(74, 106, 58, 0.08);
    border-left: 3px solid var(--ok);
    border-radius: 0 4px 4px 0;
    font-size: 0.85rem;
    color: var(--ink-soft);
  }
  .faithfulness.warn {
    background: rgba(154, 58, 31, 0.06);
    border-left-color: var(--error);
  }
  .faithfulness-summary {
    font-family: var(--mono);
    font-size: 0.78rem;
    letter-spacing: 0.02em;
  }
  .faithfulness details {
    margin-top: 0.5rem;
  }
  .faithfulness summary {
    font-family: var(--sans);
    cursor: pointer;
    color: var(--accent);
    font-size: 0.82rem;
  }
  .faithfulness ul {
    margin: 0.4rem 0 0;
    padding-left: 1.2rem;
    font-family: var(--serif);
    line-height: 1.5;
  }
  .faithfulness li {
    margin: 0.25rem 0;
  }

  /* ---- Misc ---- */
  .dot {
    color: var(--ink-faint);
    margin: 0 0.3rem;
  }
  .error-box {
    margin: 1rem 0;
    padding: 0.7rem 1rem;
    background: rgba(154, 58, 31, 0.08);
    border-left: 3px solid var(--error);
    color: var(--error);
    border-radius: 0 4px 4px 0;
    font-size: 0.88rem;
  }
  .glossary {
    margin-top: 0.5rem;
  }
</style>
