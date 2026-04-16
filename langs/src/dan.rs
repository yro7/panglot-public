pub use panini_langs::danish::*;

use lc_core::traits::{IpaConfig, Language, NoExtraFields, TtsConfig};

/// Local wrapper — shadows the glob-imported `Danish` from panini-langs.
/// Delegates linguistic definition to `panini_langs::danish::Danish` via composition.
pub struct Danish;

impl Language for Danish {
    lc_core::import_from_panini!(panini_langs::danish::Danish);
    type ExtraFields = NoExtraFields;

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Danish text, ensure correct agreement for the two grammatical genders: common (n-word) and neuter (t-word). Strictly follow the V2 word order rule in main clauses. Distinguish correctly between the suffixed definite article (e.g., 'huset') and the standalone definite article used with adjectives (e.g., 'det store hus'). Use standard Danish orthography, including the letters æ, ø, and å.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::None
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge {
            voice: "da-DK-ChristelNeural",
        }
    }
}