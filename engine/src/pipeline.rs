use serde::{Deserialize, Serialize};
use lc_core::domain::CardMetadata;
use lc_core::traits::{CardModel, Language};
use lc_core::user::UserSettings;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::card_models::CardModelId;
use crate::generator::GenerationRequest;
use crate::llm_client::{ChatMessage, LlmClient, LlmRequest, Role};
use crate::post_process::{EarlyPostProcessor, LatePostProcessor};
use crate::prompts::{GeneratorContext, PromptConfig};
use crate::skill_tree::SkillNode;
use crate::usage::{PostProcessEvent, PostProcessType, UsageRecorder};
use crate::validation::CardValidator;

use lc_core::storage::{StorageProvider, NewDeckData, NewCardEntry};

use crate::analyzer::{DynLexiconTracker, LexiconTracker, LibraryAnalyzer};
use crate::llm_utils::clean_llm_json;

// ----- Lexicon Status -----

/// Status of async lexicon loading.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LexiconStatus {
    NotStarted,
    Loading,
    Ready { word_count: usize },
    Failed { error: String },
}

// ----- Generation Result -----

/// A single generated card with its model and metadata.
pub struct GeneratedCard<P: std::fmt::Debug + Clone + Serialize + for<'de> Deserialize<'de>> {
    pub model: crate::card_models::AnyCard,
    pub metadata: CardMetadata<P>,
}

// ----- Pipeline Orchestrator -----

/// Orchestrates the full card generation pipeline:
///
/// 1. Build the LLM prompt from the skill tree + generation request
/// 2. Call the LLM to generate card content (Call 1)
/// 3. Parse + validate each card (with 1 retry on failure)
/// 4. In parallel: extract features (Call 2) + run early post-processors (TTS/IPA)
/// 5. Run late post-processors (with full metadata access)
///
/// The pipeline does NOT own a skill tree. The tree is injected at each call,
/// allowing per-user overlays without shared mutable state.
pub struct Pipeline<L: Language + Send + Sync> {
    pub language: L,
    llm_client: tokio::sync::RwLock<Box<dyn LlmClient>>,
    early_processors: Vec<Box<dyn EarlyPostProcessor<L>>>,
    late_processors: Vec<Box<dyn LatePostProcessor<L>>>,
    validators: Vec<Box<dyn CardValidator<L>>>,
    generator_temperature: f32,
    generator_max_tokens: u32,
    extractor_temperature: f32,
    extractor_max_tokens: u32,
    prompt_config: PromptConfig,
    usage_recorder: Option<UsageRecorder>,
    cached_base_tree: std::sync::OnceLock<SkillNode>,
}

