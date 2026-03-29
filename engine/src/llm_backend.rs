use serde::{Deserialize, Serialize};
use rig::client::CompletionClient;
use std::fmt;
use std::str::FromStr;
use anyhow::Result;

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
    pub fn build(provider: LlmProvider, model: &str, api_key: &str, base_url: &str) -> anyhow::Result<Self> {
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
