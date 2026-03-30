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
    pub rate_limits: RateLimitConfig,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub enabled: bool,
}

impl Default for AuthConfig {
    fn default() -> Self { Self { enabled: false } }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub free: TierLimitConfig,
    pub premium: TierLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TierLimitConfig {
    pub daily_token_limit: i64,
    pub hourly_call_limit: i64,
    pub daily_tts_char_limit: i64,
}



impl AppConfig {
    pub fn auth_enabled(&self) -> bool { self.auth.enabled }

    pub fn validate(&self) -> Result<()> {
        // 1. Validate OpenAPI constraints using core types
        lc_core::validated::CardCount::new(self.defaults.card_count_generate)
            .map_err(|e| anyhow::anyhow!("defaults.card_count_generate is invalid: {}", e))?;
            
        lc_core::validated::CardCount::new(self.defaults.card_count_export)
            .map_err(|e| anyhow::anyhow!("defaults.card_count_export is invalid: {}", e))?;
            
        lc_core::validated::Difficulty::new(self.defaults.difficulty)
            .map_err(|e| anyhow::anyhow!("defaults.difficulty is invalid: {}", e))?;
            
        lc_core::validated::LearnAheadMinutes::new(self.defaults.user_settings.learn_ahead_minutes)
            .map_err(|e| anyhow::anyhow!("defaults.user_settings.learn_ahead_minutes is invalid: {}", e))?;

        // 2. Validate Server bounds
        if self.server.port == 0 {
            anyhow::bail!("server.port cannot be 0");
        }

        // 3. Validate LLM Generator Settings
        let validate_llm_call = |cfg: &LlmCallConfig, name: &str| -> Result<()> {
            if cfg.temperature < 0.0 || cfg.temperature > 2.0 {
                anyhow::bail!("{}.temperature must be between 0.0 and 2.0 (got {})", name, cfg.temperature);
            }
            if cfg.max_tokens == 0 {
                anyhow::bail!("{}.max_tokens must be greater than 0", name);
            }
            Ok(())
        };
        validate_llm_call(&self.generator, "generator")?;
        validate_llm_call(&self.feature_extractor, "feature_extractor")?;

        // 4. Validate Rate Limits
        let check_tier = |tier: &TierLimitConfig, tier_name: &str| -> Result<()> {
            if tier.daily_token_limit < 0 {
                anyhow::bail!("rate_limits.{}.daily_token_limit cannot be negative (got {})", tier_name, tier.daily_token_limit);
            }
            if tier.hourly_call_limit < 0 {
                anyhow::bail!("rate_limits.{}.hourly_call_limit cannot be negative (got {})", tier_name, tier.hourly_call_limit);
            }
            if tier.daily_tts_char_limit < 0 {
                anyhow::bail!("rate_limits.{}.daily_tts_char_limit cannot be negative (got {})", tier_name, tier.daily_tts_char_limit);
            }
            Ok(())
        };

        if self.rate_limits.enabled {
            check_tier(&self.rate_limits.free, "free")?;
            check_tier(&self.rate_limits.premium, "premium")?;
        }

        Ok(())
    }
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
    /// Timeout in seconds for each individual LLM call. Defaults to 120.
    #[serde(default = "default_llm_call_timeout_secs")]
    pub call_timeout_secs: u64,
}

fn default_llm_call_timeout_secs() -> u64 { 120 }

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
    #[allow(dead_code)]
    pub audio_staging_dir: String,
    /// URL to the AnkiConnect add-on running locally.
    /// Default: http://localhost:8765
    pub anki_connect_url: Option<String>,
}

pub fn load_config(path: &str) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file '{}': {}", path, e))?;
    let config: AppConfig = serde_yml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file '{}': {}", path, e))?;
    config.validate()?;
    Ok(config)
}

/// Resolves the AnkiConnect URL. If omitted, returns the default.
/// Resolves the AnkiConnect URL. If omitted, returns the default.
pub fn resolve_anki_connect_url(configured: Option<&str>) -> String {
    configured
        .unwrap_or("http://127.0.0.1:8765")
        .trim_end_matches('/')
        .to_string()
}
