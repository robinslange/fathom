# Fathom

Read philosophy at your depth without losing the words.

Fathom is a local-first reader for a 549-book public-domain philosophy library. Highlight any sentence and get a paraphrase at your chosen depth (Simple, Standard, Scholarly) with the technical vocabulary preserved and glossed — `Dasein`, `eph' hēmin`, `eudaimonia`, `ergon` are not flattened. Every paraphrase carries a faithfulness verdict from a sentence-level NLI judge so drift is visible at a glance. All inference runs on your machine.

## Quick start (desktop app)

Download `Fathom-macos.zip` from the [latest rolling build](https://github.com/robinslange/fathom/releases/tag/latest) (rebuilt on every push to `main`) and drag `Fathom.app` to `/Applications/`. The app is ad-hoc signed; right-click → Open the first time.

On launch the app pulls a signed manifest of the corpus from `corpus.fathom.omit.nz`, then downloads three models into your OS app-data directory:

- **bge-small-en-v1.5 (ONNX)** — ~130MB, semantic search across the loaded library
- **Gemma 3 4B IT (Q4_K_M GGUF)** — ~2.5GB, paraphrase + JIT term identification, runs via the bundled `llama.cpp` with Metal acceleration
- **DeBERTa-v3-base MNLI (quantized ONNX)** — ~244MB, the live faithfulness judge

The reader is paginated (8 paragraphs per page, paginate with the ‹ › buttons, arrow keys, or spacebar; Shift+Space steps back). Highlight any selection — a single sentence or a multi-paragraph span — and the right pane streams a paraphrase, a glossary of load-bearing terms, and a faithfulness summary. The search bar runs semantic kNN across the books currently loaded into memory; click a hit to jump to the page containing the cited paragraph.

## Quick start (CLI)

```bash
git clone https://github.com/robinslange/fathom.git
cd fathom
cargo build --release -p fathom-cli

# Download the bundled Gemma model (one-time, ~2.5GB)
./target/release/fathom bootstrap --model gemma3-4b

# Paraphrase a passage (mode=auto tries curated → JIT → no-substrate)
echo "Of things some are in our power, and others are not." | \
  ./target/release/fathom paraphrase --backend llama-cpp --tier standard -

# Score a paraphrase against the original
./target/release/fathom judge \
  --original original.txt --paraphrase out.txt --json

# Show which traditions the in-binary lexicon covers
./target/release/fathom lexicon
```

The CLI also supports an Ollama backend (`--backend ollama --model gemma3:4b`) if you'd rather use a local Ollama server.

## How it works

The desktop app is a library + reader on top of a per-selection paraphrase pipeline.

**Library runtime.** A signed `index.msgpack` manifest at `corpus.fathom.omit.nz` lists 549 books (Project Gutenberg public-domain philosophy, NZ life+50 cleared). The runtime verifies the manifest signature against an in-binary minisign public key, then fetches per-book shards on demand. Each shard is msgpack + zstd, SHA-256 verified at load, and holds the book's canonical text plus per-chunk bge-small embeddings (384-dim, packed as f16). Shards stay cached locally for offline reading.

**Search.** Query text is embedded with bge-small (CPU, deterministic) and ranked by cosine similarity against chunk embeddings in the LRU shard cache. On launch the first 64 books are prewarmed in parallel so cold-cache search returns hits immediately; subsequent shards load as you open books.

**Paraphrase.** When you highlight a selection in the reader, the endpoints are translated from DOM offsets to document-absolute UTF-8 byte positions, snapped to UAX#29 sentence boundaries, then handed to `fathom_with_judge` in `Mode::Auto`. The mode tries three substrate paths in order:

1. **Curated.** 135 seed passages with verified terms-of-art ship in the binary as YAML. Matched by fingerprint; rare hit on arbitrary Gutenberg prose, but highest fidelity when it does.
2. **JIT.** Two-pass identification — Gemma asks itself which English phrases are doing technical philosophical work, then glosses each under an explicit anti-fabrication guard ("omit the substrate rather than guess"). This is the path most library selections take.
3. **No-substrate.** Fallback: the model is given the passage alone and asked to preserve and gloss terms at its own discretion.

**Faithfulness judge.** After the paraphrase lands, `DeBERTa-v3-base-mnli-fever-anli` (Xenova quantized ONNX) runs sentence-level entailment between the original and the paraphrase. Three channels surface in the UI: mean entailment support, worst-case contradiction, and the list of paraphrase sentences whose best alignment is below the entailment threshold (candidate "introductions"). The verdict turns the panel border amber when the model drifted.

## Repository layout

```
crates/
  fathom-core/    library runtime (manifest fetch, shard cache, kNN search,
                  sentence-snap), orchestration, NLI judge, lexicon loader,
                  prompts, model bootstrap
  fathom-engine/  Backend trait: bundled llama.cpp via llama-cpp-2, Ollama HTTP
  fathom-cli/     command-line interface (paraphrase, judge, bootstrap, lexicon)
  fathom-embed/   bge-small ONNX wrapper, deterministic CPU-only,
                  pack-as-f16 for shard embeddings
  fathom-chunker/ paragraph + UAX#29 sentence splitting, shared between build
                  and runtime
  fathom-build/   operator-only corpus build pipeline (catalog → filter →
                  chunk → embed → shard → sign → deploy → harvest-substrate)
  fathom-bench/   retrieval benchmark harness
apps/
  desktop/        Tauri 2 desktop app (Svelte 5 + Vite frontend)
lexicon/          curated YAML files — 135 seed passages, 12 traditions
```

## Lexicon contributions

Each YAML file in `lexicon/` covers one source text. Schema:

```yaml
source:
  title: "Enchiridion"
  author: "Epictetus"
  translation: "George Long (1890), public domain"
  language: "Greek"
  tradition: "Stoic"

passages:
  - id: "enchiridion-1"
    fingerprint: "Of things some are in our power, and others are not"
    themes: ["freedom-and-fate", "action-and-impulse"]
    terms:
      "in our power":
        substrate: "eph' hēmin"
        gloss: "What is genuinely up to us; the only proper domain of moral concern"
```

**Anti-fabrication rule:** every `substrate` field must be verifiable against a standard scholarly edition. Speculative or reconstructed substrate terms are grounds for rejection. If a term cannot be sourced, leave it out.

To contribute:

1. Fork the repository.
2. Add a new YAML file for your text, or extend an existing one. Use a public-domain translation.
3. Tag each passage with 1–3 themes from the canonical theme list (run `./target/release/fathom lexicon` to see coverage).
4. Open a PR with a source reference (edition, translator, page/line number) for every substrate term you add.

## Development

### Prerequisites

- Rust 1.90+
- `cmake` (e.g. `brew install cmake`)
- Xcode Command Line Tools (`xcode-select --install`)
- Node 20+
- pnpm (`npm install -g pnpm` or via [pnpm.io](https://pnpm.io/installation))

```bash
# Rust workspace
cargo build --workspace
cargo test --workspace      # 63 tests across runtime, chunker, engine, etc.

# Desktop app dev mode (Vite hot reload + Tauri)
cd apps/desktop
pnpm install
pnpm tauri dev

# Desktop app vitest unit tests (selection-to-byte-offset, pagination math)
pnpm test                   # 26 tests

# Desktop app release build (produces target/release/bundle/macos/Fathom.app)
pnpm tauri build -- --bundles app
```

The workspace pins `CMAKE_OSX_DEPLOYMENT_TARGET=12.0` in `.cargo/config.toml`. This is required because `cmake-rs` doesn't propagate `MACOSX_DEPLOYMENT_TARGET` to the bundled C++ compilation, and `llama.cpp` uses `std::filesystem::path` which is gated behind macOS 10.15+.

## License

Apache-2.0. See `LICENSE`.

## Status

v0.2. Library-first reader shipped: 549-book signed corpus, semantic search, paginated reader, highlight-to-paraphrase with JIT term identification and live NLI faithfulness judge. All inference local-first on Apple Silicon. Open lines: dynamic viewport-fit pagination, dark-mode theming pass, semantic substrate-term retrieval for v0.21 (rank the lexicon against the selection embedding so the JIT path gets the right substrate without dumping all of it).
