---
name: classify-traditions
description: Use this when Robin asks to classify Gutenberg books in the Fathom corpus into philosophical traditions, run a tradition-tagging batch, or update traditions.json. Operator-driven workflow — dispatches Sonnet 4.6 subagents in parallel for candidate classification, then Opus reviews the batch in main thread.
---

# Classify traditions for Fathom's Gutenberg corpus

This skill tags books in `dist/index.msgpack` with their philosophical tradition
(Stoic, Daoist, Platonic, Aristotelian, Buddhist, Cartesian, Vedantic,
Existentialist, etc.) so the runtime can surface tradition-filtered shelves.

The skill is **operator-driven**, not CI. Tradition tagging is a judgment task
that benefits from Robin's adjudication on edge cases. Run it weekly until the
backlog is cleared, then on demand as new translations enter NZ public domain.

## Invocation shape

User says: "classify the next batch of books" / "tag traditions for the corpus"
/ "update traditions.json with another N books".

## Inputs you'll need before starting

1. **`/Users/robin/dev/fathom/dist/index.msgpack`** — current manifest with all
   552 books. Decode via `msgpack` Python module or `rmp-serde` Rust to read.
2. **`/Users/robin/dev/fathom/traditions.json`** — existing classifications.
   Format: `[{ "gutenberg_id": 1497, "tradition": "Platonic", "confidence": "high", "reviewer": "robin", "reviewed_at": "2026-05-16" }]`.
   Skip books already in this file unless the user explicitly asks to re-tag.
3. **`/Users/robin/dev/fathom/dist/shards/{id}.shard`** — per-book msgpack-zstd
   containing the first-page text needed for grounding.

## The existing tradition vocabulary

Drawn from `lexicon/*.yaml` source field — these are the 12 traditions the v0.1
library already uses. Prefer these; introduce new ones only if a book genuinely
belongs to none of them and the user agrees.

```
Aristotelian
Cartesian (Rationalist)
Confucian
Daoist
Existentialist          (incl. "Existentialism (proto)" for Kierkegaard)
German Idealism         (Hegel, Schopenhauer's category)
Mahāyāna / Prajñāpāramitā
Nietzschean
Platonic
Schopenhauerian
Spinozist (Rationalist)
Stoic
Theravāda
Vedantic                (incl. "Vedantic / Vaishnava")
```

Books that don't fit cleanly: tag as `Uncategorised` and surface for Robin.

## Workflow

### Step 1: Read the current state

Read `dist/index.msgpack` to get the list of books. Read `traditions.json` to
get classifications-so-far. Compute the unclassified set: books in the manifest
not yet in traditions.json.

If the user gave a batch size (e.g. "next 50"), take the first N unclassified
books. If not, ask: "How many books in this batch? (Recommended: 30-50 per
session.)"

### Step 2: Extract first-page text for grounding

For each book in the batch, decode its shard at `dist/shards/{id}.shard`
(zstd-decompress, rmp-serde decode) and pull the first ~500 chars of
`canonical_text`. This is the grounding text the Sonnet subagent will reason
against.

Use the Bash tool with a short Python script to decode shards:

```bash
python3 -c "
import zstandard, msgpack, sys
with open(sys.argv[1], 'rb') as f:
    data = zstandard.ZstdDecompressor().decompress(f.read())
shard = msgpack.unpackb(data, raw=False)
print(shard['canonical_text'][:500])
"
```

(If `python3-zstandard` / `python3-msgpack` aren't installed, install them or
shell out to a small Rust helper.)

### Step 3: Dispatch parallel Sonnet subagents

Spawn one general-purpose subagent per ~5-10 books in the batch (so 30-50 books
= 3-10 parallel subagents). Each subagent receives:

- The 12-tradition vocabulary above
- Its slice of books: `[{ gutenberg_id, title, translators, locc, first_page }]`
- Instructions: classify each book into one of the 12 traditions OR
  `Uncategorised`. Return JSON: `{ id, tradition, confidence: high|medium|low,
  reasoning: "one sentence" }`.

Subagent prompt template:

```
You are tagging philosophical books with their tradition. Use exactly one of
these traditions per book (or "Uncategorised" if none fits):

[the 12-tradition list]

For each book below, return a JSON object with: gutenberg_id, tradition,
confidence (high/medium/low), reasoning (one sentence). Output as a JSON array.

Books:
[the slice]

Confidence rubric:
- high: title + author + first-page text all clearly indicate one tradition
- medium: clear from title + author but first-page text is generic
- low: ambiguous; might be borderline (e.g. Augustine = Platonic? Christian?
  Aristotelian?). Tag with best guess and flag for review.

Strict JSON output. No prose around it.
```

Use the model:sonnet hint when invoking the subagent if available.

### Step 4: Opus review in main thread

Once subagents return, in main thread:

1. **Surface low-confidence items** to Robin. Show: title, author, subagent's
   tag + reasoning. Ask Robin to confirm or correct each.
2. **Surface contradictions** with existing tags — if a Plato dialogue already
   in traditions.json as "Platonic" and the new batch tags another Plato
   dialogue differently, flag it.
3. **Spot-check 5 random high-confidence items** against title + author. Look
   for obvious mis-tags before commit.
4. **Look for novel traditions** the subagents introduced (anything outside the
   12-list). Ask Robin: introduce, or remap to existing?

### Step 5: Append to traditions.json and commit

Append the approved tags to `traditions.json` with:
- `reviewer: "robin"`
- `reviewed_at: <YYYY-MM-DD>` (today)
- `confidence: <as decided>`

Sort by gutenberg_id for stable diffs.

Then offer to commit:
```
git -C /Users/robin/dev/fathom add traditions.json
git -C /Users/robin/dev/fathom commit -m "traditions: classify N books (batch of <date>)"
```

### Step 6: Report

Tell Robin:
- N books classified this session
- M low-confidence flagged for his review (already adjudicated)
- K total classified so far / 552 in corpus
- Suggest: next batch in a week, or run a `manifest` rebuild now to ship the
  new tags.

## Edge cases worth knowing

- **W.D. Ross trap on Aristotle volumes**: Ross edited Oxford Aristotle 1908–
  1931. Tag the work as "Aristotelian" — translator copyright is upstream of
  the tradition tag.
- **Augustine**: clearly Platonist in lineage (Neoplatonic) but also Christian
  theologian. Default: "Platonic" with low confidence + reasoning that flags
  the Christian theology overlay. Robin adjudicates.
- **Aquinas**: Aristotelian in method, Christian in content. Same shape:
  "Aristotelian" with confidence flag.
- **Vedic texts**: distinguish Upanishads (Vedantic) from Yoga Sutras
  (technically Yogic but often classed Vedantic). "Vedantic" covers both.
- **Pali Canon**: "Theravāda" not "Buddhist" — we have separate Mahāyāna +
  Theravāda buckets.
- **Self-help adjacent to philosophy**: Smiles' "Self Help", Bennett's "How to
  Live on 24 Hours a Day". Don't bend the tradition vocabulary — tag
  "Uncategorised" and let the LCC subject filter handle them.

## Why this lives in the repo

The skill is invocable from any Claude Code session checked out in the fathom
repo. It documents the classification workflow in one place — no drift between
what Robin does and what a future contributor or future-Robin reads here.

`traditions.json` lives in the repo (not in dist/) because it's source data,
not artefact data. The `manifest` build stage reads it as input; the manifest
itself is regenerated. So weekly batch → traditions.json commit → next
`manifest` rebuild ships the new tags to users.
