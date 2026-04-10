//! Language registry for panini.
//!
//! Provides the type-erased extraction entry-point `extract_erased_with_components()`
//! which dispatches on an ISO code and runs the composable component pipeline.

use anyhow::{anyhow, Result};
use rig::completion::CompletionModel;

use panini_core::component::{AnalysisComponent, ExtractionResult};
use panini_core::components::*;
use panini_engine::{extract_with_components, ExtractionOptions, ExtractionRequest, PreviousAttempt};
use panini_engine::prompts::ExtractorPrompts;

use crate::{Arabic, French, Italian, Polish, Turkish};

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

    let options = ExtractionOptions {
        temperature,
        max_tokens,
        previous_attempt,
        extractor_prompts,
    };

    Ok(extract_with_components(
        lang,
        model,
        request,
        &selected,
        options,
    )
    .await?)
}

/// Macro to generate the registry functions for all languages.
/// Each language must be a unit struct implementing LinguisticDefinition.
macro_rules! generate_registry {
    ($($lang:ident),* $(,)?) => {
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
                $(
                    <$lang as panini_core::LinguisticDefinition>::ISO_CODE => {
                        extract_for_language(
                            &$lang,
                            model,
                            request,
                            component_keys,
                            temperature,
                            max_tokens,
                            previous_attempt,
                            extractor_prompts,
                        )
                        .await
                    }
                )*
                _ => Err(anyhow!("Unsupported language: {lang_code}")),
            }
        }

        /// Returns all supported ISO 639-3 language codes.
        pub fn supported_languages() -> &'static [&'static str] {
            &[$(<$lang as panini_core::LinguisticDefinition>::ISO_CODE),*]
        }
    };
}

// Generate the registry for all supported languages
generate_registry!(Polish, Turkish, Arabic, French, Italian);

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
