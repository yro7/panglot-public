use serde::{Deserialize, Serialize};
use anyhow::Result;
use lc_core::traits::Language;

use crate::generator::GenerationRequest;
use crate::llm_client::{ChatMessage, LlmClient, LlmRequest, Role};
use crate::llm_utils::clean_llm_json;
use crate::prompts::{FeatureExtractorContext, PromptConfig};
use crate::skill_tree::SkillNode;

/// Error returned when feature extraction parsing fails, carrying the raw LLM output.
#[derive(Debug)]
pub struct ExtractionParseError {
    pub raw_response: String,
    pub error_message: String,
}

impl std::fmt::Display for ExtractionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_message)
    }
}

impl std::error::Error for ExtractionParseError {}

pub use lc_core::morpheme::FeatureExtractionResponse;

/// Previous failed attempt context for LLM self-correction retry.
pub struct PreviousAttempt {
    pub raw_response: String,
    pub error: String,
}

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
    prompt_config: &PromptConfig,
) -> Result<FeatureExtractionResponse<L::Morphology, L::GrammaticalFunction>>
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
    let system_prompt = FeatureExtractorContext::builder()
        .language(language)
        .skill_node(node)
        .node_path(node_path)
        .request(req)
        .prompt_config(prompt_config)
        .build()
        .generate_prompt()?;

    let schema = language.build_extraction_schema();

    let mut messages = vec![
        ChatMessage {
            role: Role::System,
            content: system_prompt,
        },
        ChatMessage {
            role: Role::User,
            content: format!(
                "Extract features from this card:\n{}\n\nTARGET WORDS: {:?}",
                card_json, targets
            ),
        },
    ];

    if let Some(prev) = previous_attempt {
        messages.push(ChatMessage {
            role: Role::Assistant,
            content: prev.raw_response.clone(),
        });
        messages.push(ChatMessage {
            role: Role::User,
            content: format!(
                "Your output is not conform to what I'm expecting. Please look at the error and correct yourself: {}",
                prev.error
            ),
        });
    }

    let request = LlmRequest {
        messages,
        temperature,
        max_tokens: Some(max_tokens),
        response_schema: Some(schema),
        request_context: req.request_context.clone(),
        call_type: crate::llm_client::CallType::Extraction,
    };

    let response = llm_client.chat_completion(&request).await?.content;
    let cleaned = clean_llm_json(&response);
    let normalized = crate::llm_utils::normalize_pos_tags(cleaned);

    let mut response_parsed: FeatureExtractionResponse<L::Morphology, L::GrammaticalFunction> = match serde_json::from_str(&normalized) {
        Ok(parsed) => parsed,
        Err(e) => {
            let err_msg = e.to_string();
            tracing::warn!(error = %err_msg, "Failed to parse feature extraction response");
            return Err(ExtractionParseError {
                raw_response: normalized,
                error_message: err_msg,
            }.into());
        }
    };

    // Run language-specific post-processing (morpheme validation for agglutinative languages).
    if let Err(e) = language.post_process_extraction(&mut response_parsed.morpheme_segmentation) {
        tracing::warn!(error = %e, "Morpheme post-processing failed — retrying");
        return Err(ExtractionParseError {
            raw_response: normalized,
            error_message: e,
        }.into());
    }

    Ok(response_parsed)
}
