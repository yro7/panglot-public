use std::time::Duration;
use panini_core::component::{AnalysisComponent, ExtractionResult};
use panini_core::traits::LinguisticDefinition;
use rig::completion::{CompletionModel, CompletionRequestBuilder};
use rig::message::Message;

use crate::composer::{compose_schema, compose_prompt};
use crate::llm_utils::clean_llm_json;
use crate::prompts::{ExtractorPrompts, ExtractionRequest};

// ─── Error types ──────────────────────────────────────────────────────────────

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

/// Typed error enum for the extraction pipeline.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// LLM provider errors (rig-core completion failures, network, auth, etc.)
    #[error("LLM completion failed: {0}")]
    Llm(#[from] rig::completion::request::CompletionError),

    /// JSON serialization/deserialization errors (schema conversion, response parsing)
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Prompt composition errors (missing placeholders, I/O, etc.)
    #[error("prompt composition failed: {0}")]
    PromptComposition(#[from] crate::prompts::PromptBuilderError),

    /// LLM returned no text content in its response
    #[error("LLM returned no text content")]
    EmptyResponse,

    /// Schema validation or component validation/parse failure — carries the raw
    /// LLM output so callers can retry with `PreviousAttempt`
    #[error("{0}")]
    Parse(#[from] ExtractionParseError),

    /// Failed to map raw `ExtractionResult` into a typed consumer struct
    /// (used by `#[derive(PaniniResult)]` generated code)
    #[error("failed to map extracted components to result struct")]
    ResultMapping(#[from] panini_core::component::ExtractionResultError),
}

// ─── Extraction options ───────────────────────────────────────────────────────

/// Previous failed attempt context for LLM self-correction retry.
struct PreviousAttempt {
    pub raw_response: String,
    pub error: String,
}

/// Configuration for the retry mechanism
#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_backoff_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            initial_backoff_secs: 1,
        }
    }
}

/// Bundles extraction parameters
#[derive(Clone)]
pub struct ExtractionOptions<'a> {
    pub temperature: f32,
    pub max_tokens: u32,
    pub extractor_prompts: &'a ExtractorPrompts,
    pub retry: RetryConfig,
    pub timeout: Duration,
}

impl<'a> ExtractionOptions<'a> {
    pub fn new(extractor_prompts: &'a ExtractorPrompts) -> Self {
        Self {
            temperature: 0.2,
            max_tokens: 4096,
            extractor_prompts,
            retry: RetryConfig::default(),
            timeout: Duration::from_secs(30),
        }
    }
}

// ─── Composable entry point ───────────────────────────────────────────────────

/// Extracts features using composable `AnalysisComponent`s.
///
/// This is the entry-point that supports selecting which analyses to include.
/// It includes an automatic self-correction loop (Retry) in case of validation errors.
pub async fn extract_with_components<L, M>(
    language: &L,
    model: &M,
    request: &ExtractionRequest,
    components: &[&dyn AnalysisComponent<L>],
    options: ExtractionOptions<'_>,
) -> Result<ExtractionResult, ExtractionError>
where
    L: LinguisticDefinition + Send + Sync,
    M: CompletionModel,
{
    let mut prev_attempt: Option<PreviousAttempt> = None;
    let mut backoff = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_secs(options.retry.initial_backoff_secs))
        .with_multiplier(2.0)
        .with_max_elapsed_time(Some(options.timeout))
        .build();

    loop {
        let result = perform_single_shot_extraction(
            language,
            model,
            request,
            components,
            &options,
            prev_attempt.as_ref(),
        )
        .await;

        match result {
            Ok(res) => return Ok(res),
            Err(e) => {
                // Only retry on parsing/validation errors
                if let ExtractionError::Parse(pe) = &e {
                    if let Some(wait) = backoff::backoff::Backoff::next_backoff(&mut backoff) {
                        tracing::warn!(
                            ?wait,
                            error = %pe.error_message,
                            "Extraction validation failed, retrying with self-correction..."
                        );
                        prev_attempt = Some(PreviousAttempt {
                            raw_response: pe.raw_response.clone(),
                            error: pe.error_message.clone(),
                        });
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }
                return Err(e);
            }
        }
    }
}

