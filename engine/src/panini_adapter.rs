//! Adapter layer between Panglot's LLM client and panini-engine's abstract LlmClient.

use anyhow::Result;
use async_trait::async_trait;

use crate::llm_client::{self, RequestContext, CallType};

/// Wraps a Panglot `dyn LlmClient` to implement panini-engine's `LlmClient` trait.
///
/// Carries optional billing/tracing context so extraction calls are properly tracked.
pub struct PaniniLlmAdapter<'a> {
    pub inner: &'a dyn llm_client::LlmClient,
    pub request_context: Option<RequestContext>,
}

#[async_trait]
impl panini_engine::LlmClient for PaniniLlmAdapter<'_> {
    async fn chat_completion(
        &self,
        request: &panini_engine::llm::LlmRequest,
    ) -> Result<panini_engine::llm::LlmResponse> {
        let messages = request
            .messages
            .iter()
            .map(|m| llm_client::ChatMessage {
                role: match m.role {
                    panini_engine::llm::Role::System => llm_client::Role::System,
                    panini_engine::llm::Role::User => llm_client::Role::User,
                    panini_engine::llm::Role::Assistant => llm_client::Role::Assistant,
                },
                content: m.content.clone(),
            })
            .collect();

        let panglot_request = llm_client::LlmRequest {
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_schema: request.response_schema.clone(),
            request_context: self.request_context.clone(),
            call_type: CallType::Extraction,
        };

        let response = self.inner.chat_completion(&panglot_request).await?;
        Ok(panini_engine::llm::LlmResponse {
            content: response.content,
        })
    }
}

/// Converts Panglot's `KnownLanguage` to panini-engine's `LanguageLevel`.
pub fn to_panini_language_levels(
    background: &[lc_core::user::KnownLanguage],
) -> Vec<panini_engine::prompts::LanguageLevel> {
    background
        .iter()
        .map(|lang| panini_engine::prompts::LanguageLevel {
            iso_639_3: lang.iso_639_3.clone(),
            level: format!("{:?}", lang.level),
        })
        .collect()
}
