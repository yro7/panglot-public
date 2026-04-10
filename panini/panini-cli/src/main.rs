mod add_language;
mod config;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{Config, Provider};
use panini_engine::prompts::{ExtractionRequest, ExtractorPrompts};
use panini_langs::registry;
use rig::client::CompletionClient;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "panini",
    about = "Morphological feature extraction via LLM",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract morphological features for one or more target words.
    Extract {
        /// Path to the TOML configuration file.
        #[arg(long, default_value = "panini.toml")]
        config: String,

        /// The text / flashcard content to analyse.
        #[arg(long)]
        text: String,

        /// Target word(s) to focus extraction on (repeatable).
        #[arg(long = "target", num_args = 1..)]
        targets: Vec<String>,

        /// Sampling temperature (0.0–2.0).
        #[arg(long, default_value_t = 0.2)]
        temperature: f32,

        /// Maximum tokens for the LLM response.
        #[arg(long, default_value_t = 4096)]
        max_tokens: u32,

        /// Optional learner UI language name (e.g. "French").
        #[arg(long, default_value = "English")]
        ui_language: String,

        /// Comma-separated list of components to extract (e.g. "pedagogical_explanation,morphology").
        /// When omitted, all compatible components are extracted.
        #[arg(long)]
        components: Option<String>,
    },

    /// List supported language codes.
    Languages,

    /// Generate a new language implementation using an LLM.
    AddLanguage {
        /// Path to the TOML configuration file (for provider/model/api_key).
        #[arg(long, default_value = "panini.toml")]
        config: String,

        /// The language to generate (English name, e.g. "Japanese", "Finnish").
        #[arg(long)]
        language: String,

        /// ISO 639-3 code (e.g. "jpn", "fin"). If omitted, you must provide it.
        #[arg(long)]
        iso_code: Option<String>,

        /// Whether the language is agglutinative (triggers Agglutinative trait generation).
        #[arg(long, default_value_t = false)]
        agglutinative: bool,

        /// Sampling temperature for generation.
        #[arg(long, default_value_t = 0.3)]
        temperature: f32,

        /// Skip the cargo check validation step.
        #[arg(long, default_value_t = false)]
        skip_check: bool,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Languages => {
            for code in registry::supported_languages() {
                println!("{code}");
            }
        }
        Command::AddLanguage {
            config: config_path,
            language,
            iso_code,
            agglutinative,
            temperature,
            skip_check,
        } => {
            let config = Config::load(&config_path)
                .with_context(|| format!("Loading config from '{config_path}'"))?;

            add_language::run(
                &config,
                &language,
                iso_code.as_deref(),
                agglutinative,
                temperature,
                skip_check,
            )
            .await?;
        }
        Command::Extract {
            config: config_path,
            text,
            targets,
            temperature,
            max_tokens,
            ui_language,
            components,
        } => {
            let config = Config::load(&config_path)
                .with_context(|| format!("Loading config from '{config_path}'"))?;

            let prompts = load_prompts(&config)?;

            let request = ExtractionRequest {
                content: text,
                targets,
                pedagogical_context: None,
                skill_path: None,
                learner_ui_language: ui_language,
                linguistic_background: vec![],
                user_prompt: None,
            };

            let keys: Option<Vec<&str>> = components
                .as_ref()
                .map(|s| s.split(',').map(|k| k.trim()).collect());
            let result = run_component_extraction(
                &config,
                &prompts,
                &request,
                keys.as_deref(),
                temperature,
                max_tokens,
            )
            .await
            .context("Feature extraction failed")?;

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Extraction dispatch: resolve provider → rig model → extract_erased_with_components
// ---------------------------------------------------------------------------

async fn run_component_extraction(
    config: &Config,
    prompts: &ExtractorPrompts,
    request: &ExtractionRequest,
    component_keys: Option<&[&str]>,
    temperature: f32,
    max_tokens: u32,
) -> Result<serde_json::Value> {
    match config.provider {
        Provider::Openai => {
            let client = rig::providers::openai::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;
            let model = client.completion_model(&config.model);
            let result = registry::extract_erased_with_components(
                &config.language,
                &model,
                request,
                component_keys,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await?;
            Ok(result.into_raw())
        }
        Provider::Anthropic => {
            let client = rig::providers::anthropic::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create Anthropic client: {e}"))?;
            let model = client.completion_model(&config.model);
            let result = registry::extract_erased_with_components(
                &config.language,
                &model,
                request,
                component_keys,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await?;
            Ok(result.into_raw())
        }
        Provider::Google => {
            let client = rig::providers::gemini::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create Gemini client: {e}"))?;
            let model = client.completion_model(&config.model);
            let result = registry::extract_erased_with_components(
                &config.language,
                &model,
                request,
                component_keys,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await?;
            Ok(result.into_raw())
        }
    }
}

// ---------------------------------------------------------------------------
// Prompts loading
// ---------------------------------------------------------------------------

/// Load ExtractorPrompts — from a custom file in config, or panic with a
/// clear message if none is provided (the default prompts file is user-supplied).
fn load_prompts(config: &Config) -> Result<ExtractorPrompts> {
    let path = config.prompts_file.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No 'prompts_file' specified in config. \
             Add `prompts_file = \"path/to/prompts.yml\"` to your TOML config."
        )
    })?;

    ExtractorPrompts::load(path)
        .with_context(|| format!("Failed to load prompts from '{path}'"))
}
