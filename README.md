# Panglot

An open-source, LLM-powered language learning engine that generates exercises and extracts linguistic features from any language for pedagogical purposes.

The project is entirely source/target language-agnostic but still leverages the specific traits of each language to generate tailored exercises based on users' progression.

The engine is compatible with Anki, you can export .apkg and import them to your local collection :) 

## Architecture

Panglot is a modular Rust workspace:

- **`core/`** — Domain types, database layer, SRS algorithms (FSRS, SM-2, Leitner), user profiles
- **`engine/`** — LLM pipeline: exercise generation, morphological feature extraction, post-processing
- **`langs/`** — Per-language definitions: morphology enums, typological features, IPA/TTS configs
- **`anki_bridge/`** — Export to Anki `.apkg` decks
- **`codegen/`** — Build-time code generation utilities
- **`lc_macro/`** — Derive macros (e.g. `MorphologyInfo`)
- **`prompts/`** — YAML prompt templates for the LLM pipeline (see `example_*.yaml`)
- **`configs/`** — Per-language skill trees defining exercise categories

## Adding a new language

1. Create `langs/src/<lang>.rs` implementing the `Language` trait (see `polish.rs` or `arabic.rs` as examples)
2. Create `configs/<iso>_tree.yaml` defining the skill tree
3. The build script (`langs/build.rs`) auto-discovers and registers new languages

## Prompt system

Prompts are YAML templates with `{placeholder}` variables, loaded at runtime by the engine. See `prompts/example_*.yaml` for the expected structure.

## Python sidecar

`scripts/sidecar.py` handles IPA transcription (via `epitran`) and TTS (via `edge-tts`), called from Rust through `engine/src/python_sidecar.rs`.

## Configuration

Copy `config.yml` and adjust LLM provider, model, and paths. API keys go in `.env` (see `.env.example`).

## Building

```bash
cargo build --release
```

## License

MIT
