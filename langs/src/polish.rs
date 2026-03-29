pub use panini_langs::polish::*;

use lc_core::traits::{IpaConfig, Language, NoExtraFields, TtsConfig};

/// Local wrapper — shadows the glob-imported `Polish` from panini-langs.
/// Delegates linguistic definition to `panini_langs::polish::Polish` via composition.
pub struct Polish;

impl Language for Polish {
    type Morphology = PolishMorphology;
    type GrammaticalFunction = ();
    type ExtraFields = NoExtraFields;
    type LinguisticDef = panini_langs::polish::Polish;

    fn linguistic_def(&self) -> &panini_langs::polish::Polish {
        &panini_langs::polish::Polish
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Polish text, use standard Polish orthography with correct diacritics. Polish is a pro-drop language; omit subject pronouns when they are contextually clear.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("pol-Latn")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "pl-PL-ZofiaNeural" }
    }
}
