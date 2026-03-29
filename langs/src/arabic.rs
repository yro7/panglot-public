pub use panini_langs::arabic::*;

use serde::{Deserialize, Serialize};
use lc_core::traits::{IpaConfig, Language, TtsConfig};

/// Arabic-specific extra fields for generation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArabicExtraFields {
    /// The fully vowelled text for disambiguation and phonetic processing.
    pub context_disambiguation: String,
}

/// Local wrapper — shadows the glob-imported `Arabic` from panini-langs.
/// Delegates linguistic definition to `panini_langs::arabic::Arabic` via composition.
pub struct Arabic;

impl Language for Arabic {
    type Morphology = ArabicMorphology;
    type GrammaticalFunction = ();
    type ExtraFields = ArabicExtraFields;
    type LinguisticDef = panini_langs::arabic::Arabic;

    fn linguistic_def(&self) -> &panini_langs::arabic::Arabic {
        &panini_langs::arabic::Arabic
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("Output Modern Standard Arabic without diacritics unless explicitly required. Ensure correct agreement for duals and reverse gender agreement for numbers 3-10.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("ara-Arab")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "ar-SA-HamedNeural" }
    }
}
