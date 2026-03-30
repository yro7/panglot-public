use serde::{Deserialize, Serialize};
use lc_core::traits::Language;
use lc_core::user::UserSettings;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::card_models::CardModelId;
use crate::generator::GenerationRequest;
use crate::llm_backend::LlmBackend;
use crate::post_process::{EarlyPostProcessor, LatePostProcessor};
use crate::prompts::{GeneratorContext, PromptConfig};
use crate::skill_tree::SkillNode;
use crate::usage::UsageRecorder;
use crate::validation::CardValidator;
use crate::llm_utils::clean_llm_json;
use crate::analyzer::LexiconTracker;

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
    pub(super) llm_backend: std::sync::RwLock<LlmBackend>,
    pub(super) early_processors: Vec<Box<dyn EarlyPostProcessor<L>>>,
    pub(super) late_processors: Vec<Box<dyn LatePostProcessor<L>>>,
    pub(super) validators: Vec<Box<dyn CardValidator<L>>>,
    pub(super) generator_temperature: f32,
    pub(super) generator_max_tokens: u32,
    pub(super) extractor_temperature: f32,
    pub(super) extractor_max_tokens: u32,
    pub(super) llm_call_timeout: std::time::Duration,
    pub(super) prompt_config: PromptConfig,
    pub(super) usage_recorder: Option<UsageRecorder>,
    pub(super) cached_base_tree: std::sync::OnceLock<SkillNode>,
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
    L::GrammaticalFunction: std::fmt::Debug
        + Clone
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
{
    pub fn new(
        language: L,
        llm_backend: LlmBackend,
        generator_temperature: f32,
        generator_max_tokens: u32,
        extractor_temperature: f32,
        extractor_max_tokens: u32,
        prompt_config: PromptConfig,
    ) -> Self {
        Self {
            language,
            llm_backend: std::sync::RwLock::new(llm_backend),
            early_processors: Vec::new(),
            late_processors: Vec::new(),
            validators: Vec::new(),
            generator_temperature,
            generator_max_tokens,
            extractor_temperature,
            extractor_max_tokens,
            llm_call_timeout: std::time::Duration::from_secs(120),
            prompt_config,
            usage_recorder: None,
            cached_base_tree: std::sync::OnceLock::new(),
        }
    }

    /// Builds the system prompt for the content generator from the skill tree context.
    pub(super) fn build_generator_system_prompt(
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

    /// Parses a card JSON and runs all validators. Returns the parsed card or a feedback string.
    pub(super) async fn parse_and_validate(
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
    pub(super) async fn retry_single_card(
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

        let raw_json = {
            let _permit = llm_semaphore.acquire().await
                .map_err(|_| "LLM semaphore closed".to_string())?;
            let backend = self.llm_backend.read()
                .map_err(|e| format!("LLM backend lock poisoned: {}", e))?.clone();
            let rig_schema: schemars::Schema = serde_json::from_value(item_schema).map_err(|e| e.to_string())?;

            tokio::time::timeout(self.llm_call_timeout, backend.execute_generation(
                &system_content, &user_content, rig_schema,
                self.generator_temperature, self.generator_max_tokens,
                "GenerationRetry",
                self.usage_recorder.as_ref(), req.request_context.as_ref(),
            )).await
                .map_err(|_| format!("LLM retry call timed out after {:?}", self.llm_call_timeout))?
                .map_err(|e| e.to_string())?
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

    pub(super) fn build_generation_request(
        &self,
        card_model_id: CardModelId,
        num_cards: u32,
        difficulty: u8,
        user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_backend::RequestContext>,
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
