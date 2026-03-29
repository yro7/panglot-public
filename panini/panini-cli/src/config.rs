use serde::Deserialize;

/// Top-level configuration loaded from a TOML file.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// LLM provider: "openai" or "anthropic"
    pub provider: Provider,
    /// Model identifier (e.g. "gpt-4o", "claude-3-5-sonnet-20241022")
    pub model: String,
    /// ISO 639-3 language code (e.g. "pol", "tur", "ara")
    pub language: String,
    /// API key — can be a plain string or "${ENV_VAR}" to read from the environment.
    pub api_key: String,
    /// Optional custom base URL for the provider's API endpoint.
    pub base_url: Option<String>,
    /// Optional path to a custom prompts YAML file.
    pub prompts_file: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Openai,
    Anthropic,
    Google,
}

impl Config {
    /// Load and parse a TOML config file.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file '{}': {}", path, e))?;
        let mut config: Config = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file '{}': {}", path, e))?;

        // Resolve ${ENV_VAR} references in api_key.
        config.api_key = resolve_env_var(&config.api_key)?;

        Ok(config)
    }
}

/// If `value` matches `${VAR_NAME}`, return the environment variable.
/// Otherwise return the value unchanged.
fn resolve_env_var(value: &str) -> anyhow::Result<String> {
    if let Some(var_name) = value
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix('}'))
    {
        std::env::var(var_name).map_err(|_| {
            anyhow::anyhow!(
                "Environment variable '{}' referenced in config is not set",
                var_name
            )
        })
    } else {
        Ok(value.to_owned())
    }
}