impl<L: Language + Send + Sync> Pipeline<L>
where
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + Serialize
        + for<'de> Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
{
    pub fn new(
        language: L,
        llm_client: Box<dyn LlmClient>,
        generator_temperature: f32,
        generator_max_tokens: u32,
        extractor_temperature: f32,
        extractor_max_tokens: u32,
        prompt_config: PromptConfig,
    ) -> Self {
        Self {
            language,
            llm_client: tokio::sync::RwLock::new(llm_client),
            early_processors: Vec::new(),
            late_processors: Vec::new(),
            validators: Vec::new(),
            generator_temperature,
            generator_max_tokens,
            extractor_temperature,
            extractor_max_tokens,
            prompt_config,
            usage_recorder: None,
            cached_base_tree: std::sync::OnceLock::new(),
        }
    }

    /// Builds the Anki deck name from the tree path: `Language::TreePath::CardModel`
    fn build_deck_name(&self, tree_root: &SkillNode, node_id: &str, req: &GenerationRequest<L>) -> Result<String> {
        let tree_path = crate::skill_tree::get_node_path(tree_root, node_id)
            .ok_or_else(|| anyhow::anyhow!("Node path not found for '{}'", node_id))?;
        let deck_path = tree_path.replace(" > ", "::");
        Ok(format!("{}::{}", deck_path, req.card_model_id))
    }

    /// Generates cards and returns both display data (DynGeneratedCard) and storage
    /// data (NewDeckData) from a single `generate_cards_batch` call, avoiding double
    /// LLM invocations.
    pub async fn generate_cards_and_deck(
        &self,
        tree_root: &SkillNode,
        target_node_id: &str,
        req: &GenerationRequest<L>,
        llm_semaphore: Arc<Semaphore>,
        post_process_semaphore: Arc<Semaphore>,
    ) -> Result<(Vec<DynGeneratedCard>, NewDeckData)> {
        let _node = crate::skill_tree::find_node(tree_root, target_node_id)
            .ok_or_else(|| anyhow::anyhow!("Target node not found in skill tree"))?;

        let deck_name = self.build_deck_name(tree_root, target_node_id, req)?;
        let cards = self.generate_cards_batch(tree_root, req, target_node_id, llm_semaphore, post_process_semaphore).await?;

        let mut dyn_cards = Vec::with_capacity(cards.len());
        let mut new_cards = Vec::with_capacity(cards.len());

        for c in cards {
            let metadata_json = serde_json::to_string_pretty(&c.metadata).unwrap_or_default();
            dyn_cards.push(DynGeneratedCard {
                card_id: c.metadata.card_id.clone(),
                template_name: c.model.template_name().to_string(),
                fields: c.model.to_fields(),
                explanation: c.model.explanation(),
                metadata_json: metadata_json.clone(),
            });
            let metadata_json_compact = serde_json::to_string(&c.metadata).unwrap_or_default();
            let fields_json = serde_json::to_string(&c.model.to_fields()).unwrap_or_default();
            new_cards.push(NewCardEntry {
                front_html: c.model.front_html(),
                back_html: c.model.back_html(),
                skill_name: c.metadata.skill_name,
                template_name: c.model.template_name().to_string(),
                fields_json,
                explanation: lc_core::sanitize::escape_html(&c.metadata.pedagogical_explanation)
                    .replace('\n', "<br>"),
                ipa: c.metadata.ipa.unwrap_or_default(),
                metadata_json: metadata_json_compact,
                audio_path: c.metadata.audio_file,
            });
        }

        let language_code = self.language.iso_code().to_639_3().to_string();
        Ok((dyn_cards, NewDeckData { name: deck_name, language_code, cards: new_cards }))
    }

    /// Generates cards and aggregates them into a `NewDeckData` object ready to be
    /// exported or pushed to storage by the calling application layer.
    pub async fn generate_deck_data(
        &self,
        tree_root: &SkillNode,
        target_node_id: &str,
        req: &GenerationRequest<L>,
        llm_semaphore: Arc<Semaphore>,
        post_process_semaphore: Arc<Semaphore>,
    ) -> Result<NewDeckData> {
        let _node = crate::skill_tree::find_node(tree_root, target_node_id)
            .ok_or_else(|| anyhow::anyhow!("Target node not found in skill tree"))?;

        let deck_name = self.build_deck_name(tree_root, target_node_id, req)?;

        let cards = match self.generate_cards_batch(tree_root, req, target_node_id, llm_semaphore, post_process_semaphore).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(%e, "Failed to generate cards batch");
                Vec::new()
            }
        };

        let new_cards = cards.into_iter().map(|c| {
            let metadata_json = serde_json::to_string(&c.metadata).unwrap_or_default();
            let fields_json = serde_json::to_string(&c.model.to_fields()).unwrap_or_default();
            NewCardEntry {
                front_html: c.model.front_html(),
                back_html: c.model.back_html(),
                skill_name: c.metadata.skill_name,
                template_name: c.model.template_name().to_string(),
                fields_json,
                explanation: lc_core::sanitize::escape_html(&c.metadata.pedagogical_explanation)
                    .replace('\n', "<br>"),
                ipa: c.metadata.ipa.unwrap_or_default(),
                metadata_json,
                audio_path: c.metadata.audio_file,
            }
        }).collect();

        let language_code = self.language.iso_code().to_639_3().to_string();
        Ok(NewDeckData {
            name: deck_name,
            language_code,
            cards: new_cards,
        })
    }

    /// Builds the system prompt for the content generator from the skill tree context.
    fn build_generator_system_prompt(
        &self,
        tree_root: &SkillNode,
        req: &GenerationRequest<L>,
        skill_node_id: &str,
    ) -> Result<String> {
        let node = crate::skill_tree::find_node(tree_root, skill_node_id)
            .ok_or_else(|| anyhow::anyhow!("Node not found"))?;
        let path = crate::skill_tree::get_node_path(tree_root, skill_node_id)
            .ok_or_else(|| anyhow::anyhow!("Path not found"))?;

        Ok(GeneratorContext::builder()
            .language(&self.language)
            .skill_node(node)
            .node_path(&path)
            .request(req)
            .prompt_config(&self.prompt_config)
            .build()
            .generate_prompt()?)
    }

    /// Generates a batch of cards using the dual-call LLM strategy.
    pub async fn generate_cards_batch(
        &self,
        tree_root: &SkillNode,
        req: &GenerationRequest<L>,
        skill_node_id: &str,
        llm_semaphore: Arc<Semaphore>,
        post_process_semaphore: Arc<Semaphore>,
    ) -> Result<Vec<GeneratedCard<L::Morphology>>> {
        if req.num_cards == 0 {
            return Ok(Vec::new());
        }

        // --- Call 1: Content Generation (Batched) ---
        let system_content = self.build_generator_system_prompt(tree_root, req, skill_node_id)?;

        let user_content = format!(
            "Difficulty level: {}/10.{}\n\nRespond with valid JSON only, no markdown.",
            req.difficulty,
            req.user_prompt
                .as_deref()
                .map(|p| format!(" {}", p))
                .unwrap_or_default(),
        );

        let item_schema = crate::card_models::AnyCard::schema_json_value::<L>(req.card_model_id);
        // LLM structured outputs (Anthropic tool_use and OpenAI JSON schema) strictly require
        // the top-level schema to be an object, not an array. We wrap it in a `cards` property.
        let array_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "cards": {
                    "type": "array",
                    "items": item_schema,
                }
            },
            "required": ["cards"],
        });

        let content_request = LlmRequest {
            messages: vec![
                ChatMessage { role: Role::System, content: system_content },
                ChatMessage { role: Role::User, content: user_content },
            ],
            temperature: self.generator_temperature,
            max_tokens: Some(self.generator_max_tokens),
            response_schema: Some(array_schema),
            request_context: req.request_context.clone(),
            call_type: crate::llm_client::CallType::Generation,
        };

        let raw_json = {
            let _permit = llm_semaphore.acquire().await.unwrap();
            let client = self.llm_client.read().await;
            client.chat_completion(&content_request).await?.content
        };
        tracing::debug!(raw_len = raw_json.len(), "Raw LLM content output");

        let cleaned_json = clean_llm_json(&raw_json);

        tracing::debug!(cleaned_len = cleaned_json.len(), "Cleaned LLM content output");

        // --- Parse the wrapper object and extract the array of cards ---
        let mut parsed_root: serde_json::Value = serde_json::from_str(cleaned_json)?;
        let json_array: Vec<serde_json::Value> = if let Some(arr) = parsed_root.get_mut("cards").and_then(|v| v.as_array_mut()) {
            std::mem::take(arr)
        } else if cleaned_json.starts_with('[') {
             // Fallback just in case the LLM returned the array directly ignoring the schema wrapper
             serde_json::from_str(cleaned_json)?
        } else if req.num_cards == 1 {
             vec![parsed_root]
        } else {
             return Err(anyhow::anyhow!("LLM returned an object without 'cards' array"));
        };

        // Pre-resolve node info for use inside the async closures
        let skill_name = crate::skill_tree::find_node(tree_root, skill_node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let node_for_extraction = crate::skill_tree::find_node(tree_root, skill_node_id).cloned();
        let node_path_for_extraction = crate::skill_tree::get_node_path(tree_root, skill_node_id)
            .unwrap_or_default();

        let mut fetch_tasks = Vec::new();

        for value in json_array.into_iter() {
             let card_json_str = serde_json::to_string(&value)?;

             // --- Parse + Validate (with 1 retry) ---
             let parsed = match self.parse_and_validate(req, &card_json_str).await {
                 Ok(result) => result,
                 Err(feedback) => {
                     tracing::warn!(%feedback, "Card validation failed, retrying");
                     match self.retry_single_card(tree_root, req, skill_node_id, &feedback, &llm_semaphore).await {
                         Ok(result) => result,
                         Err(e) => {
                             tracing::error!(%e, "Card abandoned after retry");
                             continue;
                         }
                     }
                 }
             };

             let (any_card, extra_fields, card_json_for_extraction) = parsed;
             let llm_sem = llm_semaphore.clone();
             let pp_sem = post_process_semaphore.clone();
             let skill_name = skill_name.clone();
             let extraction_node = node_for_extraction.clone();
             let extraction_path = node_path_for_extraction.clone();

             let future = async move {
                 let card_id_str = uuid::Uuid::new_v4().to_string();
                 let extra_value = serde_json::to_value(&extra_fields).unwrap_or(serde_json::Value::Null);

                 // --- Parallel: Feature Extraction + Early Post-Processing ---
                 let (extracted, early_results) = tokio::join!(
                     // Feature extraction (LLM call 2, with 1 retry)
                      async {
                          let targets = any_card.targets().to_vec();
                          let extraction_node_ref = extraction_node.as_ref();
                          let Some(ext_node) = extraction_node_ref else {
                              return Err(anyhow::anyhow!("Node not found for extraction"));
                          };
                          let mut result = {
                              let _permit = llm_sem.acquire().await.unwrap();
                              let client = self.llm_client.read().await;
                              crate::feature_extractor::extract_features_via_llm(
                                  &self.language, ext_node, &extraction_path,
                                  client.as_ref(), req,
                                  &card_json_for_extraction, &targets,
                                  self.extractor_temperature, self.extractor_max_tokens,
                                  None, &self.prompt_config,
                              ).await
                          };
                          if let Err(ref e) = result {
                              // Build correction context from the parse error if available
                              let prev_attempt = e.downcast_ref::<crate::feature_extractor::ExtractionParseError>()
                                  .map(|pe| crate::feature_extractor::PreviousAttempt {
                                      raw_response: pe.raw_response.clone(),
                                      error: pe.error_message.clone(),
                                  });
                              tracing::warn!("Feature extraction first attempt failed, retrying with error feedback");
                              tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                              let _permit = llm_sem.acquire().await.unwrap();
                              let client = self.llm_client.read().await;
                              result = crate::feature_extractor::extract_features_via_llm(
                                  &self.language, ext_node, &extraction_path,
                                  client.as_ref(), req,
                                  &card_json_for_extraction, &targets,
                                  self.extractor_temperature, self.extractor_max_tokens,
                                  prev_attempt.as_ref(), &self.prompt_config,
                              ).await;
                          }
                          result
                      },
                     // Early post-processors (TTS, IPA — subprocesses)
                     async {
                         let _pp_permit = pp_sem.acquire().await.unwrap();
                         let speakable = any_card.speakable_text();
                         let input_chars = speakable.as_ref().map(|t| t.len() as u32).unwrap_or(0);
                         let lang_code = self.language.iso_code().to_639_3().to_string();

                         let mut results = Vec::new();
                         for processor in &self.early_processors {
                             let pp_start = std::time::Instant::now();
                             match processor.process(&self.language, &card_id_str, &any_card, &extra_value).await {
                                 Ok(r) => {
                                     if let (Some(recorder), Some(ctx)) = (&self.usage_recorder, &req.request_context) {
                                         let latency_ms = pp_start.elapsed().as_millis() as u64;
                                         if r.ipa.is_some() {
                                             recorder.record_post_process(PostProcessEvent {
                                                 user_id: ctx.user_id.clone(),
                                                 request_id: ctx.request_id.clone(),
                                                 language: Some(lang_code.clone()),
                                                 process_type: PostProcessType::Ipa,
                                                 input_chars,
                                                 latency_ms,
                                                 success: true,
                                             });
                                         }
                                         if r.audio_file.is_some() {
                                             recorder.record_post_process(PostProcessEvent {
                                                 user_id: ctx.user_id.clone(),
                                                 request_id: ctx.request_id.clone(),
                                                 language: Some(lang_code.clone()),
                                                 process_type: PostProcessType::Tts,
                                                 input_chars,
                                                 latency_ms,
                                                 success: true,
                                             });
                                         }
                                     }
                                     results.push(r);
                                 }
                                 Err(e) => tracing::error!(%e, "Early post-processing error"),
                             }
                         }
                         results
                     }
                 );

                 // Assemble metadata from feature extraction results
                 let (pedagogical_explanation, target_features, context_features, multiword_expressions) = match extracted {
                     Ok(resp) => (resp.pedagogical_explanation, resp.target_features, resp.context_features, resp.multiword_expressions),
                     Err(e) => {
                         tracing::error!(%e, "Feature extraction FAILED (even after retry) — explanation will be empty");
                         (String::new(), Vec::new(), Vec::new(), Vec::new())
                     },
                 };

                 let mut metadata = CardMetadata {
                     card_id: card_id_str,
                     language: self.language.iso_code().to_639_3().to_string(),
                     skill_id: skill_node_id.to_string(),
                     skill_name,
                     pedagogical_explanation,
                     target_features,
                     context_features,
                     multiword_expressions,
                     ipa: None,
                     audio_file: None,
                 };

                 // Merge early post-processing results
                 for r in early_results {
                     if r.ipa.is_some() { metadata.ipa = r.ipa; }
                     if r.audio_file.is_some() { metadata.audio_file = r.audio_file; }
                 }

                 // --- Late Post-Processing (sequential, after feature extraction) ---
                 for processor in &self.late_processors {
                     if let Err(e) = processor.process(&self.language, &any_card, &extra_value, &mut metadata).await {
                         tracing::error!(%e, "Late post-processing error");
                     }
                 }

                 GeneratedCard { model: any_card, metadata }
             };

             fetch_tasks.push(future);
        }

        let generated_cards = futures::future::join_all(fetch_tasks).await;

        Ok(generated_cards)
    }

    /// Parses a card JSON and runs all validators. Returns the parsed card or a feedback string.
    async fn parse_and_validate(
        &self,
        req: &GenerationRequest<L>,
        card_json: &str,
    ) -> std::result::Result<(crate::card_models::AnyCard, L::ExtraFields, String), String> {
        let (any_card, extra_fields) = crate::card_models::AnyCard::parse::<L>(req.card_model_id, card_json)
            .map_err(|e| format!("Parse error: {}", e))?;

        let extra_value = serde_json::to_value(&extra_fields).unwrap_or(serde_json::Value::Null);
        for validator in &self.validators {
            validator.validate(&self.language, &any_card, &extra_value).await?;
        }

        Ok((any_card, extra_fields, card_json.to_string()))
    }

    /// Retries generating a single card after validation failure.
    /// Injects the feedback message into the prompt so the LLM can correct itself.
    async fn retry_single_card(
        &self,
        tree_root: &SkillNode,
        req: &GenerationRequest<L>,
        skill_node_id: &str,
        feedback: &str,
        llm_semaphore: &Arc<Semaphore>,
    ) -> std::result::Result<(crate::card_models::AnyCard, L::ExtraFields, String), String> {
        let system_content = self.build_generator_system_prompt(tree_root, req, skill_node_id)
            .map_err(|e| format!("Prompt generation error: {}", e))?;

        let user_content = format!(
            "Difficulty level: {}/10.{}\n\n\
             IMPORTANT: A previous attempt failed with the following error:\n{}\n\
             Please generate exactly 1 valid card. Respond with valid JSON only, no markdown.",
            req.difficulty,
            req.user_prompt.as_deref().map(|p| format!(" {}", p)).unwrap_or_default(),
            feedback,
        );

        let item_schema = crate::card_models::AnyCard::schema_json_value::<L>(req.card_model_id);

        let retry_request = LlmRequest {
            messages: vec![
                ChatMessage { role: Role::System, content: system_content },
                ChatMessage { role: Role::User, content: user_content },
            ],
            temperature: self.generator_temperature,
            max_tokens: Some(self.generator_max_tokens),
            response_schema: Some(item_schema),
            request_context: req.request_context.clone(),
            call_type: crate::llm_client::CallType::Generation,
        };

        let raw_json = {
            let _permit = llm_semaphore.acquire().await.unwrap();
            let client = self.llm_client.read().await;
            client.chat_completion(&retry_request).await
                .map_err(|e| format!("Retry LLM call failed: {}", e))?
                .content
        };

        let cleaned = clean_llm_json(&raw_json);

        // Strip array wrapper if LLM returned [{ ... }] for a single card
        let card_json = if cleaned.starts_with('[') {
            let arr: Vec<serde_json::Value> = serde_json::from_str(cleaned)
                .map_err(|e| format!("Retry parse error (array): {}", e))?;
            let first = arr.into_iter().next()
                .ok_or_else(|| "Retry returned empty array".to_string())?;
            serde_json::to_string(&first)
                .map_err(|e| format!("Retry serialize error: {}", e))?
        } else {
            cleaned.to_string()
        };

        let (any_card, extra_fields) = crate::card_models::AnyCard::parse::<L>(req.card_model_id, &card_json)
            .map_err(|e| format!("Retry parse error: {}", e))?;

        // Run validators on the retried card (no further retry)
        let extra_value = serde_json::to_value(&extra_fields).unwrap_or(serde_json::Value::Null);
        for validator in &self.validators {
            validator.validate(&self.language, &any_card, &extra_value).await
                .map_err(|e| format!("Retry validation error: {}", e))?;
        }

        Ok((any_card, extra_fields, card_json))
    }

    /// Set the usage recorder for tracking post-processing operations.
    pub fn set_usage_recorder(&mut self, recorder: UsageRecorder) {
        self.usage_recorder = Some(recorder);
    }

    /// Add an early post-processor (runs in parallel with feature extraction).
    pub fn add_early_processor(&mut self, processor: Box<dyn EarlyPostProcessor<L>>) {
        self.early_processors.push(processor);
    }

    /// Add a late post-processor (runs after feature extraction, with full metadata access).
    pub fn add_late_processor(&mut self, processor: Box<dyn LatePostProcessor<L>>) {
        self.late_processors.push(processor);
    }

    /// Add a card validator (runs after parsing, before processing).
    pub fn add_validator(&mut self, validator: Box<dyn CardValidator<L>>) {
        self.validators.push(validator);
    }

    fn build_generation_request(
        &self,
        card_model_id: CardModelId,
        num_cards: u32,
        difficulty: u8,
        user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        lexicon: Option<&LexiconTracker<L::Morphology>>,
    ) -> GenerationRequest<L> {
        let mut injected_vocabulary = Vec::new();
        let mut excluded_vocabulary = Vec::new();

        if let (Some(opt), Some(tracker)) = (lexicon_options, lexicon) {
            let words = match opt.level {
                crate::generator::LexiconLevel::Known => {
                    match opt.pos_filter {
                        Some(ref pos) if pos != "All" => tracker.get_known_by_pos(pos),
                        _ => tracker.mastered_words(),
                    }
                }
                crate::generator::LexiconLevel::All => {
                    match opt.pos_filter {
                        Some(ref pos) if pos != "All" => tracker.get_all_by_pos(pos),
                        _ => tracker.get_all_words(),
                    }
                }
            };

            match opt.mode {
                crate::generator::LexiconMode::Include => injected_vocabulary = words,
                crate::generator::LexiconMode::Exclude => excluded_vocabulary = words,
            }
        }

        GenerationRequest {
            card_model_id,
            num_cards,
            difficulty,
            user_profile,
            user_prompt,
            transliteration: None,
            injected_vocabulary,
            excluded_vocabulary,
            request_context,
        }
    }
}

