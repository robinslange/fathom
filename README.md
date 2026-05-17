# Fathom

Read philosophy at your depth without losing the words.

Fathom paraphrases classical philosophy into plain English while keeping every term-of-art exactly as it appears in the source. The design principle is fidelity over fluency: `Dasein`, `eph' hēmin`, `eudaimonia`, and `ergon` are not flattened into approximations — they are preserved and explained. Each run produces a paraphrase, a glossary of load-bearing terms with one-line philosophical explanations, and a faithfulness score that flags any sentence the model couldn't anchor to the original.

## Quick start (desktop app)

Download `Fathom-macos.zip` from the [latest rolling build](https://github.com/robinslange/fathom/releases/tag/latest) (rebuilt on every push to `main`) and drag `Fathom.app` to `/Applications/`. The app is ad-hoc signed for now, so you may need to right-click → Open the first time.

On first paraphrase the app downloads two models into your OS app-data directory:
- **Gemma 3 4B IT (Q4_K_M GGUF)** — ~2.5GB, runs locally via the bundled `llama.cpp` with Metal acceleration.
- **DeBERTa v3 base MNLI (quantized ONNX)** — ~244MB, runs the live faithfulness judge.

Browse the 135-passage library by tradition (Stoic, Aristotelian, Daoist, etc.) or by theme (freedom and fate, virtue and character, mind and self, …). Pick a passage, choose a depth (Simple / Standard / Scholarly), click **Fathom this passage**. The paraphrase and glossary render below the passage, alongside a faithfulness indicator that turns red if the model drifted.

## Quick start (CLI)

```bash
git clone https://github.com/robinslange/fathom.git
cd fathom
cargo build --release -p fathom-cli

# Download the bundled Gemma model (one-time, ~2.5GB)
./target/release/fathom bootstrap --model gemma3-4b

# Paraphrase a passage
echo "Of things some are in our power, and others are not." | \
  ./target/release/fathom paraphrase \
    --backend llama-cpp --tier simple --mode curated -

# Score a paraphrase against the original
./target/release/fathom judge \
  --original original.txt --paraphrase out.txt --json

# Browse the curated library
./target/release/fathom library themes
./target/release/fathom library list --theme freedom-and-fate
./target/release/fathom library show enchiridion-1
```

The CLI also supports an Ollama backend (`--backend ollama --model gemma3:4b`) if you'd rather use a local Ollama server instead of the bundled `llama.cpp`.

## How it works

Three-tier substrate resolution, tried in order:

1. **Curated.** 135 passages across twelve traditions ship with the binary as YAML files, with every term-of-art verified against a public-domain scholarly edition. When the input matches a known passage by fingerprint, the curated substrate is injected straight into the prompt — the highest-fidelity path.
2. **JIT.** For passages outside the lexicon, a two-pass identification step asks the model which English phrases are doing technical philosophical work, then glosses them under an explicit anti-fabrication guard ("omit the substrate rather than guess").
3. **No-substrate.** Fallback: the model is given the passage alone and asked to preserve and gloss terms at its own discretion.

After paraphrase, the **NLI judge** runs sentence-level entailment between the original and the paraphrase using `DeBERTa-v3-base-mnli-fever-anli` (Xenova quantized ONNX). The score is three channels: mean entailment support, worst-case contradiction, and a list of paraphrase sentences whose best alignment is below the entailment threshold (candidate "introductions"). All three surface in the UI and in the CLI's JSON output.

## Repository layout

```
crates/
  fathom-core/    lexicon, prompts, orchestration, NLI judge, library API, model bootstrap
  fathom-engine/  Backend trait: bundled llama.cpp, Ollama HTTP, future HttpEndpoint
  fathom-cli/     command-line interface
apps/
  desktop/        Tauri 2 desktop app (Svelte 5 frontend, Rust backend)
lexicon/          curated YAML files — 135 passages, 12 traditions
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
3. Tag each passage with 1–3 themes from `crates/fathom-core/src/library.rs::THEMES`.
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
cargo test --workspace

# Desktop app dev mode (Vite hot reload + Tauri)
cd apps/desktop
pnpm install
pnpm tauri dev

# Desktop app release build (produces target/release/bundle/macos/Fathom.app)
pnpm tauri build
```

The workspace pins `CMAKE_OSX_DEPLOYMENT_TARGET=12.0` in `.cargo/config.toml`. This is required because `cmake-rs` doesn't propagate `MACOSX_DEPLOYMENT_TARGET` to the bundled C++ compilation, and `llama.cpp` uses `std::filesystem::path` which is gated behind macOS 10.15+.

## License

Apache-2.0. See `LICENSE`.

## Status

v0.1. Library browse + curated paraphrase + live NLI judge all working end-to-end on Apple Silicon. Next: NLI self-critique loop, threshold calibration, library v0.2 (embedding model + user-added texts + semantic search).
