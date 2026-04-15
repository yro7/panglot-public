use std::collections::HashMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

// Re-export everything from panini-core::traits
pub use panini_core::traits::*;

/// Empty extra fields for languages that do not require any.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoExtraFields {}

/// Defines the IPA generation strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpaConfig {
    Epitran(&'static str), // Language code for Epitran (e.g., "pol-Latn")
    Custom(&'static str),  // Placeholder for other specific tools
    None,
}

/// Defines the TTS generation strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtsConfig {
    Edge { voice: &'static str },
    None,
}

/// Imports a language definition from Panini. 
#[macro_export]
macro_rules! import_from_panini {
    ($panini_path:path) => {
        type Morphology = <$panini_path as $crate::traits::LinguisticDefinition>::Morphology;
        type GrammaticalFunction = <$panini_path as $crate::traits::LinguisticDefinition>::GrammaticalFunction;
        type LinguisticDef = $panini_path;

        fn linguistic_def(&self) -> &Self::LinguisticDef {
            &$panini_path
        }
    };
}

/// Defines a Language with its morphological system and generation capabilities.
///
/// Uses composition to access the linguistic definition (from panini-core)
/// rather than inheriting from it. Panglot-specific concerns (IPA, TTS,
/// generation directives) live here; extraction concerns live in `LinguisticDef`.
pub trait Language {
    /// The language's morphological enum (POS-tagged, with `#[derive(MorphologyInfo)]`).
    type Morphology: Debug + Clone + Serialize + for<'de> Deserialize<'de>
        + schemars::JsonSchema + MorphologyInfo + Send + Sync;

    /// For agglutinative languages: the grammatical function enum. `()` otherwise.
    type GrammaticalFunction: Debug + Clone + PartialEq
        + Serialize + for<'de> Deserialize<'de>
        + schemars::JsonSchema + Send + Sync;

    /// The specific extra features this language requires the LLM to provide,
    /// defined as a statically typed struct for compile-time JSON Schema generation.
    type ExtraFields: schemars::JsonSchema + Debug + Clone + Serialize + for<'de> Deserialize<'de>;

    /// The concrete panini-core `LinguisticDefinition` backing this language.
    type LinguisticDef: LinguisticDefinition<
        Morphology = Self::Morphology,
        GrammaticalFunction = Self::GrammaticalFunction,
    > + Send + Sync;

    /// Access the panini linguistic definition — the only surface of contact with Panini.
    fn linguistic_def(&self) -> &Self::LinguisticDef;

    /// Optional language-specific global instructions for the LLM Generator.
    fn generation_directives(&self) -> Option<&str> {
        None
    }

    /// Define the IPA generation strategy for this language.
    fn ipa_strategy(&self) -> IpaConfig {
        IpaConfig::None
    }

    /// Define the TTS generation strategy for this language.
    fn tts_strategy(&self) -> TtsConfig {
        TtsConfig::None
    }

    /// The default skill tree configuration for this language, embedded at compile time.
    fn default_tree_config(&self) -> crate::skill_tree::SkillTreeConfig {
        crate::skill_tree::resolve_config(self.linguistic_def().iso_code().to_639_3())
    }
}

/// Defines the type of card model.
pub trait CardModel {
    fn template_name(&self) -> &str;
    fn explanation(&self) -> String;
    fn to_fields(&self) -> HashMap<String, String>;
    fn front_html(&self) -> String;
    fn back_html(&self) -> String;
}

/// Trait for converting a struct's fields into a `HashMap<String, String>`.
pub trait ToFields {
    fn to_fields(&self) -> HashMap<String, String>;
}

/// Derive macro for the `ToFields` trait.
pub use lc_macro::ToFields;

pub trait ToFieldString {
    fn to_field_string(&self) -> Option<String>;
}

impl ToFieldString for String {
    fn to_field_string(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl ToFieldString for Vec<String> {
    fn to_field_string(&self) -> Option<String> {
        Some(self.join(", "))
    }
}

impl ToFieldString for Option<String> {
    fn to_field_string(&self) -> Option<String> {
        self.clone()
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_constants_resolve_to_valid_iso15924() {
        let scripts = [Script::LATN, Script::CYRL, Script::HIRA, Script::KANA, Script::HANI];
        for script in &scripts {
            let resolved = script.resolve();
            assert_eq!(resolved.code.as_ref(), script.code());
        }
    }

    #[test]
    fn script_new_validates_iso15924() {
        assert!(Script::new("Arab").is_some());
        assert_eq!(Script::new("Arab").unwrap().code(), "Arab");
        assert_eq!(Script::new("Latn").unwrap(), Script::LATN);
        assert!(Script::new("XXXX").is_none());
        assert!(Script::new("latn").is_none()); // case-sensitive
    }

    #[test]
    fn script_serde_roundtrip() {
        let script = Script::LATN;
        let json = serde_json::to_string(&script).unwrap();
        assert_eq!(json, "\"Latn\"");
        let deserialized: Script = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, script);
    }
}
