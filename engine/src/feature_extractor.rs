//! Feature extraction — delegates to panini-engine.
//!
//! This module provides a Panglot-flavored wrapper around panini-engine's
//! `extract_features_via_llm`. It adapts Panglot's `GenerationRequest` and
//! `LlmClient` into panini's abstract types.

use anyhow::Result;
use lc_core::traits::Language;

use crate::generator::GenerationRequest;
use crate::llm_client::LlmClient;
use crate::panini_adapter::{PaniniLlmAdapter, to_panini_language_levels};
use crate::skill_tree::SkillNode;

// Re-export panini-engine types under the old names for backward compatibility.
pub use panini_engine::extractor::{ExtractionParseError, PreviousAttempt};
pub use panini_core::morpheme::FeatureExtractionResponse;

/// Call 2: extracts morphological features from a generated card's JSON.
///
/// Accepts the resolved `node` and `node_path` directly so this function
/// is agnostic to whether the tree was customized or not.
pub async fn extract_features_via_llm<L: Language + Send + Sync>(
    language: &L,
    node: &SkillNode,
    node_path: &str,
    llm_client: &dyn LlmClient,
    req: &GenerationRequest<L>,
    card_json: &str,
    targets: &[String],
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    prompt_config: &crate::prompts::PromptConfig,
) -> Result<FeatureExtractionResponse<L::Morphology, L::GrammaticalFunction>>
where
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    L::GrammaticalFunction: std::fmt::Debug
        + Clone
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
{
    // Adapt Panglot's LLM client to panini's abstract interface.
    let adapter = PaniniLlmAdapter {
        inner: llm_client,
        request_context: req.request_context.clone(),
    };

    // Convert Panglot's GenerationRequest context into panini's ExtractionRequest.
    let extraction_request = panini_engine::prompts::ExtractionRequest {
        content: card_json.to_string(),
        targets: targets.to_vec(),
        pedagogical_context: node.node_instructions.clone(),
        skill_path: Some(node_path.to_string()),
        learner_ui_language: req.user_profile.ui_language.clone(),
        linguistic_background: to_panini_language_levels(&req.user_profile.linguistic_background),
        user_prompt: req.user_prompt.clone(),
    };

    // Delegate to panini-engine. prompt_config.extractor is already the panini type.
    panini_engine::extract_features_via_llm(
        language.linguistic_def(),
        &adapter,
        &extraction_request,
        temperature,
        max_tokens,
        previous_attempt,
        &prompt_config.extractor,
    )
    .await
}
