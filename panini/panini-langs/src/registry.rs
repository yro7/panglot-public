//! Language registry for panini.
//!
//! Provides a type-erased extraction entry-point: `extract_erased()`.
//! Because `LinguisticDefinition` has associated types (non object-safe), each
//! language arm is resolved monomorphically and the result is serialized to
//! `serde_json::Value` to erase the concrete types.

use anyhow::{anyhow, Result};
use rig::completion::CompletionModel;
use serde_json;

use panini_engine::{extract_features_via_llm, ExtractionRequest, PreviousAttempt};
use panini_engine::prompts::ExtractorPrompts;

use crate::{Arabic, Polish, Turkish};

/// Extracts morphological features for any supported language, returning
/// the result serialized as a `serde_json::Value`.
///
/// This is the main entry-point for the CLI and any consumer that doesn't
/// want to deal with Panini's generic associated types.
pub async fn extract_erased<M: CompletionModel>(
    lang_code: &str,
    model: &M,
    request: &ExtractionRequest,
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    extractor_prompts: &ExtractorPrompts,
) -> Result<serde_json::Value> {
    match lang_code {
        "pol" => {
            let result = extract_features_via_llm(
                &Polish,
                model,
                request,
                temperature,
                max_tokens,
                previous_attempt,
                extractor_prompts,
            )
            .await?;
            Ok(serde_json::to_value(&result)?)
        }
        "tur" => {
            let result = extract_features_via_llm(
                &Turkish,
                model,
                request,
                temperature,
                max_tokens,
                previous_attempt,
                extractor_prompts,
            )
            .await?;
            Ok(serde_json::to_value(&result)?)
        }
        "ara" => {
            let result = extract_features_via_llm(
                &Arabic,
                model,
                request,
                temperature,
                max_tokens,
                previous_attempt,
                extractor_prompts,
            )
            .await?;
            Ok(serde_json::to_value(&result)?)
        }
        _ => Err(anyhow!("Unsupported language: {lang_code}")),
    }
}

/// Returns all supported ISO 639-3 language codes.
pub fn supported_languages() -> &'static [&'static str] {
    &["pol", "tur", "ara"]
}
