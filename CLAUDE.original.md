# Panglot

LLM-powered language learning engine. Generates exercises (Anki flashcards) and extracts morphological linguistic features for any language.

## Philosophy

### Radical linguistic agnosticism
Each language defines what *it* needs — not what all languages share. There is no universal morphological schema. Polish has its 7 cases and verbal aspect. Japanese has its particles, agglutination, and 3 scripts. Arabic has its triliteral roots and wazn patterns. Mandarin has its classifiers and aspect particles. 4 languages = 4 entirely distinct type systems.

### No Indo-European/Western bias
The code acknowledges each language's specificities in its LLM directives: Mandarin is topic-prominent, Arabic has reverse gender agreement for numbers 3-10, Japanese requires explicit de-agglutination. Generation and extraction directives are linguistically informed, not modeled after English grammar.

### Scripts and pedagogical transliterations
Scripts are validated via ISO 15924 and are first-class citizens. Japanese supports 3 scripts (Hani, Hira, Kana). The `transliteration` field in `GenerationRequest` is designed for pedagogical systems (pinyin, furigana, etc.). The system naturally integrates the transliterations needed for learning.

### Linguistics-driven pedagogy
Skill trees reflect how linguists actually teach each language: Polish starts with cases, Japanese with writing systems, Mandarin with tones and pronunciation, Arabic with the alphabet and diacritics. Nodes carry language-specific `node_instructions` for the LLM.

### Universal coverage
The system is designed to support any human language, including dialects. The `codegen` tool can generate skeletons, and the build system auto-detects new languages. Adding a language = 1 Rust file + 1 YAML.

### Open-core model
Panglot is partially open source. The engine (core, engine, langs, anki_bridge, lc_macro, codegen) is public; the hosted product layer (app, frontend, real prompts, internal docs) is private. A `scripts/sync_to_public.sh` script uses `git-filter-repo` to produce a filtered clone pushed to `panglot-public`. What stays **private**: production prompts (`prompts/*.yaml`), some language implementations (cmn, jpn), the frontend (`app/static/`), internal docs (roadmap, audit, implementation plans), secrets, and the sync script itself. What goes **public**: the full engine, core traits, macros, Anki bridge, Polish + Arabic as reference languages with their skill trees, and example prompts. `README_public.md` is renamed to `README.md` in the public repo. When adding new features, consider whether they belong to the open engine or the private product layer — infrastructure-level code (observability, traits, pipeline) should stay public; business logic (billing, auth, rate limiting) should be feature-gated or kept private.

## Software values

- **Type safety as a guarantee** — Associated types on `Language` (`Morphology`, `ExtraFields`), bounded types in `validated.rs` (impossible to construct invalid values), exhaustive `AnyCard` via macro, `#[derive(MorphologyInfo)]` enforces `lemma` on every morphological enum variant.
- **Compile-time over runtime** — `langs/build.rs` generates the registry at build time, `dispatch_iso!` resolves languages at compilation, `include_dir!` embeds trees, `static_assertions` verifies Send+Sync. The build fails if a language is missing its tree YAML.
- **Modularity through traits** — `Language`, `CardModel`, `LlmClient`, `StorageProvider`, `SrsAlgorithm`, `EarlyPostProcessor`, `LatePostProcessor`, `CardValidator`. Each interface is minimal and focused. Pipeline accepts injected dependencies, never direct coupling.
- **LLM as untrusted source** — Every LLM response is validated against a JSON schema, parsed into strongly-typed Rust structs, then re-validated by CardValidators. Automatic retry with feedback on validation failure.
- **Defensive by default** — Explicit HTML sanitization (character by character in `sanitize.rs`), input validation at type construction, no framework magic.
- **Explicit concurrency** — Semaphores passed as parameters (not global), `tokio::sync::RwLock` for the LLM client, `parking_lot::RwLock` for short synchronous access.
- **Separation of concerns** — CardMetadata (linguistic analysis) != CardModel (HTML rendering) != Storage (persistence) != SRS (scheduling). Each component has a clear boundary.
- 
- **__PRIMORDIAL :__** **No NIH (Not Invented Here)** — Prefer established ecosystem solutions over custom implementations. If a well-maintained crate or industry standard exists, discuss it rather than building a bespoke system.

## Architecture

### Workspace (7 crates)

