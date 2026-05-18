<script lang="ts">
  import { onMount } from "svelte";
  import Header from "./lib/Header.svelte";
  import LeftRail from "./lib/LeftRail.svelte";
  import Reader from "./lib/Reader.svelte";
  import ParaphrasePane from "./lib/ParaphrasePane.svelte";
  import { library } from "./lib/use-library.svelte.js";
  import { search } from "./lib/use-search.svelte.js";
  import { paraphrase } from "./lib/use-paraphrase.svelte.js";

  onMount(() => {
    library.initEffects();
    search.initEffects();
    paraphrase.initEffects();
    library.init();
  });
</script>

<Header />

<main class="library">
  <LeftRail />
  <Reader />
  <ParaphrasePane />
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: "Iowan Old Style", "Charter", "Georgia", serif;
    background: var(--paper);
    color: var(--ink);
  }
  :global(*) {
    box-sizing: border-box;
  }

  .library {
    display: grid;
    grid-template-columns: 22rem 1fr 24rem;
    height: calc(100vh - 4rem);
    overflow: hidden;
  }
</style>
