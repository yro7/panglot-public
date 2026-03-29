pub use panini_langs::turkish::*;

use lc_core::traits::{IpaConfig, Language, NoExtraFields, TtsConfig};

/// Local wrapper — shadows the glob-imported `Turkish` from panini-langs.
/// Delegates linguistic definition to `panini_langs::turkish::Turkish` via composition.
pub struct Turkish;

impl Language for Turkish {
    type Morphology = TurkishMorphology;
    type GrammaticalFunction = TurkishGrammaticalFunction;
    type ExtraFields = NoExtraFields;
    type LinguisticDef = panini_langs::turkish::Turkish;

    fn linguistic_def(&self) -> &panini_langs::turkish::Turkish {
        &panini_langs::turkish::Turkish
    }

    fn generation_directives(&self) -> Option<&str> {
        Some("When generating Turkish text, strictly adhere to vowel harmony rules for all suffixes. Ensure correct agreement in case, number, person, tense, mood, and voice. Turkish is a pro-drop language; omit subject pronouns when contextually clear.")
    }

    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::Epitran("tur-Latn")
    }

    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::Edge { voice: "tr-TR-AhmetNeural" }
    }
}
