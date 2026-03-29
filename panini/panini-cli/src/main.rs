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
    },

    /// List supported language codes.
    Languages,
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
        Command::Extract {
            config: config_path,
            text,
            targets,
            temperature,
            max_tokens,
            ui_language,
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

            let result = run_extraction(&config, &prompts, &request, temperature, max_tokens)
                .await
                .context("Feature extraction failed")?;

            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Extraction dispatch: resolve provider → rig model → extract_erased
// ---------------------------------------------------------------------------

async fn run_extraction(
    config: &Config,
    prompts: &ExtractorPrompts,
    request: &ExtractionRequest,
    temperature: f32,
    max_tokens: u32,
) -> Result<serde_json::Value> {
    match config.provider {
        Provider::Openai => {
            let client = rig::providers::openai::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;
            let model = client.completion_model(&config.model);
            registry::extract_erased(
                &config.language,
                &model,
                request,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await
        }
        Provider::Anthropic => {
            let client = rig::providers::anthropic::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create Anthropic client: {e}"))?;
            let model = client.completion_model(&config.model);
            registry::extract_erased(
                &config.language,
                &model,
                request,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await
        }
        Provider::Google => {
            let client = rig::providers::gemini::Client::new(&config.api_key)
                .map_err(|e| anyhow::anyhow!("Failed to create Gemini client: {e}"))?;
            let model = client.completion_model(&config.model);
            registry::extract_erased(
                &config.language,
                &model,
                request,
                temperature,
                max_tokens,
                None,
                prompts,
            )
            .await
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
