# Lexicon substrate map format

Each YAML file in this directory defines a curated substrate map for one philosophical text or text group. The lexicon is the codified-expertise layer that makes Fathom not just-another-paraphraser: each substrate term should be checked against a scholarly source before it ships.

## Schema

```yaml
source:
  title: "Enchiridion"
  author: "Epictetus"
  translation: "George Long (1890), public domain"
  language: "Greek"
  tradition: "Stoic"

passages:
  - id: "enchiridion-1"                                   # unique within the file
    fingerprint: "Of things some are in our power"         # leading ~50 chars, unique within the file
    terms:
      "in our power":
        substrate: "eph' hēmin"                            # canonical transliteration
        gloss: "What is genuinely up to us; the only proper domain of moral concern"
        citation: "LSJ s.v. ἐφ' ἡμῖν; Long & Sedley §53"  # optional — see below
```

## Fields

- `source.language`: substrate language (`Greek`, `Latin`, `German`, `French`, `Danish`, `Classical Chinese (pinyin)`, `Sanskrit (IAST)`, `Pali`, etc.)
- `passages[].id`: unique slug
- `passages[].fingerprint`: leading ~50 chars of the source text, unique within the file. Matched by `lookup_canonical` in `crates/fathom-core/src/lexicon.rs` (case-folded, whitespace-normalised substring of the first 300 chars of the input).
- `passages[].terms`: map of `English phrase` → `{substrate, gloss, citation}`
- `terms[].substrate`: the source-language term in its canonical transliteration. Greek uses macrons (`hēmin`, not `hemin`). Sanskrit uses IAST (`ātman`, not `atman`). Chinese uses pinyin with tone marks (`dào`, not `dao`).
- `terms[].gloss`: one-line PHILOSOPHICAL meaning. Not a dictionary definition. Should answer "what did the author mean by this concept" not "what does this English word mean".
- `terms[].citation`: optional. The scholarly source you verified the term against. See below.

## Citations

`citation` is optional in the schema: the loader accepts terms without it, and none of the current files carry one yet. New terms should include one so a reviewer can check the substrate against its source. Nothing checks this automatically; review is by hand.

**Acceptable citation formats:**

- Dictionary lookup: `"LSJ s.v. ἕξις"` (Liddell-Scott-Jones for Greek)
- Latin dictionary: `"Lewis & Short s.v. substantia"`
- Sanskrit: `"Monier-Williams s.v. ātman"`
- Pali: `"PED s.v. dukkha"` (Pali-English Dictionary)
- Chinese: `"Kroll s.v. 道; CTP Tao Te Ching 1"` (Kroll's Student's Dictionary + Chinese Text Project)
- Primary source position: `"Long, Discourses 1.1.7"` (author + work + locator)
- Scholarly anthology: `"Long & Sedley, The Hellenistic Philosophers, §53A"`
- SEP entry: `"SEP s.v. 'Stoicism', §3.2"`
- URL of verification source acceptable as fallback: `"https://www.perseus.tufts.edu/hopper/morph?l=..."`

Multiple sources can be combined with semicolons.

## Anti-patterns

These are the failure modes we've observed in LLM-generated lexicons. Treat them as automatic FAIL:

1. **Anachronisms** — attributing a term to a tradition that didn't have it. Example: tagging `amor fati` as a Stoic substrate. The phrase is Nietzsche's coinage from *The Gay Science* (1882); no Stoic source uses it. The doctrine of consent to *logos* is Stoic; the phrase is not.

2. **Garbled transliterations** — using a verb form where a noun belongs, or wrong inflection. Example: `diplē lambanē` for "two handles" — `lambanē` is a verb form; the standard Greek is `labai` (plural of `labē`).

3. **Plausible-but-fictional terms** — vocabulary that looks like authentic source-language morphology but doesn't appear in the corpus. Catch these by failing to find the term in Perseus / Monier-Williams / Pali Text Society / ctext.org.

4. **Misattribution across authors** — Aristotelian term tagged to Plato, Heideggerian term tagged to Husserl, etc.

5. **Dictionary glosses instead of philosophical glosses** — `opinion: what we think` is a dictionary definition, not a philosophical gloss. The gloss should explain what the AUTHOR meant by the concept, not what the English word means.

## Verifying your own contribution before submitting

If you are submitting via PR (community contribution), or running a lexicon-generation agent, you MUST verify each substrate term BEFORE writing it to the YAML:

1. Use WebFetch / WebSearch / your dictionary of choice to look up the term in the appropriate authoritative source.
2. Confirm the term exists in the source language as you've written it.
3. Confirm it is used in the philosophical sense you're attributing.
4. Add the verification source as the `citation` field.

If you cannot verify a term, **omit it**. Wrong substrate is worse than no substrate.

## Contributing

Open a PR adding YAML files for one source text (or closely related text group). One file per book or per author within a tradition.

Filename conventions:
- `<tradition>-<author>-<text>.yaml` (e.g. `stoic-epictetus-enchiridion.yaml`, `kantian-groundwork.yaml`)
- Use kebab-case
- Avoid duplicates — check existing files first

All fabricated substrate terms are grounds for PR rejection.
