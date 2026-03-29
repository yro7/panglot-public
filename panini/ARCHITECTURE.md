# How Panini is used in Panglot

## 1. Core types — `panini-core` via `lc_core`

Panglot never does `use panini_core::…` directly (except in `engine/` and `langs/`). Everything flows through `lc_core` re-exports:

**core/src/traits.rs:7**
```rust
pub use panini_core::traits::*;
// → LinguisticDefinition, MorphologyInfo, Script, IsoLang, Person, BinaryNumber, etc.
```

**core/src/morpheme.rs:2**
```rust
pub use panini_core::morpheme::*;
// → FeatureExtractionResponse, WordSegmentation, Agglutinative, MorphemeDefinition
```

**core/src/domain.rs:4**
```rust
pub use panini_core::domain::{ExtractedFeature, MultiwordExpression};
```

**core/src/morphology_enums.rs:2**
```rust
pub use panini_core::morphology_enums::*;
// → BinaryGender, TernaryGender, SlavicAspect, BinaryVoice, etc.
```

The rest of Panglot (`app/`, `anki_bridge/`, etc.) continues to write `use lc_core::traits::Language` — it doesn't even know panini exists.

---

## 2. Language definitions — `panini-langs` via `langs/`

Two patterns coexist:

### Pattern A — Migrated languages (Polish, Turkish, Arabic)

Enums come from `panini-langs`, the unit struct is local.

**langs/src/polish.rs:**
```rust
pub use panini_langs::polish::*;  // PolishCase, PolishGender, PolishMorphology, etc.

pub struct Polish;  // local struct, shadows the Polish from panini-langs

impl LinguisticDefinition for Polish {
    type Morphology = PolishMorphology;     // type imported from panini-langs
    type GrammaticalFunction = ();

    fn iso_code(&self) -> IsoLang {
        panini_langs::polish::Polish.iso_code()       // delegates
    }
    fn supported_scripts(&self) -> &[Script] {
        panini_langs::polish::Polish.supported_scripts()  // delegates
    }
    // ... same for default_script, typological_features, extraction_directives
}

impl Language for Polish {  // Panglot trait (generation only)
    type ExtraFields = NoExtraFields;
    fn generation_directives(&self) -> Option<&str> { Some("...") }
    fn ipa_strategy(&self) -> IpaConfig { IpaConfig::Epitran("pol-Latn") }
    fn tts_strategy(&self) -> TtsConfig { TtsConfig::Edge { voice: "pl-PL-ZofiaNeural" } }
}
```

Turkish (`langs/src/tur.rs`) additionally delegates `build_extraction_schema`, `extra_extraction_directives` and `post_process_extraction` (agglutination).

### Pattern B — Non-migrated languages (Russian, Korean, Mandarin, Japanese)

Everything is defined locally with two separate `impl` blocks (`LinguisticDefinition` + `Language`).

---

## 3. The proc-macro — `panini_core::traits::MorphologyInfo`

**lc_macro/src/lib.rs:60** — `#[derive(MorphologyInfo)]` generates:
```rust
impl panini_core::traits::MorphologyInfo for PolishMorphology {
    fn lemma(&self) -> &str { ... }
    fn pos_tag(&self) -> Self::PosTag { ... }
    fn pos_label(&self) -> &'static str { ... }
}
```
This is the same trait as `lc_core::traits::MorphologyInfo` via the re-export.

---

## 4. Prompt structs — `panini-engine` via `engine/src/prompts.rs`

**engine/src/prompts.rs:48-50:**
```rust
pub use panini_engine::prompts::{
    ExtractorPrompts, LearnerProfile, SkillContextPrompts,
};
```

`PromptConfig` uses the panini type directly:
```rust
pub struct PromptConfig {
    pub generator: GeneratorPrompts,     // Panglot only
    pub extractor: ExtractorPrompts,     // ← panini-engine type
    pub user_messages: UserMessages,
    pub common: CommonPrompts,
}
```

YAML loading (`PromptConfig::load`) deserializes `extractor.yaml` directly into the panini type.

---

## 5. LLM utilities — `panini-engine` via `engine/src/llm_utils.rs`