- **core/** (lc_core) — Core traits (`Language`, `CardModel`, `MorphologyInfo`), domain types (`CardMetadata`, `ExtractedFeature`, `LexiconEntry`), SQLite DB layer (sqlx), SRS algorithms (SM-2, FSRS-4.5/5/6, Leitner), skill trees, input validation (`validated.rs`), HTML sanitization
- **engine/** — Full LLM pipeline: exercise generation (`GeneratorContext`), morphological feature extraction (`FeatureExtractorContext`), post-processing (IPA/TTS), multi-provider LLM client (Google/Anthropic/OpenAI/custom), YAML prompts with `{placeholder}` variables
- **langs/** — Per-language implementations. Each language is a unit struct implementing `Language` with its own `Morphology` enum (POS-tagged via `#[serde(tag = "pos")]`). Build script (`build.rs`) auto-discovers files, generates `lang_registry.rs` with `dispatch_iso!` and `ALL_ISO_CODES`
- **anki_bridge/** — `.apkg` export (DeckBuilder, MultiDeckBuilder), AnkiConnect provider for local Anki sync
- **app/** (bin: panglot) — Actix-web server, REST API, Supabase JWT auth, vanilla JS SPA frontend in `app/static/`
- **lc_macro/** — Procedural macros: `#[derive(ToFields)]` (struct -> HashMap for Anki), `#[derive(MorphologyInfo)]` (enforces `lemma`, generates `.lemma()` and `.pos_label()`)
- **codegen/** (bin: lc-codegen) — LLM-assisted code generation for new languages (Rust skeleton + skill tree YAML)

### Generation pipeline

1. **Prompt** — `GeneratorContext` assembles language directives, skill node instructions, user profile, card model JSON schema
2. **LLM Call 1** — Content generation (temperature 0.8, structured output via JSON schema)
3. **Parse + Validation** — `AnyCard::parse<L>()` into typed structs, then `CardValidator::validate()`. On failure -> retry with feedback message
4. **In parallel**:
   - LLM Call 2 — Morphological feature extraction (temperature 0.0) -> `FeatureExtractionResponse<M>` with `target_features`, `context_features`, `pedagogical_explanation`
   - Early post-processing — IPA (epitran) + TTS (edge-tts) via Python sidecar
5. **Late post-processing** — Final `CardMetadata<M>` assembly

### Python sidecar
`scripts/sidecar.py` — Long-lived subprocess, JSON-line protocol. Commands: `ipa` (transcription via epitran), `tts` (audio via edge-tts), `quit`. Called from `engine/src/python_sidecar.rs`.

### Frontend
Vanilla JS SPA in `app/static/` (no framework). 14 JS modules, 7 CSS stylesheets. Pages: generator, skill tree, decks, study (SRS), lexicon, profile.

## Key patterns

- **`dispatch_iso!`** — Build-generated macro. Resolves an ISO code (`"pol"`, `"jpn"`, etc.) to the concrete `Language` type at compile time. Used in `app/src/main.rs` to instantiate pipelines.
- **`define_card_models!`** — Single macro (`engine/src/card_models.rs`) that generates `CardModelId` enum, `AnyCard` enum, JSON parsing, schema generation, `speakable_text()`, `front_html()`/`back_html()`.
- **`#[derive(MorphologyInfo)]`** — Every morphological enum variant MUST have `lemma: String` as its first field. The macro generates `.lemma()` and `.pos_label()`.
- **`#[derive(ToFields)]`** — Converts a struct into `HashMap<String, String>` for Anki export. Supports `#[serde(flatten)]` for nested structs.
- **Skill tree overlay** — Non-destructive per-user customization (add/hide/edit nodes) applied on top of the shared base tree.
- **`DynPipeline`** — Type-erasure of the generic `Pipeline<L>` for the API layer. HTTP handlers work with `Box<dyn DynPipeline>`, no generics leak out.
- **`OnceLock`** — Lazy caching of base trees, avoids re-parsing YAML on every request.

## Commands

```bash
cargo build --release            # Optimized build
cargo run -p app                 # Dev server (http://127.0.0.1:8080)
cargo run --release -p app       # Release server
cargo test --workspace           # All tests
cargo clippy --workspace         # Lint
cargo run -p codegen -- --iso <code>  # Generate skeleton for new language
```

## Adding a language

1. Create `langs/src/<code>.rs` — Unit struct implementing `Language`. Define the `Morphology` enum with `#[derive(MorphologyInfo)]` and `#[serde(tag = "pos")]` (every variant MUST have `lemma: String`). Define `ExtraFields` if needed (e.g., `context_disambiguation` for Japanese). Specify supported scripts, extraction/generation directives, IPA/TTS strategies.
2. Create `core/trees/<iso>_tree.yaml` — Skill tree reflecting the linguistic pedagogy specific to that language. Each node carries `node_instructions` to guide the LLM.
3. `cargo build` — The build script auto-discovers the file, verifies the tree YAML, generates the registry. Done.

## Configuration

- **`config.yml`** — LLM provider (`google`/`anthropic`/`openai`/`custom`), model, host/port, concurrency (`max_llm_calls`, `max_post_process`), Supabase auth, paths
- **`.env`** — `GOOGLE_API_KEY`, `ANTHROPIC_API_KEY`, `SUPABASE_URL`, `SUPABASE_ANON_KEY`, `SUPABASE_JWT_SECRET`
- **`prompts/*.yaml`** — Prompt templates with `{placeholder}` variables: `generator.yaml` (exercise generation), `extractor.yaml` (morphological extraction), `common.yaml`, `user.yaml`
