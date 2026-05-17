<script lang="ts">
  import {
    getLoadedBook,
    getDownloadProgress,
    getLoadBookError,
    modelLabels,
  } from "./use-library.svelte.js";
  import {
    getTier,
    setTier,
    getLastSelectionText,
    isParaphraseBusy,
    getParaphraseError,
    getParaphraseResult,
    paraphraseSelection,
    type Tier,
  } from "./use-paraphrase.svelte.js";

  function pctOrNull(p?: { bytes: number; total: number | null }) {
    if (!p || !p.total) return null;
    return Math.min(100, Math.round((p.bytes / p.total) * 100));
  }

  let displayError = $derived(getParaphraseError() ?? getLoadBookError());
</script>

<aside class="paraphrase-pane">
  <div class="tier-control">
    <span class="control-label">Depth</span>
    <div class="tier-buttons" role="group" aria-label="Paraphrase depth">
      {#each (["simple", "standard", "scholarly"] as Tier[]) as t (t)}
        <button
          class="tier-btn"
          class:active={getTier() === t}
          aria-pressed={getTier() === t}
          onclick={() => setTier(t)}
        >
          {t}
        </button>
      {/each}
    </div>
  </div>

  <button
    class="fathom-trigger"
    onclick={paraphraseSelection}
    disabled={isParaphraseBusy() || !getLoadedBook()}
  >
    Fathom selection
  </button>

  {#if getLastSelectionText()}
    <section class="selection-preview">
      <h3>Selection</h3>
      <p>{getLastSelectionText()}</p>
    </section>
  {/if}

  {#if isParaphraseBusy()}
    <p class="busy">fathoming…</p>
  {/if}

  {#if isParaphraseBusy() && Object.keys(getDownloadProgress()).length > 0}
    <section class="downloads">
      {#each Object.values(getDownloadProgress()) as p}
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

  {#if displayError}
    <section class="error-box">{displayError}</section>
  {/if}

  {#if getParaphraseResult()}
    {@const result = getParaphraseResult()!}
    <section class="paraphrase-block" aria-live="polite" aria-atomic="true">
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
        {@const v = result.faithfulness_verdict}
        {@const isWarn = v ? !v.faithful : false}
        <div
          class="faithfulness"
          class:warn={isWarn}
          title={v ? `passes when support > ${v.support_floor} and contradiction < ${v.contradiction_ceiling}` : ""}
        >
          <div
            class="faithfulness-verdict"
            role="status"
            aria-label={isWarn
              ? `Faithfulness warning: ${f.introductions.length} unsupported ${f.introductions.length === 1 ? "sentence" : "sentences"}`
              : "Faithful: paraphrase aligns with the source"}
          >
            <span class="verdict-glyph" aria-hidden="true">{isWarn ? "⚠" : "✓"}</span>
            <span class="verdict-label">{isWarn ? "Check" : "Faithful"}</span>
          </div>
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

      {#if result.glossary.length > 0}
        <h4>Glossary</h4>
        <dl class="glossary">
          {#each result.glossary as g}
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

<style>
  .paraphrase-pane {
    overflow-y: auto;
    padding: 1.25rem 1.25rem 2rem;
    border-left: 1px solid var(--rule);
    background: var(--panel);
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
    color: inherit;
    border: 1px solid var(--rule-strong);
    padding: 0.25rem 0.55rem;
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
    border-radius: 3px;
  }
  .tier-btn.active {
    background: var(--accent);
    color: var(--accent-contrast);
    border-color: var(--accent);
  }
  .tier-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .fathom-trigger {
    width: 100%;
    background: var(--accent);
    color: var(--accent-contrast);
    border: 1px solid var(--accent);
    padding: 0.45rem 0.7rem;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    border-radius: 4px;
    margin-bottom: 0.7rem;
  }
  .fathom-trigger:hover:not(:disabled) {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
  }
  .fathom-trigger:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .fathom-trigger:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
    background: var(--surface-fill);
    overflow: hidden;
    border-radius: 1.5px;
  }
  .bar-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  @media (prefers-reduced-motion: reduce) {
    .bar-fill {
      transition: none;
    }
  }

  .error-box {
    background: var(--error-bg);
    color: var(--error-ink);
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
    border: 1px solid var(--surface-fill-strong);
    border-radius: 4px;
    padding: 0.5rem 0.7rem;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.78rem;
    margin-bottom: 0.9rem;
  }
  .faithfulness.warn {
    border-color: var(--warn-ink);
    background: var(--warn-bg);
  }
  .faithfulness-verdict {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    font-weight: 600;
    margin-bottom: 0.35rem;
  }
  .faithfulness .verdict-glyph {
    font-size: 0.95rem;
    line-height: 1;
    color: var(--ok-ink);
  }
  .faithfulness.warn .verdict-glyph {
    color: var(--warn-ink);
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
</style>