**engine/src/llm_utils.rs:2:**
```rust
pub use panini_engine::llm_utils::{clean_llm_json, normalize_pos_tags};
```

---

## 6. The extraction call — the core of the delegation

### 6a. The LLM adapter — `engine/src/panini_adapter.rs`

```rust
pub struct PaniniLlmAdapter<'a> {
    pub inner: &'a dyn llm_client::LlmClient,      // Panglot client
    pub request_context: Option<RequestContext>,     // billing/tracing
}

#[async_trait]
impl panini_engine::LlmClient for PaniniLlmAdapter<'_> {
    async fn chat_completion(
        &self,
        request: &panini_engine::llm::LlmRequest,
    ) -> Result<panini_engine::llm::LlmResponse> {
        // Convert panini messages → Panglot messages
        let messages = request.messages.iter().map(|m| llm_client::ChatMessage {
            role: match m.role {
                panini_engine::llm::Role::System => llm_client::Role::System,
                // ...
            },
            content: m.content.clone(),
        }).collect();

        let panglot_request = llm_client::LlmRequest {
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_schema: request.response_schema.clone(),
            request_context: self.request_context.clone(),  // ← billing preserved
            call_type: CallType::Extraction,
        };

        let response = self.inner.chat_completion(&panglot_request).await?;
        Ok(panini_engine::llm::LlmResponse { content: response.content })
    }
}
```

### 6b. The extraction wrapper — `engine/src/feature_extractor.rs`

```rust
pub async fn extract_features_via_llm<L: Language + Send + Sync>(
    language: &L,
    node: &SkillNode,
    node_path: &str,
    llm_client: &dyn LlmClient,   // Panglot client
    req: &GenerationRequest<L>,
    card_json: &str,
    targets: &[String],
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    prompt_config: &PromptConfig,
) -> Result<FeatureExtractionResponse<L::Morphology, L::GrammaticalFunction>>
{
    // 1. Adapt the LLM client
    let adapter = PaniniLlmAdapter {
        inner: llm_client,
        request_context: req.request_context.clone(),
    };

    // 2. Convert Panglot context → panini ExtractionRequest
    let extraction_request = panini_engine::prompts::ExtractionRequest {
        content: card_json.to_string(),
        targets: targets.to_vec(),
        pedagogical_context: node.node_instructions.clone(),
        skill_path: Some(node_path.to_string()),
        learner_ui_language: req.user_profile.ui_language.clone(),
        linguistic_background: to_panini_language_levels(&req.user_profile.linguistic_background),
        user_prompt: req.user_prompt.clone(),
    };

    // 3. Delegate to panini-engine
    panini_engine::extract_features_via_llm(
        language,              // impl LinguisticDefinition (via Language: LinguisticDefinition)
        &adapter,              // impl panini_engine::LlmClient
        &extraction_request,
        temperature,
        max_tokens,
        previous_attempt,
        &prompt_config.extractor,   // ← already the panini-engine type
    ).await
}
```

### 6c. The call from the pipeline — `engine/src/pipeline.rs:371`

```rust
// The pipeline calls the same signature as before — it doesn't know it delegates
let result = crate::feature_extractor::extract_features_via_llm(
    &self.language, ext_node, &extraction_path,
    client.as_ref(), req,
    &card_json_for_extraction, &targets,
    self.extractor_temperature, self.extractor_max_tokens,
    None, &self.prompt_config,
).await;
```

The pipeline interface hasn't changed. The delegation is transparent.

---

## Summary — call flow

```
pipeline.rs                          (Panglot)
  └→ feature_extractor.rs            (Panglot — thin wrapper)
       ├→ PaniniLlmAdapter           (Panglot — adapts the LLM client)
       └→ panini_engine::extract_features_via_llm()   (Panini)
            ├→ build_extraction_prompt()               (Panini — assembles the prompt)
            ├→ adapter.chat_completion()               (→ Panglot LlmHttpClient)
            ├→ clean_llm_json() + normalize_pos_tags() (Panini)
            ├→ serde_json::from_str::<FeatureExtractionResponse>()  (Panini types)
            └→ language.post_process_extraction()      (Panini trait)
```
