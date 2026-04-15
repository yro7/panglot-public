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
    lc_core::import_from_panini!(panini_langs::arabic::Arabic);
    type ExtraFields = ArabicExtraFields;

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
