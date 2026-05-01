# Panglot

Open-source, LLM-powered language learning engine. Generates exercises + extracts linguistic features from any language for pedagogy.

Source/target language-agnostic. Still leverage per-language traits for tailored exercises based on user progression.

Anki compatible. Export `.apkg`, import to local collection :)

## Architecture

Panglot = modular Rust workspace:

- **`core/`** — Domain types, DB layer, SRS algorithms (FSRS, SM-2, Leitner), user profiles
- **`engine/`** — LLM pipeline: exercise gen, morphological feature extraction, post-processing
- **`langs/`** — Per-language defs: morphology enums, typological features, IPA/TTS configs
- **`anki_bridge/`** — Export to Anki `.apkg` decks
- **`codegen/`** — Build-time codegen utils
- **`lc_macro/`** — Derive macros (e.g. `MorphologyInfo`)
- **`prompts/`** — YAML prompt templates for LLM pipeline (see `example_*.yaml`)
- **`configs/`** — Per-language skill trees defining exercise categories

## Adding a new language

1. Create `langs/src/<lang>.rs` implementing `Language` trait (see `polish.rs` or `arabic.rs`)
2. Create `configs/<iso>_tree.yaml` defining skill tree
3. Build script (`langs/build.rs`) auto-discovers + registers new languages

## Prompt system

Prompts = YAML templates with `{placeholder}` vars, loaded at runtime by engine. See `prompts/example_*.yaml` for structure.

## Python sidecar

`scripts/sidecar.py` handles IPA transcription (via `epitran`) + TTS (via `edge-tts`). Called from Rust via `engine/src/python_sidecar.rs`.

## Configuration

Copy `config.yml`, adjust LLM provider, model, paths. API keys go in `.env` (see `.env.example`).

## Building

```bash
cargo build --release
```

## License

MIT