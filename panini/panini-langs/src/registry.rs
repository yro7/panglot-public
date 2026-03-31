//! Language registry for panini.
//!
//! Provides type-erased extraction entry-points: `extract_erased()` (legacy)
//! and `extract_erased_with_components()` (composable).

use anyhow::{anyhow, Result};
use rig::completion::CompletionModel;

use panini_core::component::{AnalysisComponent, ExtractionResult};
use panini_core::components::*;
use panini_engine::{extract_features_via_llm, extract_with_components, ExtractionRequest, PreviousAttempt};
use panini_engine::prompts::ExtractorPrompts;

use crate::{Arabic, French, Polish, Turkish};

/// Extracts morphological features for any supported language, returning
/// the result serialized as a `serde_json::Value`.
///
/// This is the legacy entry-point — returns the monolithic `FeatureExtractionResponse`.
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
        "fra" => {
            let result = extract_features_via_llm(
                &French,
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

/// Extracts features using composable components for any supported language.
///
/// `component_keys` selects which analyses to include (e.g. `["pedagogical_explanation", "morphology"]`).
/// If `None`, all compatible components are used.
pub async fn extract_erased_with_components<M: CompletionModel>(
    lang_code: &str,
    model: &M,
    request: &ExtractionRequest,
    component_keys: Option<&[&str]>,
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    extractor_prompts: &ExtractorPrompts,
) -> Result<ExtractionResult> {
    match lang_code {
        "pol" => {
            extract_for_language(&Polish, model, request, component_keys, temperature, max_tokens, previous_attempt, extractor_prompts).await
        }
        "tur" => {
            extract_for_language(&Turkish, model, request, component_keys, temperature, max_tokens, previous_attempt, extractor_prompts).await
        }
        "ara" => {
            extract_for_language(&Arabic, model, request, component_keys, temperature, max_tokens, previous_attempt, extractor_prompts).await
        }
        "fra" => {
            extract_for_language(&French, model, request, component_keys, temperature, max_tokens, previous_attempt, extractor_prompts).await
        }
        _ => Err(anyhow!("Unsupported language: {lang_code}")),
    }
}

/// Helper: build the component list for a concrete language and dispatch.
async fn extract_for_language<L, M>(
    lang: &L,
    model: &M,
    request: &ExtractionRequest,
    component_keys: Option<&[&str]>,
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    extractor_prompts: &ExtractorPrompts,
) -> Result<ExtractionResult>
where
    L: panini_core::LinguisticDefinition + Send + Sync,
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + schemars::JsonSchema
        + panini_core::MorphologyInfo
        + Send
        + Sync,
    L::GrammaticalFunction: std::fmt::Debug
        + Clone
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    M: CompletionModel,
{
    let pedagogical = PedagogicalExplanation;
    let morphology = MorphologyAnalysis;
    let multiword = MultiwordExpressions;
    let morpheme_seg = MorphemeSegmentation;
    let leipzig = LeipzigAlignment;

    let all_components: Vec<(&str, &dyn AnalysisComponent<L>)> = vec![
        ("pedagogical_explanation", &pedagogical),
        ("morphology", &morphology),
        ("multiword_expressions", &multiword),
        ("morpheme_segmentation", &morpheme_seg),
        ("leipzig_alignment", &leipzig),
    ];

    let selected: Vec<&dyn AnalysisComponent<L>> = match component_keys {
        Some(keys) => all_components
            .iter()
            .filter(|(k, _)| keys.contains(k))
            .map(|(_, c)| *c)
            .collect(),
        None => all_components.iter().map(|(_, c)| *c).collect(),
    };

    extract_with_components(
        lang,
        model,
        request,
        &selected,
        temperature,
        max_tokens,
        previous_attempt,
        extractor_prompts,
    )
    .await
}

/// Returns all supported ISO 639-3 language codes.
pub fn supported_languages() -> &'static [&'static str] {
    &["pol", "tur", "ara", "fra"]
}

#[cfg(test)]
mod tests {
    use panini_core::component::AnalysisComponent;
    use panini_core::components::*;
    use crate::{Polish, Turkish, Arabic};

    #[test]
    fn morpheme_segmentation_compatible_with_turkish() {
        let comp = MorphemeSegmentation;
        assert!(comp.is_compatible(&Turkish));
    }

    #[test]
    fn morpheme_segmentation_incompatible_with_polish() {
        let comp = MorphemeSegmentation;
        assert!(!comp.is_compatible(&Polish));
    }

    #[test]
    fn morpheme_segmentation_incompatible_with_arabic() {
        let comp = MorphemeSegmentation;
        assert!(!comp.is_compatible(&Arabic));
    }

    #[test]
    fn all_other_components_universal() {
        let ped = PedagogicalExplanation;
        let morph = MorphologyAnalysis;
        let multi = MultiwordExpressions;

        assert!(ped.is_compatible(&Turkish));
        assert!(morph.is_compatible(&Turkish));
        assert!(multi.is_compatible(&Turkish));

        assert!(ped.is_compatible(&Polish));
        assert!(morph.is_compatible(&Polish));
        assert!(multi.is_compatible(&Polish));

        assert!(ped.is_compatible(&Arabic));
        assert!(morph.is_compatible(&Arabic));
        assert!(multi.is_compatible(&Arabic));
    }
}
