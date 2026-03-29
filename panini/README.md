# Pāṇini

**A LLM-powered linguistic feature extraction framework.**

Pāṇini lets you define *what* linguistic features to extract and *how* to extract them, for any language : You describe your language's morphology as Rust types, write extraction directives, and the framework handles the rest: prompt assembly, JSON schema generation, LLM orchestration, response parsing, and validation.

Pāṇini doesn't impose a universal schema — you define exactly the features that matter for your language, and the framework builds the extraction pipeline around your definitions.

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
use rig::client::CompletionClient;
use rig::providers::openai;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = openai::Client::new("sk-…")?;
    let model  = client.completion_model("gpt-4o");
    let prompts = ExtractorPrompts::load("prompts/default.yml")?;

    let request = ExtractionRequest {
        content: "Dał kotowi mleko.".to_string(),
        targets: vec!["kotowi".to_string()],
        pedagogical_context: Some("Dative case".to_string()),
        skill_path: Some("grammar/cases/dative".to_string()),
        learner_ui_language: "English".to_string(),
        linguistic_background: vec![],
        user_prompt: None,
    };

    let result = extract_features_via_llm(
        &Polish,
        &model,
        &request,
        0.2,   // temperature
        4096,  // max tokens
        None,  // no previous attempt
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
api_key      = "${GEMINI_API_KEY}"
prompts_file = "panini-cli/prompts/default.yml"
```

**3. Run**

```bash
export GEMINI_API_KEY="…"

panini extract \
  --config panini.toml \
  --text "Studentka czyta interesującą książkę." \
  --target studentka --target czyta --target książkę

# List supported languages
panini languages

# Pipe output to jq
panini extract --config panini.toml --text "…" --target "…" \
  | jq '.target_features'
```

**CLI options**

| Flag | Default | Description |
|------|---------|-------------|
| `--config` | `panini.toml` | Path to TOML config |
| `--text` | *(required)* | Sentence / card content to analyse |
| `--target` | *(required, repeatable)* | Target word(s) to focus extraction on |
| `--temperature` | `0.2` | Sampling temperature |
| `--max-tokens` | `4096` | Max tokens for LLM response |
| `--ui-language` | `English` | Learner's UI language for pedagogical explanation |

## Examples

### Polish — case & aspect

**Input:** `Studentka czyta interesującą książkę w bibliotece.`  
**Targets:** `studentka`, `czyta`, `książkę`

```json
{
  "target_features": [
    { "word": "studentka", "morphology": { "pos": "noun",  "lemma": "studentka", "gender": "feminine", "case": "nominative" } },
    { "word": "czyta",     "morphology": { "pos": "verb",  "lemma": "czytać",    "tense": "present",   "aspect": "imperfective" } },
    { "word": "książkę",   "morphology": { "pos": "noun",  "lemma": "książka",   "gender": "feminine", "case": "accusative" } }
  ],
  "context_features": [
    { "word": "interesującą", "morphology": { "pos": "adjective",  "lemma": "interesujący", "gender": "feminine", "case": "accusative" } },
    { "word": "w",            "morphology": { "pos": "adposition", "lemma": "w",            "governed_case": "locative" } },
    { "word": "bibliotece",   "morphology": { "pos": "noun",       "lemma": "biblioteka",   "gender": "feminine", "case": "locative" } }
  ]
}
```

---

### Turkish — agglutinative morpheme segmentation

**Input:** `Öğrenciler kütüphanede kitap okuyorlar.`  
**Targets:** `öğrenciler`, `okuyorlar`

```json
{
  "target_features": [
    { "word": "öğrenciler", "morphology": { "pos": "noun", "lemma": "öğrenci", "case": "nominative", "number": "plural" } },
    { "word": "okuyorlar",  "morphology": { "pos": "verb", "lemma": "okumak",  "tense": "present", "mood": "indicative",
                                            "voice": "active", "person": "third", "number": "plural", "polarity": "positive" } }
  ],
  "morpheme_segmentation": [
    {
      "word": "öğrenciler",
      "morphemes": [{ "surface": "ler", "base_form": "lAr", "function": { "category": "number", "value": "plural" } }]
    },
    {
      "word": "okuyorlar",
      "morphemes": [
        { "surface": "yor", "base_form": "(I)yor", "function": { "category": "tense",     "value": "present" } },
        { "surface": "lar", "base_form": "lAr",    "function": { "category": "agreement", "person": "third", "number": "plural" } }
      ]
    }
  ]
}
```

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
panini-core/         # Traits, domain types, morphology enums
panini-engine/       # LLM extraction pipeline, prompt assembly
panini-langs/        # Per-language implementations (Polish, Arabic, Turkish)
panini-macro/        # #[derive(MorphologyInfo)] proc macro
```

## Adding a language

1. Create `panini-langs/src/<language>.rs`
2. Define a `Morphology` enum with `#[derive(MorphologyInfo)]` and `#[serde(tag = "pos")]` -- every variant must have `lemma: String` as its first field
3. Implement `LinguisticDefinition` on a unit struct
4. For agglutinative languages, also implement `Agglutinative` with a morpheme inventory

See `panini-langs/src/polish.rs` or `panini-langs/src/turkish.rs` as references.

## Building

```bash
cargo build
cargo test
```

## License

MIT