/// Internal function to perform a single extraction attempt.
async fn perform_single_shot_extraction<L, M>(
    language: &L,
    model: &M,
    request: &ExtractionRequest,
    components: &[&dyn AnalysisComponent<L>],
    options: &ExtractionOptions<'_>,
    previous_attempt: Option<&PreviousAttempt>,
) -> Result<ExtractionResult, ExtractionError>
where
    L: LinguisticDefinition + Send + Sync,
    M: CompletionModel,
{
    // 1. Filter to compatible components
    let compatible: Vec<&dyn AnalysisComponent<L>> = components
        .iter()
        .filter(|c| c.is_compatible(language))
        .copied()
        .collect();

    let requested_keys: Vec<&'static str> = compatible.iter().map(|c| c.schema_key()).collect();

    // 2. Compose schema
    let schema_value = compose_schema(language, &compatible);
    let rig_schema: schemars::Schema = serde_json::from_value(schema_value.clone())?;

    // 3. Compose prompt
    let system_prompt = compose_prompt(language, request, options.extractor_prompts, &compatible)?;

    let user_message = format!(
        "Extract features from this card:\n{}\n\nTARGET WORDS: {:?}",
        request.content, request.targets
    );

    // 4. Build LLM request
    let mut builder: CompletionRequestBuilder<M> = model
        .completion_request(user_message.as_str())
        .preamble(system_prompt)
        .temperature(options.temperature as f64)
        .max_tokens(options.max_tokens as u64)
        .output_schema(rig_schema);

    if let Some(prev) = previous_attempt {
        builder = builder
            .message(Message::assistant(&prev.raw_response))
            .message(Message::user(format!(
                "Your output is not conform to what I'm expecting. \
                 Please look at the error and correct yourself: {}",
                prev.error
            )));
    }

    let completion_response = builder.send().await?;

    let raw_text = completion_response
        .choice
        .into_iter()
        .find_map(|c| {
            if let rig::completion::message::AssistantContent::Text(t) = c {
                Some(t.text)
            } else {
                None
            }
        })
        .ok_or(ExtractionError::EmptyResponse)?;

    // 5. Chain pre_process from each component
    let cleaned = clean_llm_json(&raw_text);
    let mut processed = cleaned.to_string();
    for comp in &compatible {
        processed = comp.pre_process(&processed);
    }

    // 6. Parse JSON
    let mut json_value: serde_json::Value = match serde_json::from_str(&processed) {
        Ok(v) => v,
        Err(e) => {
            let err_msg = format!("Invalid JSON syntax: {}", e);
            tracing::warn!(error = %err_msg, "Failed to parse JSON syntax");
            return Err(ExtractionParseError {
                raw_response: processed,
                error_message: err_msg,
            }
            .into());
        }
    };

    // 7. Validate composed schema
    if let Ok(validator) = jsonschema::validator_for(&schema_value) {
        let schema_errors: Vec<_> = validator.iter_errors(&json_value).collect();
        if !schema_errors.is_empty() {
            let mut err_msgs = Vec::new();
            for err in schema_errors {
                err_msgs.push(format!("- Path: {}: {}", err.instance_path(), err));
            }
            let err_msg = format!(
                "Schema validation failed with {} errors:\n{}",
                err_msgs.len(),
                err_msgs.join("\n")
            );
            tracing::warn!(error = %err_msg, "Schema validation failed — retrying");
            return Err(ExtractionParseError {
                raw_response: processed,
                error_message: err_msg,
            }
            .into());
        }
    }

    // 8. Per-component validate + post_process
    for comp in &compatible {
        let key = comp.schema_key();
        if let Some(section) = json_value.get(key) {
            comp.validate(language, section).map_err(|e| {
                ExtractionParseError {
                    raw_response: processed.clone(),
                    error_message: format!("Validation failed for component '{}': {}", key, e),
                }
            })?;
        }
    }

    for comp in &compatible {
        let key = comp.schema_key();
        if let Some(section) = json_value.get_mut(key) {
            comp.post_process(language, section).map_err(|e| {
                ExtractionParseError {
                    raw_response: processed.clone(),
                    error_message: format!("Post-processing failed for component '{}': {}", key, e),
                }
            })?;
        }
    }

    // 9. Return ExtractionResult
    Ok(ExtractionResult::new(json_value, requested_keys))
}