// ═══════════════════════════════════════════════
//  Type-erased pipeline trait (for the API layer)
// ═══════════════════════════════════════════════

/// A generated card with all language-specific types erased to strings/JSON.
pub struct DynGeneratedCard {
    pub card_id: String,
    pub template_name: String,
    pub fields: HashMap<String, String>,
    pub explanation: String,
    pub metadata_json: String,
}

/// Preview data for both LLM calls, with schemas.
pub struct DynPromptPreview {
    pub system_prompt_call_1: String,
    pub system_prompt_call_2: String,
    pub schema_call_1: serde_json::Value,
    pub schema_call_2: serde_json::Value,
}

/// Language-agnostic pipeline interface. The API layer works exclusively through this trait.
/// The tree is injected at each call — the pipeline is stateless w.r.t. the skill tree.
#[async_trait::async_trait]
pub trait DynPipeline: Send + Sync {
    fn language_name(&self) -> &str;
    fn iso_code_str(&self) -> &str;

    /// Returns the base skill tree for this language (cached after first call).
    fn base_tree(&self) -> SkillNode;

    async fn generate_cards_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<Vec<DynGeneratedCard>>;

    async fn generate_deck_data_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<NewDeckData>;

    /// Single LLM call that returns both display cards and storage-ready deck data.
    async fn generate_cards_and_deck_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<(Vec<DynGeneratedCard>, NewDeckData)>;

