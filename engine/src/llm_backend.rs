use serde::{Deserialize, Serialize};
use rig::client::CompletionClient;
use rig::completion::CompletionModel as _;
use std::fmt;
use std::str::FromStr;
use anyhow::Result;

use crate::usage::{LlmProviderUsageEvent, UsageRecorder};

// ── Provider Enum ──

/// Known LLM providers with built-in URL & API-key conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Google,
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Custom,
}

impl LlmProvider {
    /// Default base URL for this provider's chat-completions endpoint.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::Google    => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::OpenAi    => "https://api.openai.com/v1",
            Self::Custom    => "",
        }
    }

    /// Environment variable name that holds the API key for this provider.
    pub fn default_api_key_env(&self) -> &'static str {
        match self {
            Self::Google    => "GOOGLE_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi    => "OPENAI_API_KEY",
            Self::Custom    => "LLM_API_KEY",
        }
    }
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Google    => write!(f, "google"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::OpenAi    => write!(f, "openai"),
            Self::Custom    => write!(f, "custom"),
        }
    }
}

impl FromStr for LlmProvider {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "google" | "gemini"    => Ok(Self::Google),
            "anthropic" | "claude" => Ok(Self::Anthropic),
            "openai" | "gpt"      => Ok(Self::OpenAi),
            "custom"              => Ok(Self::Custom),
            other => Err(anyhow::anyhow!(
                "Unknown LLM provider '{}'. Expected: google, anthropic, openai, custom", other
            )),
        }
    }
}

// ── Context Types ──

#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    pub user_id: String,
    pub request_id: String,
    pub endpoint: String,
    pub language: Option<String>,
}

// ── Backend Enum ──

#[derive(Clone)]
pub enum LlmBackend {
    OpenAi(rig::providers::openai::responses_api::ResponsesCompletionModel),
    Anthropic(rig::providers::anthropic::completion::CompletionModel),
    Google(rig::providers::gemini::CompletionModel),
}

impl LlmBackend {
    /// Returns the provider name (e.g. "openai", "anthropic", "google").
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::OpenAi(_) => "openai",
            Self::Anthropic(_) => "anthropic",
            Self::Google(_) => "google",
        }
    }

    /// Returns the model name (e.g. "gpt-4o", "claude-sonnet-4-20250514").
    /// Reads the `pub model: String` field exposed by all Rig provider types.
    pub fn model_name(&self) -> &str {
        match self {
            Self::OpenAi(m) => &m.model,
            Self::Anthropic(m) => &m.model,
            Self::Google(m) => &m.model,
        }
    }

    /// Sends a structured-output completion request and returns the raw text response.
    /// Handles provider dispatch, usage recording, and error mapping internally.
    pub async fn execute_generation(
        &self,
        system_prompt: &str,
        user_content: &str,
        schema: schemars::Schema,
        temperature: f32,
        max_tokens: u32,
        call_type: &str,
        usage_recorder: Option<&UsageRecorder>,
        request_context: Option<&RequestContext>,
    ) -> Result<String> {
        let start = std::time::Instant::now();

        macro_rules! send_and_extract {
            ($m:expr) => {{
                let req_builder = $m.completion_request(user_content)
                    .preamble(system_prompt.to_string())
                    .temperature(temperature as f64)
                    .max_tokens(max_tokens as u64)
                    .output_schema(schema.clone());

                match req_builder.send().await {
                    Ok(response) => {
                        let tokens_in = response.usage.input_tokens as u32;
                        let tokens_out = response.usage.output_tokens as u32;
                        let text = response.choice.into_iter().find_map(|c| {
                            if let rig::completion::message::AssistantContent::Text(t) = c {
                                Some(t.text)
                            } else { None }
                        });
                        Ok((tokens_in, tokens_out, text))
                    }
                    Err(e) => Err(anyhow::anyhow!("LLM request failed: {}", e)),
                }
            }}
        }

        let (tokens_in, tokens_out, text) = match self {
            Self::OpenAi(m) => send_and_extract!(m),
            Self::Anthropic(m) => send_and_extract!(m),
            Self::Google(m) => send_and_extract!(m),
        }?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if let (Some(recorder), Some(ctx)) = (usage_recorder, request_context) {
            recorder.record_llm_call(LlmProviderUsageEvent {
                user_id: ctx.user_id.clone(),
                request_id: ctx.request_id.clone(),
                endpoint: ctx.endpoint.clone(),
                call_type: call_type.to_string(),
                provider: self.provider_name().to_string(),
                model: self.model_name().to_string(),
                language: ctx.language.clone(),
                tokens_in,
                tokens_out,
                latency_ms,
                is_error: false,
                error_message: None,
            });
        }

        text.ok_or_else(|| anyhow::anyhow!("LLM returned no text content"))
    }

    pub fn build(provider: LlmProvider, model: &str, api_key: &str, _base_url: &str) -> anyhow::Result<Self> {
        match provider {
            LlmProvider::OpenAi | LlmProvider::Custom => {
                let client = rig::providers::openai::Client::new(api_key)
                    .map_err(|e| anyhow::anyhow!("Failed to map OpenAI client: {}", e))?;
                Ok(Self::OpenAi(client.completion_model(model)))
            }
            LlmProvider::Anthropic => {
                let client = rig::providers::anthropic::Client::new(api_key)
                    .map_err(|e| anyhow::anyhow!("Failed to map Anthropic client: {}", e))?;
                Ok(Self::Anthropic(client.completion_model(model)))
            }
            LlmProvider::Google => {
                let client = rig::providers::gemini::Client::new(api_key)
                    .map_err(|e| anyhow::anyhow!("Failed to map Gemini client: {}", e))?;
                Ok(Self::Google(client.completion_model(model)))
            }
        }
    }
}
