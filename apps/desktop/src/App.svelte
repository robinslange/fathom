<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Tier = "simple" | "standard" | "scholarly";
  type Mode = "auto" | "curated" | "jit" | "no-substrate";

  type GlossaryEntry = {
    term: string;
    gloss: string;
    substrate_term?: string | null;
  };
  type FathomResult = {
    paraphrase: string;
    glossary: GlossaryEntry[];
    tier: Tier;
    resolution: "curated" | "jit" | "no-substrate";
    model: string;
    identified_terms: string[];
  };

  let text = $state("");
  let tier: Tier = $state("standard");
  let mode: Mode = $state("auto");
  let result: FathomResult | null = $state(null);
  let error: string | null = $state(null);
  let loading = $state(false);

  async function run() {
    error = null;
    result = null;
    loading = true;
    try {
      result = await invoke<FathomResult>("paraphrase", {
        args: { text, tier, mode },
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }
</script>

<main>
  <header>
    <h1>Fathom</h1>
    <p>Read philosophy at your depth without losing the words.</p>
  </header>

  <section class="input">
    <textarea
      bind:value={text}
      placeholder="Paste a philosophy passage..."
      rows="8"
    ></textarea>

    <div class="controls">
      <fieldset class="tier">
        <legend>Tier</legend>
        {#each ["simple", "standard", "scholarly"] as t}
          <label>
            <input type="radio" name="tier" value={t} bind:group={tier} />
            {t}
          </label>
        {/each}
      </fieldset>

      <fieldset class="mode">
        <legend>Mode</legend>
        {#each ["auto", "curated", "jit", "no-substrate"] as m}
          <label>
            <input type="radio" name="mode" value={m} bind:group={mode} />
            {m}
          </label>
        {/each}
      </fieldset>

      <button onclick={run} disabled={loading || !text.trim()}>
        {loading ? "fathoming..." : "Fathom"}
      </button>
    </div>
  </section>

  {#if error}
    <section class="error">
      <p>{error}</p>
    </section>
  {/if}

  {#if result}
    <section class="output">
      <div class="meta">
        resolution: {result.resolution} · tier: {result.tier} · model: {result.model}
      </div>
      <h2>Paraphrase</h2>
      <p class="paraphrase">{result.paraphrase}</p>

      {#if result.glossary.length > 0}
        <h2>Glossary</h2>
        <ul class="glossary">
          {#each result.glossary as entry}
            <li>
              <strong>{entry.term}</strong>
              {#if entry.substrate_term}
                <code>{entry.substrate_term}</code>
              {/if}
              <span>{entry.gloss}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</main>

<style>
  main {
    max-width: 880px;
    margin: 0 auto;
    padding: 2rem 1.5rem 4rem;
  }
  header h1 {
    margin: 0 0 0.25rem;
    font-size: 2rem;
    font-weight: 600;
  }
  header p {
    margin: 0 0 1.5rem;
    color: #555;
  }
  textarea {
    width: 100%;
    padding: 0.75rem;
    border: 1px solid #ccc;
    border-radius: 6px;
    resize: vertical;
    font-size: 0.95rem;
  }
  .controls {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 0.75rem;
    flex-wrap: wrap;
  }
  fieldset {
    border: 1px solid #ddd;
    border-radius: 6px;
    padding: 0.4rem 0.75rem;
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  legend {
    padding: 0 0.3rem;
    color: #666;
    font-size: 0.85rem;
  }
  label {
    font-size: 0.9rem;
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }
  button {
    padding: 0.55rem 1.1rem;
    border: none;
    border-radius: 6px;
    background: #1a1a1a;
    color: white;
    font-weight: 500;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .error {
    margin-top: 1.5rem;
    padding: 0.75rem 1rem;
    background: #fee;
    border-left: 3px solid #c33;
    color: #800;
  }
  .output {
    margin-top: 2rem;
  }
  .meta {
    font-size: 0.85rem;
    color: #888;
    margin-bottom: 0.5rem;
  }
  h2 {
    font-size: 1.1rem;
    margin: 1.2rem 0 0.5rem;
  }
  .paraphrase {
    white-space: pre-wrap;
    line-height: 1.6;
  }
  .glossary {
    list-style: none;
    padding: 0;
  }
  .glossary li {
    padding: 0.4rem 0;
    border-bottom: 1px solid #eee;
  }
  .glossary code {
    font-family: ui-monospace, monospace;
    background: #f3f0e8;
    padding: 0.05rem 0.3rem;
    border-radius: 3px;
    margin: 0 0.3rem;
    font-size: 0.85em;
  }
</style>