    fn preview_prompt_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        difficulty: u8, user_profile: UserSettings,
        lexicon_options: Option<crate::generator::LexiconOption>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<DynPromptPreview>;

    /// Factory: builds a type-erased lexicon tracker from a storage provider.
    async fn load_lexicon(&self, provider: &(dyn StorageProvider + Sync)) -> Result<Arc<dyn DynLexiconTracker>>;

    /// Hot-swap the LLM client (e.g. after changing provider/model at runtime).
    async fn swap_llm_client(&self, new_client: Box<dyn LlmClient>);

    fn available_models(&self) -> Vec<CardModelId>;
}

#[async_trait::async_trait]
impl<L> DynPipeline for Pipeline<L>
where
    L: Language + Send + Sync + 'static,
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + Serialize
        + for<'de> Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    L::ExtraFields: schemars::JsonSchema + Send + Sync,
{
    fn language_name(&self) -> &str {
        self.language.name()
    }

    fn iso_code_str(&self) -> &str {
        self.language.iso_code().to_639_3()
    }

    fn base_tree(&self) -> SkillNode {
        self.cached_base_tree.get_or_init(|| {
            let config = self.language.default_tree_config();
            crate::skill_tree::build_node(config.root)
        }).clone()
    }

    async fn generate_cards_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<Vec<DynGeneratedCard>> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, num_cards, difficulty, user_profile, user_prompt, lexicon_options, request_context, concrete);
        let cards = self.generate_cards_batch(tree_root, &req, node_id, llm_sem, pp_sem).await?;
        Ok(cards.into_iter().map(|c| {
            let metadata_json = serde_json::to_string_pretty(&c.metadata).unwrap_or_default();
            DynGeneratedCard {
                card_id: c.metadata.card_id,
                template_name: c.model.template_name().to_string(),
                fields: c.model.to_fields(),
                explanation: c.model.explanation(),
                metadata_json,
            }
        }).collect())
    }

    async fn generate_deck_data_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<NewDeckData> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, num_cards, difficulty, user_profile, user_prompt, lexicon_options, request_context, concrete);
        self.generate_deck_data(tree_root, node_id, &req, llm_sem, pp_sem).await
    }

    async fn generate_cards_and_deck_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_client::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<(Vec<DynGeneratedCard>, NewDeckData)> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, num_cards, difficulty, user_profile, user_prompt, lexicon_options, request_context, concrete);
        self.generate_cards_and_deck(tree_root, node_id, &req, llm_sem, pp_sem).await
    }

    fn preview_prompt_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        difficulty: u8, user_profile: UserSettings,
        lexicon_options: Option<crate::generator::LexiconOption>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<DynPromptPreview> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, 1, difficulty, user_profile, None, lexicon_options, None, concrete);

        let node = crate::skill_tree::find_node(tree_root, node_id)
            .ok_or_else(|| anyhow::anyhow!("Node '{}' not found", node_id))?;
        let path = crate::skill_tree::get_node_path(tree_root, node_id).unwrap_or_default();

        let system_prompt_call_1 = GeneratorContext::builder()
            .language(&self.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&self.prompt_config)
            .build()
            .generate_prompt()?;

        let system_prompt_call_2 = crate::prompts::FeatureExtractorContext::builder()
            .language(&self.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&self.prompt_config)
            .build()
            .generate_prompt()?;

        let schema_call_1 = crate::card_models::AnyCard::schema_json_value::<L>(card_model_id);
        let schema_call_2 = serde_json::to_value(
            &schemars::schema_for!(crate::feature_extractor::FeatureExtractionResponse<L::Morphology>)
        ).unwrap_or_default();

        Ok(DynPromptPreview {
            system_prompt_call_1,
            system_prompt_call_2,
            schema_call_1,
            schema_call_2,
        })
    }

    async fn load_lexicon(&self, provider: &(dyn StorageProvider + Sync)) -> Result<Arc<dyn DynLexiconTracker>> {
        let analyzer = LibraryAnalyzer;
        let lang = self.language.iso_code().to_639_3();
        let tracker = analyzer.extract_tracker_async::<L::Morphology>(provider, Some(lang)).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(Arc::new(tracker))
    }

    async fn swap_llm_client(&self, new_client: Box<dyn LlmClient>) {
        let mut client = self.llm_client.write().await;
        *client = new_client;
    }

    fn available_models(&self) -> Vec<CardModelId> {
        CardModelId::available_models(&self.language)
    }
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::MockLlmClient;
    use crate::skill_tree::{SkillNodeConfig, SkillTree};
    use langs::{Polish, PolishMorphology};
    use lc_core::domain::ExtractedFeature;

    fn sample_tree() -> SkillTree<Polish> {
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "Polski".to_string(),
            node_instructions: None,
            children: vec![SkillNodeConfig {
                id: "accusative".to_string(),
                name: "Biernik".to_string(),
                node_instructions: Some(
                    "Generate a Polish accusative cloze test as JSON.".to_string(),
                ),
                children: vec![],
            }],
        };
        SkillTree::new(Polish, config)
    }

    #[tokio::test]
    async fn pipeline_generates_cards_with_mock_llm() {
        let tree = sample_tree();
        let acc_id = "accusative";
        let prompts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("prompts");
        let config = crate::prompts::PromptConfig::load(prompts_path.to_str().unwrap()).expect("Failed to load prompts");

        let mock = MockLlmClient::new(vec![
            // Call 1: Content generation
            r#"{"cards": [{"sentence": "Czytam {{c1::książkę}}.", "targets": ["książkę"], "debug": "pipi caca debug", "translation": "I am reading a book."}]}"#.to_string(),
            // Call 2: Feature extraction (new morphology schema)
            r#"{"pedagogical_explanation": "Test", "target_features": [{"word": "książkę", "morphology": {"pos": "Noun", "lemma": "książka", "gender": "Feminine", "case": "Accusative"}}], "context_features": [{"word": "czytam", "morphology": {"pos": "Verb", "lemma": "czytać", "aspect": "Imperfective"}}]}"#.to_string(),
        ]);

        let pipeline = Pipeline::new(Polish, Box::new(mock), 0.8, 4000, 0.0, 4000, config);

        let req = GenerationRequest::<langs::Polish> {
            card_model_id: crate::card_models::CardModelId::ClozeTest,
            num_cards: 1,
            difficulty: 3,
            user_profile: lc_core::user::UserSettings::default(),
            user_prompt: None,
            transliteration: None,
            injected_vocabulary: vec![ExtractedFeature {
                word: "dom".to_string(),
                morphology: PolishMorphology::Noun {
                    lemma: "dom".to_string(),
                    gender: "Masculine".to_string(),
                    case: "Nominative".to_string(),
                },
            }],
            excluded_vocabulary: vec![],
            request_context: None,
        };

        let deck_data = pipeline
            .generate_deck_data(&tree.root, acc_id, &req, Arc::new(Semaphore::new(1)), Arc::new(Semaphore::new(1)))
            .await
            .unwrap();

        assert_eq!(deck_data.cards.len(), 1);
    }

    #[tokio::test]
    async fn pipeline_handles_llm_failure_gracefully() {
        let tree = sample_tree();
        let acc_id = "accusative";
        let prompts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("prompts");
        let config = crate::prompts::PromptConfig::load(prompts_path.to_str().unwrap()).expect("Failed to load prompts");

        let mock = MockLlmClient::with_fixed_response("not valid json at all");
        let pipeline = Pipeline::new(Polish, Box::new(mock), 0.8, 4000, 0.0, 4000, config);

        let req = GenerationRequest::<langs::Polish> {
            card_model_id: crate::card_models::CardModelId::ClozeTest,
            num_cards: 3,
            difficulty: 1,
            user_profile: lc_core::user::UserSettings::default(),
            user_prompt: None,
            transliteration: None,
            injected_vocabulary: vec![],
            excluded_vocabulary: vec![],
            request_context: None,
        };

        let deck_data = pipeline
            .generate_deck_data_dyn(&tree.root, acc_id, req.card_model_id, req.num_cards, req.difficulty, req.user_profile, req.user_prompt, None, None, Arc::new(Semaphore::new(1)), Arc::new(Semaphore::new(1)), None)
            .await
            .unwrap();

        assert_eq!(deck_data.cards.len(), 0);
    }

}
