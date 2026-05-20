<script lang="ts">
  import { paraphrase, type Tier } from "./use-paraphrase.svelte.js";
  import { library } from "./use-library.svelte.js";

  const POPOVER_WIDTH = 420;
  const VIEWPORT_MARGIN = 16;
  const GAP_FROM_SELECTION = 14;

  let popoverEl: HTMLDivElement | null = $state(null);
  let metaOpen = $state(false);

  // Place the popover in whichever vertical or horizontal gutter has room,
  // preferring the side beside the selection so we never cover the source
  // text. Order: right gutter → left gutter → above → below.
  let position = $derived.by(() => {
    const rect = paraphrase.selectionRect;
    if (!rect) return null;
    const popH = popoverEl?.getBoundingClientRect().height ?? 260;
    const viewportW = window.innerWidth;
    const viewportH = window.innerHeight;

    const rightGutter = viewportW - rect.right - GAP_FROM_SELECTION - VIEWPORT_MARGIN;
    const leftGutter = rect.left - GAP_FROM_SELECTION - VIEWPORT_MARGIN;

    if (rightGutter >= POPOVER_WIDTH) {
      // Sit in the right gutter, vertically aligned with the selection mid.
      const top = clampVertical(rect.top + rect.height / 2 - popH / 2, popH, viewportH);
      return { top, left: rect.right + GAP_FROM_SELECTION };
    }
    if (leftGutter >= POPOVER_WIDTH) {
      const top = clampVertical(rect.top + rect.height / 2 - popH / 2, popH, viewportH);
      return { top, left: rect.left - GAP_FROM_SELECTION - POPOVER_WIDTH };
    }

    // Fall back: above the selection, flipping below if not enough room.
    const preferredTop = rect.top - popH - GAP_FROM_SELECTION;
    const flipBelow = preferredTop < VIEWPORT_MARGIN;
    const top = flipBelow ? rect.bottom + GAP_FROM_SELECTION : preferredTop;
    const center = rect.left + rect.width / 2;
    let left = center - POPOVER_WIDTH / 2;
    left = Math.max(VIEWPORT_MARGIN, Math.min(left, viewportW - POPOVER_WIDTH - VIEWPORT_MARGIN));

    return {
      top: clampVertical(top, popH, viewportH),
      left,
    };
  });

  function clampVertical(top: number, popH: number, viewportH: number): number {
    return Math.max(VIEWPORT_MARGIN, Math.min(top, viewportH - popH - VIEWPORT_MARGIN));
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && paraphrase.popoverOpen) {
      paraphrase.closePopover();
      window.getSelection()?.removeAllRanges();
    }
  }

  function onOutsideClick(e: MouseEvent) {
    if (!paraphrase.popoverOpen) return;
    const target = e.target as Node;
    if (popoverEl?.contains(target)) return;
    // Clicks inside the reader's paragraphs are handled by handleSelection
    // (mouseup will re-open with a new selection). A click anywhere else
    // dismisses the popover.
    const paraEl = (target as HTMLElement).closest?.("[data-byte-start]");
    if (paraEl) return;
    paraphrase.closePopover();
  }

  function setTier(t: Tier) {
    if (paraphrase.tier === t) return;
    paraphrase.tier = t;
    paraphrase.retryWithCurrentTier();
  }
</script>

<svelte:window onkeydown={onKeydown} onmousedown={onOutsideClick} />

