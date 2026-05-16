# Fathom

Fathom paraphrases classical philosophy passages into plain English while keeping every term-of-art exactly as it appears in the source text. The design principle is fidelity over fluency: `Dasein`, `eph' hēmin`, `eudaimonia`, and `ergon` are not flattened into approximations — they are preserved and explained. Each run produces a paraphrase and a glossary of load-bearing terms with one-line philosophical explanations and substrate-language citations where they can be verified.

## Quick start

```bash
pip install fathom
# or: uv add fathom

# From a file
fathom paraphrase epictetus.txt --source "Epictetus, Enchiridion §1"

# From stdin
echo "Of things some are in our power..." | fathom paraphrase -

# Choose a different model
fathom paraphrase epictetus.txt --model qwen3:4b

# Force JIT identification (skip curated lookup)
fathom paraphrase epictetus.txt --mode jit
```

Fathom requires an Ollama server running locally (default: `http://localhost:11434`). Install Ollama from https://ollama.ai and pull a model:

```bash
ollama pull gemma3:4b
```

## How it works

Fathom uses a three-tier substrate resolution strategy, tried in order:

**Tier 1 — Canonical-text lookup.** The package ships with curated lexicon files (`src/fathom/lexicon/*.yaml`) mapping known passages to verified term substrates. When a passage fingerprints against a known entry, those substrates are injected directly into the prompt. This is the highest-fidelity path.

**Tier 2 — JIT identification.** For passages not in the lexicon, a two-pass pipeline runs: pass 1 asks the model to identify which English phrases are doing technical philosophical work; pass 2 glosses those terms with an explicit anti-fabrication guard ("omit Greek rather than guess"). This catches load-bearing terms the curated lexicon does not cover.

**Tier 3 — No-substrate baseline.** If JIT is disabled or as a fallback, the model is given the passage alone and asked to preserve and gloss terms-of-art at its own discretion. Substrate citations are included only where the model is confident.

## Configuration

| Option | Default | Description |
|---|---|---|
| `--model` | `gemma3:4b` | Any model available in your Ollama instance |
| `--depth` | `3` | Gloss depth 1-5: higher values surface more terms |
| `--mode` | `auto` | `auto` (tier 1 → 2 → 3), `jit` (tier 2 only), `curated` (tier 1 only, errors if no match) |
| `--base-url` | `http://localhost:11434` | Ollama server URL |

Tested models: `gemma3:4b`, `qwen3:4b`, `phi4-mini`.

## Contributing the lexicon

Lexicon entries live in `src/fathom/lexicon/*.yaml`. One file per source text. See `src/fathom/lexicon/README.md` for the schema.

**Anti-fabrication rule**: every `substrate` field must be verifiable against a standard scholarly edition. Speculative or reconstructed substrate terms are grounds for rejection. If a term cannot be sourced, leave it out.

To contribute:
1. Fork the repository.
2. Add a new YAML file for your text, or extend an existing one.
3. Open a pull request with a source reference (edition, translator, page/line number) for each substrate term you add.

## License

Apache 2.0. See `LICENSE`.
