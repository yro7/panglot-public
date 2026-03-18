use serde::Deserialize;
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub generator: LlmCallConfig,
    pub feature_extractor: LlmCallConfig,
    pub defaults: DefaultsConfig,
    pub paths: PathsConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub enabled: bool,
}

impl Default for AuthConfig {
    fn default() -> Self { Self { enabled: false } }
}

impl AppConfig {
    pub fn auth_enabled(&self) -> bool { self.auth.enabled }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub static_path: String,
}

#[derive(Debug, Deserialize)]
pub struct LlmConfig {
    /// Provider name: "google", "anthropic", "openai", "custom"
    pub provider: String,
    /// Per-provider model lists. The first model of the chosen provider is used.
    pub models: HashMap<String, Vec<String>>,
    /// Optional override — only needed for custom provider or non-default URLs.
    pub base_url: Option<String>,
    /// Optional override — only needed for custom provider.
    pub api_key_env: Option<String>,
    pub retry: RetryConfig,
    pub concurrency: ConcurrencyConfig,
}

impl LlmConfig {
    /// Returns the first model from the chosen provider's list.
    pub fn active_model(&self) -> Result<&str> {
        let provider_key = self.provider.to_lowercase();
        let models = self.models.get(&provider_key)
            .ok_or_else(|| anyhow::anyhow!(
                "No models configured for provider '{}'. Add an entry under llm.models.{}",
                self.provider, provider_key
            ))?;
        models.first()
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "Model list for provider '{}' is empty", self.provider
            ))
    }
}

#[derive(Debug, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub delay_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmCallConfig {
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct ConcurrencyConfig {
    pub max_llm_calls: usize,
    pub max_post_process: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultsConfig {
    pub language: String,
    pub card_model: String,
    pub card_count_generate: u32,
    pub card_count_export: u32,
    pub difficulty: u8,
    pub user_language: String,
    #[serde(default)]
    pub user_settings: UserSettingsDefaultsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserSettingsDefaultsConfig {
    pub srs_algorithm: String,
    pub learn_ahead_minutes: i32,
}

impl Default for UserSettingsDefaultsConfig {
    fn default() -> Self {
        Self {
            srs_algorithm: "sm2".to_string(),
            learn_ahead_minutes: 20,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PathsConfig {
    pub output_dir: String,
    pub audio_staging_dir: String,
    /// URL to the AnkiConnect add-on running locally.
    /// Default: http://localhost:8765
    pub anki_connect_url: Option<String>,
}

pub fn load_config(path: &str) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file '{}': {}", path, e))?;
    let config: AppConfig = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file '{}': {}", path, e))?;
    Ok(config)
}

/// Resolves the AnkiConnect URL. If omitted, returns the default.
pub fn resolve_anki_connect_url(configured: Option<&str>) -> String {
    configured
        .unwrap_or("http://127.0.0.1:8765")
        .trim_end_matches('/')
        .to_string()
}
