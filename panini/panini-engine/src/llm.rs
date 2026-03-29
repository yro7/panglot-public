use anyhow::Result;
use async_trait::async_trait;

/// Messages for LLM interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Minimal LLM response for extraction.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
}

/// Minimal LLM request for extraction.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub response_schema: Option<serde_json::Value>,
}

/// Abstract LLM client trait for panini.
///
/// Panini does not depend on any specific LLM provider. Consumers implement this
/// trait to provide their own LLM client (e.g., wrapping reqwest, rig-core, etc.).
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat_completion(&self, request: &LlmRequest) -> Result<LlmResponse>;
}
