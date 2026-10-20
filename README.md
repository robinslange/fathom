# Fathom

A local-first reader for classical philosophy. Highlight a passage, get it in plain English without the technical vocabulary getting flattened.

Built and maintained by [omit](https://omit.nz).

## What it does

You open a public-domain philosophy book (say, Epictetus's *Enchiridion* or Hume's *Enquiry*), browse a 283-book corpus by theme ("Who am I really?", "How should I live?", "Suffering and loss"), highlight any sentence or paragraph in the reader, and Fathom returns it paraphrased at one of three depths (Simple, Standard, Scholarly). The technical vocabulary the author is actually using (`eph' hēmin`, `Dasein`, `eudaimonia`) is preserved and glossed alongside the paraphrase, not silently replaced with a colloquial gloss. Every paraphrase carries a faithfulness verdict from a sentence-level NLI judge so you can see at a glance where the model drifted.

Everything runs on your machine after a one-time ~2.7GB model download. No telemetry, no cloud round-trips, no account.

## Who it's for

- **Readers new to philosophy.** The Simple tier gets you reading the actual primary sources at fifteen-year-old reading level instead of bouncing off Kant in week one.
- **Students.** The Standard tier matches an undergraduate first-philosophy-course register; the glossary preserves the technical vocabulary you actually need to learn.
- **Scholars.** The Scholarly tier preserves register and source-language terms without flattening, and the faithfulness judge makes drift visible at a glance. The CLI exposes the same pipeline for batch work.

## Install

Download `Fathom-macos.zip` from the [latest rolling build](https://github.com/robinslange/fathom/releases/tag/latest) (rebuilt on every push to `main`; dated `v<YYYY>.<MM>.<DD>.<N>` tags with changelogs pin versions on the same page), unzip, and drag `Fathom.app` to `/Applications/`. The app is ad-hoc signed; right-click → Open the first time.

On first launch an onboarding screen shows the three model downloads with live progress, and you land in the library once everything is on disk. The status dot in the header shows what's loaded and what's still fetching; click it for details and retry buttons.

## How it works

**Library.** A signed manifest at `corpus.fathom.omit.nz` lists 283 books: Project Gutenberg public-domain philosophy, cleared for New Zealand copyright, primary sources only. The app verifies the manifest signature, fetches each book on demand, checks its hash, and caches it locally for offline reading.

**Search.** Queries run against the books in the local cache with dense embeddings (bge-small) and BM25 keyword scoring, fused by reciprocal rank. On launch the first 64 books are prewarmed so search returns hits immediately.

**Paraphrase.** A highlighted selection is snapped to sentence boundaries and paraphrased by Gemma. Fathom finds the passage's technical terms three ways, in order:

1. **Curated.** 135 seed passages in `lexicon/` with verified source-language terms ship in the binary. Highest fidelity, rarely hit on arbitrary prose.
2. **Identified.** The model first lists which English phrases are doing technical philosophical work, then glosses each, told to leave a term out rather than guess. Most library selections take this path.
3. **Unassisted.** The model gets the passage alone and preserves terms at its own discretion.

**Faithfulness judge.** DeBERTa runs sentence-level entailment between the original and the paraphrase. The UI shows mean support, worst-case contradiction, and any paraphrase sentence the original doesn't support, and turns the faithfulness indicator amber when the model drifted.

## Models

Three models download into your OS app-data directory on first launch (~2.7GB total):

| model | size | job |
|---|---|---|
| bge-small-en-v1.5 (ONNX) | ~130MB | semantic search |
| DeBERTa-v3-base MNLI (quantized ONNX) | ~244MB | faithfulness judge |
| Gemma 3 4B IT (Q4_K_M GGUF) | ~2.5GB | paraphrase and term identification, via bundled `llama.cpp` with Metal |

## CLI

The same pipeline is exposed as a CLI for batch work. `fathom --help` lists every command.

```bash
git clone https://github.com/robinslange/fathom.git
cd fathom
cargo build --release -p fathom-cli

# Download the bundled Gemma model (one-time, ~2.5GB)
./target/release/fathom bootstrap --model gemma3-4b

echo "Of things some are in our power, and others are not." | \
  ./target/release/fathom paraphrase --backend llama-cpp --tier standard -

./target/release/fathom judge --original original.txt --paraphrase out.txt --json
```

`--backend ollama --model gemma3:4b` uses a local Ollama server instead.

## Repository layout

| path | what |
|---|---|
| `crates/fathom-core` | library runtime, search, orchestration, NLI judge, lexicon loader, model bootstrap |
| `crates/fathom-engine` | model backends: bundled llama.cpp, Ollama |
| `crates/fathom-cli` | command-line interface |
| `crates/fathom-embed` | bge-small ONNX wrapper, shared by build and runtime |
| `crates/fathom-chunker` | paragraph and sentence splitting, shared by build and runtime |
| `crates/fathom-build` | operator-only corpus build pipeline |
| `crates/fathom-bench` | retrieval benchmark harness |
| `apps/desktop` | Tauri 2 desktop app (Svelte 5 + Vite) |
| `lexicon/` | curated passages and their source-language terms |

## Lexicon contributions

Each YAML file in `lexicon/` covers one source text. The schema, citation formats and the failure modes reviewers reject are in [lexicon/README.md](lexicon/README.md). The one rule that matters most: every source-language term must be verifiable against a standard scholarly edition. If a term cannot be sourced, leave it out.

## Development

Prerequisites: Rust 1.90+, `cmake` (`brew install cmake`), Xcode Command Line Tools, Node 22+, pnpm.

```bash
make test                   # cargo test --workspace
make lint                   # clippy, warnings as errors

cd apps/desktop
pnpm install
pnpm tauri dev              # Vite hot reload + Tauri
pnpm test                   # vitest
pnpm tauri build -- --bundles app
```

The workspace pins `CMAKE_OSX_DEPLOYMENT_TARGET=12.0` in `.cargo/config.toml`, because `cmake-rs` doesn't propagate `MACOSX_DEPLOYMENT_TARGET` to the bundled C++ build and `llama.cpp` needs `std::filesystem::path` (macOS 10.15+).

## License

Apache-2.0. See `LICENSE`.
