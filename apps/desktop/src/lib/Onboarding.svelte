<script lang="ts">
  import { onboarding } from "./use-onboarding.svelte.js";

  function bytesToGb(b: number): string {
    return (b / 1_000_000_000).toFixed(1);
  }
</script>

{#if onboarding.shouldShow}
  <div class="backdrop" role="presentation"></div>
  <div class="modal" role="dialog" aria-modal="true" aria-label="Welcome to Fathom">
    <h2>Welcome to Fathom</h2>
    <p>
      Fathom paraphrases classical philosophy into plain English. Hundreds of public-domain books,
      read at your depth. The first launch downloads about 2.7&nbsp;GB of language models — after
      that, everything runs on your machine. Nothing leaves the app.
    </p>

    <div class="checkrow">
      {#if onboarding.catalogueReady}
        <span class="check" aria-hidden="true">✓</span>
        <span>Library catalogue ready</span>
      {:else}
        <span class="spinner" aria-hidden="true"></span>
        <span>Fetching catalogue…</span>
      {/if}
    </div>

    <div class="bar-row">
      <div class="bar-meta">
        <span>Language models</span>
        <span class="bytes">
          {bytesToGb(onboarding.modelsBytes.bytes)} / {bytesToGb(onboarding.modelsBytes.total || 2_700_000_000)}&nbsp;GB
        </span>
      </div>
      <div class="bar"><div class="bar-fill" style:width="{onboarding.modelsPercent}%"></div></div>
    </div>

    {#if onboarding.completeError}
      <p class="error">Couldn't save: {onboarding.completeError}</p>
    {/if}

    <div class="actions">
      <button
        type="button"
        disabled={!onboarding.canDismiss || onboarding.dismissing}
        onclick={() => onboarding.complete()}
      >
        Get started
      </button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.35);
    z-index: 99;
  }
  .modal {
    position: fixed;
    left: 50%; top: 50%;
    transform: translate(-50%, -50%);
    width: min(540px, 92vw);
    background: var(--panel, #faf7f0);
    color: var(--ink, #2b2522);
    border: 1px solid var(--rule-strong);
    border-radius: 6px;
    padding: 1.5rem 1.5rem 1.25rem;
    z-index: 100;
    box-shadow: 0 12px 40px rgba(0,0,0,0.18);
  }
  .modal h2 {
    margin: 0 0 0.7rem;
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 1.05rem;
    letter-spacing: 0.02em;
    font-weight: 600;
  }
  .modal p {
    margin: 0 0 1rem;
    font-size: 0.92rem;
    line-height: 1.55;
  }
  .checkrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0;
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.88rem;
    border-top: 1px solid var(--rule);
    margin-top: 0.5rem;
  }
  .check { color: var(--ok-ink, #6a8f5e); font-weight: 600; }
  .spinner {
    width: 0.8rem; height: 0.8rem;
    border: 2px solid var(--rule-strong);
    border-top-color: var(--accent, #b07a3a);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    display: inline-block;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
  }
  .bar-row { margin-top: 0.9rem; }
  .bar-meta {
    display: flex;
    justify-content: space-between;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.76rem;
    opacity: 0.7;
    margin-bottom: 0.3rem;
  }
  .bytes { font-variant-numeric: tabular-nums; }
  .bar {
    height: 4px;
    background: var(--rule);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    background: var(--accent, #b07a3a);
    transition: width 200ms ease-out;
  }
  .error {
    margin-top: 0.8rem;
    margin-bottom: 0;
    color: var(--error-ink, #b8484e);
    font-size: 0.82rem;
    font-family: "IBM Plex Mono", monospace;
  }
  .actions {
    margin-top: 1.5rem;
    display: flex;
    justify-content: flex-end;
  }
  .actions button {
    padding: 0.5rem 1.1rem;
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.88rem;
    background: var(--panel);
    color: inherit;
    border: 1px solid var(--rule-strong);
    border-radius: 4px;
    cursor: pointer;
  }
  .actions button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .actions button:not(:disabled):hover {
    background: var(--accent-wash-hover);
  }
  .actions button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
