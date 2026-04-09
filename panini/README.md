<div align="center">
  <h1>Pāṇini</h1>
  <p><b>A LLM-powered linguistic feature extraction framework</b></p>
  <p>
    <a href="https://crates.io/crates/panini-engine"><img src="https://img.shields.io/crates/v/panini-engine.svg" alt="Crates.io" /></a>
    <a href="https://pypi.org/project/panini-lang/"><img src="https://img.shields.io/pypi/v/panini-lang.svg" alt="PyPI" /></a>
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" />
    <a href="https://github.com/yro7/panini/actions/workflows/publish-panini-lang.yml"><img src="https://github.com/yro7/panini/actions/workflows/publish-panini-lang.yml/badge.svg" alt="Publish to PyPI" /></a>
  </p>
  <p>
    Usage: <a href="#python">Python</a> | <a href="#as-a-library-rust-api">Rust</a> | <a href="#as-a-standalone-cli">CLI</a>
  </p>
</div>

<br>

Pāṇini is a linguistic feature extraction framework: describe your language's morphology as Rust types, write extraction directives, and the pipeline handles the rest — prompt assembly, JSON schema generation, LLM orchestration, response parsing, and validation. No universal schema imposed; you define exactly the features your language needs.

## Table of Contents
- [Extraction Capabilities](#extraction-capabilities)
  - [Available components](#available-components)
  - [Examples](#output-examples)
- [Usage](#usage)
  - [As a library (Rust API)](#as-a-library-rust-api)
  - [As a standalone CLI](#as-a-standalone-cli)
  - [Python](#python)
- [Adding a language](#adding-a-language)
- [Adding an analysis component](#adding-an-analysis-component)
- [Building](#building)
- [License](#license)

## Extraction Capabilities

Extraction is built around **composable components** (`AnalysisComponent`). Each component provides a different axis of analysis or extracts specific features. You choose which components to run per request — pick only what you need.

By default, all compatible components are run. You can restrict the selection with `--components`:

```bash
# Run only morphology and Leipzig glossing
panini extract --components morphology,leipzig_alignment \
  --text "Gila abur-u-n ferma güğüna amuq'-da-č." \
  --target "amuq'-da-č"

# Run everything (default)
panini extract --text "Dał kotowi mleko." --target kotowi
```

From the Rust API, pass an optional list of component keys to `extract_erased_with_components()`:

```rust
let result = registry::extract_erased_with_components(
    "pol", &model, &request,
    Some(&["morphology", "pedagogical_explanation"]),
    0.2, 4096, None, &prompts,
).await?;
```

### Available components

| Key                       | Component                | Description                                                                                     | Compatibility                 |
| ------------------------- | ------------------------ | ----------------------------------------------------------------------------------------------- | ----------------------------- |
| `morphology`              | `MorphologyAnalysis`     | POS tagging, lemmatization, case/tense/aspect/gender — language-specific morphological features | All languages                 |
| `pedagogical_explanation` | `PedagogicalExplanation` | Structured HTML explanation for learners (translations, analysis, grammar recap)                | All languages                 |
| `morpheme_segmentation`   | `MorphemeSegmentation`   | Morpheme-by-morpheme segmentation with grammatical function labels                              | Agglutinative languages only* |
| `multiword_expressions`   | `MultiwordExpressions`   | Extracts idioms, collocations, and phrasal expressions                                          | All languages                 |
| `leipzig_alignment`       | `LeipzigAlignment`       | Leipzig-style interlinear morpheme-by-morpheme gloss (Leipzig Glossing Rules)                   | All languages                 |

*Agglutinative languages are marked with a "Agglutinative" trait implementation in the framework. You can define the implementation for any language, even for low-agglutination languages like french, etc.

### Output examples

#### `morphology`

Polish — `"Studentka czyta interesującą książkę w bibliotece."`

```json
{
  "morphology": {
    "target_features": [
      { "word": "studentka", "morphology": { "pos": "noun", "lemma": "studentka", "gender": "feminine", "case": "nominative" } },
      { "word": "czyta",     "morphology": { "pos": "verb", "lemma": "czytać", "tense": "present", "aspect": "imperfective" } }
    ],
    "context_features": [
      { "word": "interesującą", "morphology": { "pos": "adjective", "lemma": "interesujący", "gender": "feminine", "case": "accusative" } },
      { "word": "w",            "morphology": { "pos": "adposition", "lemma": "w", "governed_case": "locative" } },
      { "word": "bibliotece",   "morphology": { "pos": "noun", "lemma": "biblioteka", "gender": "feminine", "case": "locative" } }
    ]
  }
}
```

#### `pedagogical_explanation`

```json
{
  "pedagogical_explanation": "<p><b>Translations:</b><br><i>Lit:</i> Student-female reads interesting book in library.<br><i>Nat:</i> The (female) student reads an interesting book in the library.</p><p><b>Analysis:</b></p><ul><li><span style='color:#3498db'><b>studentka</b></span> — nominative (subject)...</li></ul><div style='background-color:#3a3a3a;color:#e0e0e0;padding:10px;border-radius:5px;margin-top:10px;border-left:4px solid #3498db'><b>Grammar Recap:</b><br>Accusative case marks the direct object...</div>"
}
```

#### `morpheme_segmentation`

Turkish — `"Öğrenciler kütüphanede kitap okuyorlar."`

```json
{
  "morpheme_segmentation": [
    {
      "word": "öğrenciler",
      "morphemes": [
        { "surface": "ler", "base_form": "lAr", "function": { "category": "number", "value": "plural" } }
      ]
    },
    {
      "word": "okuyorlar",
      "morphemes": [
        { "surface": "yor", "base_form": "(I)yor", "function": { "category": "tense", "value": "present" } },
        { "surface": "lar", "base_form": "lAr", "function": { "category": "agreement", "person": "third", "number": "plural" } }
      ]
    }
  ]
}
```

#### `multiword_expressions`

Polish — `"Dał nogę przed policją."`

```json
{
  "multiword_expressions": [
    {
      "expression": "dać nogę",
      "translation": "to run away / to bolt",
      "type": "idiom"
    }
  ]
}
```

#### `leipzig_alignment`

Lezgian — `"Gila abur-u-n ferma hamišaluǧ güǧüna amuq'-da-č."`

```json
{
  "leipzig_alignment": {
    "original_script": "Gila abur-u-n ferma hamišaluǧ güǧüna amuq'-da-č.",
    "words": [
      { "source": "Gila",           "gloss": "now" },
      { "source": "abur-u-n",       "gloss": "they-OBL-GEN" },
      { "source": "ferma",          "gloss": "farm" },
      { "source": "hamišaluǧ",      "gloss": "forever" },
      { "source": "güǧüna",         "gloss": "behind" },
      { "source": "amuq'-da-č",     "gloss": "stay-FUT-NEG" }
    ],
    "free_translation": "Now their farm will not stay behind forever."
  }
}
```

## Usage

### As a library (Rust API)

Add `panini-engine` and `rig-core` to your `Cargo.toml`:

```toml
panini-engine = { path = "…" }
panini-langs   = { path = "…" }
rig-core       = "0.33"
```

Then call `extract_features_via_llm` with any `rig::completion::CompletionModel`:

```rust
use panini_engine::{extract_features_via_llm, ExtractionRequest};
use panini_engine::prompts::ExtractorPrompts;
use panini_langs::polish::Polish;
use rig::providers::openai;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = openai::Client::new(&std::env::var("OPENAI_API_KEY")?)?;
    let model  = client.completion_model("gpt-4o");
    let prompts = ExtractorPrompts::load("prompts/default.yml")?;

    let request = ExtractionRequest::builder()
        .content("Dał kotowi mleko.")
        .targets(vec!["kotowi".to_string()])
        .build();

    let result = extract_features_via_llm(
        &Polish,
        &model,
        &request,
        0.2,
        4096,
        None,
        &prompts,
    ).await?;

    println!("{:#?}", result.target_features);
    Ok(())
}
```

### As a standalone CLI

**1. Install**

```bash
# from the workspace root
cargo install --path panini-cli

# or build locally
cargo build -p panini-cli --release
```

**2. Create a config file** (copy from `panini.example.toml`):

```toml
# panini.toml
provider     = "google"           # openai | anthropic | google
model        = "gemini-2.0-flash"
language     = "pol"              # pol | tur | ara
api_key      = "$GEMINI_API_KEY"
prompts_file = "panini-cli/prompts/default.yml"
```

**3. Run**

```bash
export GEMINI_API_KEY="$GEMINI_API_KEY"

panini extract \
  --config panini.toml \
  --text "Studentka czyta interesującą książkę." \
  --target studentka --target czyta --target książkę

# Select specific components
panini extract --config panini.toml \
  --text "Dał kotowi mleko." --target kotowi \
  --components morphology,leipzig_alignment

# List supported languages
panini languages

# Pipe output to jq
panini extract --config panini.toml --text "…" --target "…" \
  | jq '.morphology.target_features'
```

**CLI options**

| Flag            | Default                  | Description                                       |
| --------------- | ------------------------ | ------------------------------------------------- |
| `--config`      | `panini.toml`            | Path to TOML config                               |
| `--text`        | *(required)*             | Sentence / card content to analyse                |
| `--target`      | *(required, repeatable)* | Target word(s) to focus extraction on             |
| `--components`  | *(all)*                  | Comma-separated list of components to run         |
| `--temperature` | `0.2`                    | Sampling temperature                              |
| `--max-tokens`  | `4096`                   | Max tokens for LLM response                       |
| `--ui-language` | `English`                | Learner's UI language for pedagogical explanation |

### Python

```bash
pip install panini-lang
```

```python
from panini import extract

result = extract(
    provider="openai",        # "openai" | "anthropic" | "google"
    model="gpt-4o",
    api_key="sk-...",
    language="pol",           # ISO 639-3 code
    text="Dał kotowi mleko.",
    targets=["kotowi"],
)
```

→ See [pynini/README.md](pynini/README.md) for the full Python API reference.

---

## What you define, what the framework does

**You define:**
- A **morphology enum** — the features you want extracted (POS, case, tense, aspect, gender… whatever your language needs)
- **Extraction directives** — natural-language instructions that guide the LLM on how to analyze your language
- **Optional morpheme segmentation** — for agglutinative languages, a morpheme inventory with validation rules
- **Optional post-processing** — hooks to validate or enrich the LLM's output after parsing

**The framework handles:**
- **Prompt assembly** — combines your directives, the generated schema, learner context, and pedagogical focus into a structured prompt
- **JSON schema generation** — automatically derived from your Rust types, so the LLM is constrained to return exactly what you defined
- **LLM orchestration** — provider-agnostic; bring your own client (OpenAI, Anthropic, Google, local)
- **Response parsing & validation** — deserializes the LLM output into your typed structs, rejects malformed responses, supports retry with self-correction

## Design principles

- **No universal schema.** Each language defines its own morphology enum with exactly the features it needs. Polish has 7 cases and verbal aspect. Arabic has triliteral roots and wazn patterns. There is no lowest-common-denominator `Morphology` struct.
- **Type safety over convention.** Morphology variants are strongly typed Rust enums, validated at compile time via `#[derive(MorphologyInfo)]`. Every variant must carry a `lemma`. The LLM's JSON output is parsed into these types and rejected if it doesn't conform.
- **LLM as untrusted source.** Responses are validated against a JSON schema, deserialized into typed structs, then post-processed. On parse failure, the raw output and error are returned for retry with self-correction.
- **Provider-agnostic via rig.** The engine accepts any `rig::completion::CompletionModel` — OpenAI, Anthropic, Google Gemini, Mistral, Ollama, or any custom provider.
- **Opt-in complexity.** A simple language (Polish) needs a morphology enum and a few directives. An agglutinative language (Turkish) can opt into morpheme inventories, segmentation, and validation. You only implement what you need.

## Workspace structure

```
panini/              # Facade crate, re-exports everything
panini-core/         # Traits, domain types, morphology enums, components
  src/components/    # AnalysisComponent implementations
panini-engine/       # LLM extraction pipeline, prompt assembly, schema composer
panini-langs/        # Per-language implementations (Polish, Arabic, Turkish)
panini-macro/        # #[derive(MorphologyInfo)] proc macro
```

## Adding a language

### Automatically (LLM-assisted)

Fill your `panini.toml` config, choose a language (with its ISO 639-3 code) and run:

`cargo run -p panini-cli -- add-language --language "French" --iso-code fra --config panini.toml`

You should ALWAYS check the file output, especially for linguistic definitions, to ensure the LLM properly described the language.

### Manually (step by step)

1. Create `panini-langs/src/<language>.rs`
2. Define a `Morphology` enum with `#[derive(MorphologyInfo)]` and `#[serde(tag = "pos")]` -- every variant must have `lemma: String` as its first field
3. Implement `LinguisticDefinition` on a unit struct
4. For agglutinative languages, also implement `Agglutinative` with a morpheme inventory

See `panini-langs/src/polish.rs` or `panini-langs/src/turkish.rs` as references.

## Adding an analysis component

To add a new component (e.g. `leipzig_alignment`), touch 3 files:

**1. Create the component** in `panini-core/src/components/<name>.rs`:

```rust
use std::fmt::Debug;
use crate::component::{AnalysisComponent, ComponentContext};
use crate::traits::LinguisticDefinition;

#[derive(Debug, Clone)]
pub struct MyComponent;

impl<L: LinguisticDefinition> AnalysisComponent<L> for MyComponent {
    fn name(&self) -> &'static str { "My Component" }
    fn schema_key(&self) -> &'static str { "my_component" }

    fn schema_fragment(&self, _lang: &L) -> serde_json::Value {
        // JSON Schema for the component's output
        serde_json::json!({
            "type": "object",
            "properties": {
                "field": { "type": "string" }
            },
            "required": ["field"]
        })
    }

    fn prompt_fragment(&self, _lang: &L, _ctx: &ComponentContext) -> String {
        "Instructions for the LLM on how to produce this component's output.".to_string()
    }

    // Optional overrides:
    // fn is_compatible(&self, lang: &L) -> bool  — filter by language/typology
    // fn output_instruction(&self) -> Option<&str>  — extra output rules
    // fn pre_process(&self, raw: &str) -> String  — clean raw JSON before parsing
    // fn validate(&self, lang: &L, section: &Value) -> Result<(), String>
    // fn post_process(&self, lang: &L, section: &mut Value) -> Result<(), String>
}
```

**2. Register the module** in `panini-core/src/components/mod.rs`:

```rust
pub mod my_component;
pub use my_component::MyComponent;
```

**3. Add to the registry** in `panini-langs/src/registry.rs` (`extract_for_language`):

```rust
let my_comp = MyComponent;
let all_components: Vec<(&str, &dyn AnalysisComponent<L>)> = vec![
    // ...existing...
    ("my_component", &my_comp),
];
```

That's it. The component is automatically integrated into the schema and prompt via `compose_schema()` / `compose_prompt()`. Use it with `--components my_component`.

## Building

```bash
cargo build
cargo test
```

## License

MIT