{#if paraphrase.popoverOpen && position}
  <div
    bind:this={popoverEl}
    class="popover"
    style="top: {position.top}px; left: {position.left}px; width: {POPOVER_WIDTH}px"
    role="dialog"
    aria-label="Paraphrase"
  >
    <header class="head">
      <div class="tier-buttons" role="group" aria-label="Paraphrase depth">
        {#each (["simple", "standard", "scholarly"] as Tier[]) as t (t)}
          <button
            class="tier-btn"
            class:active={paraphrase.tier === t}
            aria-pressed={paraphrase.tier === t}
            onclick={() => setTier(t)}
          >
            {t}
          </button>
        {/each}
      </div>
      <div class="head-actions">
        <button
          class="icon-btn info"
          aria-label="Paraphrase metadata"
          aria-expanded={metaOpen}
          onclick={() => (metaOpen = !metaOpen)}
        >i</button>
        <button
          class="icon-btn close"
          aria-label="Close"
          onclick={() => { paraphrase.closePopover(); window.getSelection()?.removeAllRanges(); }}
        >×</button>
      </div>
    </header>

    {#if metaOpen && paraphrase.paraphraseResult}
      {@const r = paraphrase.paraphraseResult}
      <div class="meta">
        <div class="meta-row">
          <span>{r.resolution}</span>
          <span class="dot">·</span>
          <span>{r.tier}</span>
          <span class="dot">·</span>
          <span>{r.model}</span>
        </div>
        {#if r.faithfulness && r.faithfulness_verdict}
          {@const f = r.faithfulness}
          {@const v = r.faithfulness_verdict}
          <div class="meta-block">
            <div class="meta-block-title">Faithfulness check</div>
            <div class="meta-row">
              <span>support {f.support.toFixed(2)}</span>
              <span class="dot">·</span>
              <span>contradiction {f.contradiction_max.toFixed(2)}</span>
            </div>
            <div class="meta-row dim">
              <span>passes when support &gt; {v.support_floor.toFixed(2)} and contradiction &lt; {v.contradiction_ceiling.toFixed(2)}</span>
            </div>
            <p class="meta-explain">
              An NLI model scores each paraphrase sentence against the original
              for entailment and contradiction. Support is the mean entailment;
              contradiction is the worst single sentence. The model is
              uncalibrated for classical prose, so treat these as a sanity
              check rather than a verdict.
            </p>
          </div>
        {/if}
      </div>
    {/if}

    {#if paraphrase.paraphraseError}
      <p class="error">{paraphrase.paraphraseError}</p>
    {:else if paraphrase.paraphraseBusy && !paraphrase.paraphraseResult}
      <p class="busy">fathoming…</p>
    {:else if paraphrase.paraphraseResult}
      {@const r = paraphrase.paraphraseResult}
      <p class="paraphrase-text" class:dim={paraphrase.paraphraseBusy}>{r.paraphrase}</p>

      {#if r.faithfulness}
        {@const f = r.faithfulness}
        {@const v = r.faithfulness_verdict}
        {@const isWarn = v ? !v.faithful : false}
        {@const headline = isWarn ? "May drift from the source" : "Stays close to the source"}
        {@const introLabel = f.introductions.length === 1 ? "sentence" : "sentences"}
        {@const introAction = isWarn ? "may go beyond the source" : "may add framing"}
        <div class="chip" class:warn={isWarn}>
          {#if f.introductions.length > 0}
            <details class="chip-detail">
              <summary>
                <span class="glyph" aria-hidden="true">{isWarn ? "⚠" : "✓"}</span>
                <span class="chip-label">{headline}</span>
                <span class="chip-meta chip-meta-sep">{f.introductions.length} {introLabel} {introAction}</span>
              </summary>
              <ul>
                {#each f.introductions as s}
                  <li>{s}</li>
                {/each}
              </ul>
            </details>
          {:else}
            <div class="chip-row">
              <span class="glyph" aria-hidden="true">{isWarn ? "⚠" : "✓"}</span>
              <span class="chip-label">{headline}</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if r.glossary.length > 0}
        <details class="glossary-disclosure">
          <summary>Glossary ({r.glossary.length} {r.glossary.length === 1 ? "term" : "terms"})</summary>
          <dl class="glossary">
            {#each r.glossary as g}
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
        </details>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    z-index: 50;
    background: var(--panel);
    color: var(--ink);
    border: 1px solid var(--rule-strong);
    border-radius: 6px;
    padding: 0.7rem 0.9rem 0.85rem;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18), 0 2px 6px rgba(0, 0, 0, 0.08);
    font-family: "IBM Plex Sans", sans-serif;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.55rem;
  }
  .tier-buttons {
    display: flex;
    gap: 0.2rem;
  }
  .tier-btn {
    background: transparent;
    color: inherit;
    border: 1px solid var(--rule-strong);
    padding: 0.18rem 0.5rem;
    font: inherit;
    font-size: 0.78rem;
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
  .head-actions {
    display: flex;
    gap: 0.25rem;
  }
  .icon-btn {
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    width: 22px;
    height: 22px;
    border-radius: 3px;
    font: inherit;
    font-size: 0.85rem;
    line-height: 1;
    cursor: pointer;
    opacity: 0.6;
  }
  .icon-btn:hover { opacity: 1; border-color: var(--rule); }
  .icon-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; opacity: 1; }
  .icon-btn.info { font-family: "IBM Plex Serif", "Georgia", serif; font-style: italic; }

  .meta {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.72rem;
    margin-bottom: 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }
  .meta-row {
    opacity: 0.6;
  }
  .meta-row.dim { opacity: 0.4; }
  .meta .dot { margin: 0 0.3em; }
  .meta-block {
    border-top: 1px solid var(--rule);
    padding-top: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .meta-block-title {
    font-family: "IBM Plex Sans", sans-serif;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.65rem;
    opacity: 0.5;
    margin-bottom: 0.15rem;
  }
  .meta-explain {
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.78rem;
    line-height: 1.5;
    opacity: 0.7;
    margin: 0.25rem 0 0;
  }

  .busy {
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.85rem;
    opacity: 0.6;
    margin: 0;
  }
  .error {
    background: var(--error-bg);
    color: var(--error-ink);
    padding: 0.5rem 0.7rem;
    border-radius: 4px;
    font-size: 0.85rem;
    margin: 0;
  }

  .paraphrase-text {
    font-family: "Iowan Old Style", "Charter", "Georgia", serif;
    font-size: 1.02rem;
    line-height: 1.55;
    margin: 0 0 0.65rem;
    transition: opacity 0.15s;
  }
  .paraphrase-text.dim { opacity: 0.4; }

  .chip {
    display: block;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.72rem;
    padding: 0.3rem 0.6rem;
    border-radius: 4px;
    background: var(--surface-fill);
    color: var(--ink);
    opacity: 0.85;
    margin-bottom: 0.6rem;
    overflow: hidden;
  }
  .chip.warn {
    background: var(--warn-bg);
    color: var(--warn-ink);
    opacity: 1;
  }
  .chip-row, .chip details > summary {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    flex-wrap: wrap;
    min-width: 0;
  }
  .chip .glyph { font-size: 0.85rem; }
  .chip .chip-label { font-weight: 600; }
  .chip .chip-meta { white-space: nowrap; opacity: 0.75; }
  .chip .chip-meta-sep::before { content: "· "; opacity: 0.5; }
  .chip details > summary {
    cursor: pointer;
    list-style: none;
  }
  .chip details > summary::-webkit-details-marker { display: none; }
  .chip details > summary::before {
    content: "▸";
    display: inline-block;
    transition: transform 0.12s;
    font-size: 0.7rem;
    opacity: 0.5;
    margin-right: 0.1rem;
  }
  .chip details[open] > summary::before { transform: rotate(90deg); }
  .chip details ul {
    padding-left: 1.4rem;
    margin: 0.4rem 0 0;
    border-top: 1px solid currentColor;
    padding-top: 0.4rem;
    opacity: 0.6;
  }
  .chip.warn details ul { opacity: 0.85; border-top-color: var(--warn-ink); }
  .chip details li { line-height: 1.5; }

  .glossary-disclosure {
    font-size: 0.85rem;
  }
  .glossary-disclosure summary {
    cursor: pointer;
    opacity: 0.65;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .glossary { margin: 0.5rem 0 0; }
  .term { margin-bottom: 0.55rem; }
  .term dt {
    display: flex;
    gap: 0.45rem;
    align-items: baseline;
    font-size: 0.88rem;
  }
  .term .term-substrate {
    font-style: italic;
    opacity: 0.6;
    font-size: 0.82rem;
  }
  .term dd {
    margin: 0.1rem 0 0;
    font-size: 0.85rem;
    line-height: 1.5;
    opacity: 0.9;
  }
</style>
