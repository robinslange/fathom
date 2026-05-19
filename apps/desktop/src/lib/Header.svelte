<script lang="ts">
  import { onMount } from "svelte";
  import {
    applyTheme,
    nextPreference,
    preferenceGlyph,
    preferenceLabel,
    readStoredPreference,
    storePreference,
    watchSystemTheme,
    type ThemePreference,
  } from "./theme.js";
  import { library } from "./use-library.svelte.js";
  import { search } from "./use-search.svelte.js";
  import StatusDot from "./StatusDot.svelte";

  let themePref: ThemePreference = $state(readStoredPreference());

  $effect(() => {
    applyTheme(themePref);
  });

  onMount(() => {
    watchSystemTheme(
      () => themePref,
      () => applyTheme(themePref),
    );
  });

  function cycleTheme() {
    themePref = nextPreference(themePref);
    storePreference(themePref);
  }
</script>

<header class="app-header">
  <div class="brand">
    <h1>Fathom</h1>
    <p>Read philosophy at your depth without losing the words.</p>
  </div>
  <StatusDot />
  <div class="search">
    <input
      type="search"
      bind:value={search.query}
      placeholder="Search across all books"
      aria-label="Search the library"
      disabled={!library.embedderReady}
    />
    {#if search.searching}
      <span class="searching">searching…</span>
    {/if}
  </div>
  <button
    class="theme-toggle"
    type="button"
    onclick={cycleTheme}
    title="Theme: {preferenceLabel(themePref)} (click to cycle)"
    aria-label="Theme: {preferenceLabel(themePref)}, click to cycle"
  >
    <span aria-hidden="true">{preferenceGlyph(themePref)}</span>
    <span class="theme-toggle-label">{preferenceLabel(themePref)}</span>
  </button>
</header>

<style>
  .app-header {
    display: flex;
    align-items: center;
    gap: 2rem;
    padding: 0.75rem 1.25rem;
    border-bottom: 1px solid var(--rule);
    background: var(--panel);
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
    background: var(--panel);
    color: var(--ink);
    border: 1px solid var(--rule-strong);
    border-radius: 4px;
  }
  .search input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
    border-color: transparent;
  }
  .search input:disabled {
    background: var(--surface-mute);
    color: var(--ink-fade);
    cursor: progress;
  }
  .searching {
    font-size: 0.8rem;
    opacity: 0.6;
    font-family: "IBM Plex Mono", monospace;
  }
  .theme-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    background: transparent;
    color: inherit;
    border: 1px solid var(--rule-strong);
    padding: 0.3rem 0.6rem;
    border-radius: 4px;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.78rem;
    cursor: pointer;
  }
  .theme-toggle:hover {
    background: var(--accent-wash-hover);
  }
  .theme-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .theme-toggle-label {
    letter-spacing: 0.04em;
  }
</style>
