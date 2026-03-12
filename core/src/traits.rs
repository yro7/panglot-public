use std::collections::HashMap;
use std::fmt::{self, Debug, Display};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Common interface for language-specific morphology enums.
/// Allows generic code to extract the lemma and PoS label from any morphology variant.
pub trait MorphologyInfo {
    /// The dictionary form of the word.
    fn lemma(&self) -> &str;
    /// The part-of-speech label (e.g. "Noun", "Verb").
    fn pos_label(&self) -> &'static str;
}

/// Re-export `isolang::Language` as `IsoLang` so downstream crates don't need `isolang` directly.
pub use isolang::Language as IsoLang;

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

/// An ISO 15924 script code (e.g., "Latn", "Hira", "Kana").
/// Thin wrapper around a `&'static str` code. Use `.resolve()` to get full ISO data.
#[derive(Clone, Copy)]
pub struct Script(&'static str);

impl Script {
    pub const LATN: Script = Script("Latn");
    pub const CYRL: Script = Script("Cyrl");
    pub const HIRA: Script = Script("Hira");
    pub const KANA: Script = Script("Kana");
    pub const HANI: Script = Script("Hani");
    pub const ARAB: Script = Script("Arab");

    /// Returns the 4-character ISO 15924 code.
    pub fn code(&self) -> &'static str {
        self.0
    }

    /// Constructs a `Script` from any valid ISO 15924 code string.
    /// Returns `None` if the code is not in the standard.
    pub fn new(code: &str) -> Option<Self> {
        let entry = iso15924::ScriptCode::by_code(code)?;
        Some(Script(entry.code.as_ref()))
    }

    /// Looks up the full ISO 15924 data for this script.
    pub fn resolve(&self) -> &'static iso15924::ScriptCode<'static> {
        iso15924::ScriptCode::by_code(self.0)
            .unwrap_or_else(|| panic!("Invalid ISO 15924 script code: {}", self.0))
    }
}

impl Debug for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Script(\"{}\")", self.0)
    }
}

impl Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl PartialEq for Script {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Script {}

impl Hash for Script {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Serialize for Script {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for Script {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Script::new(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown ISO 15924 script code: {code}")))
    }
}

/// Typological features of a language that influence its behavior or available card models.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypologicalFeature {
    /// The language features verb conjugation (e.g. Polish, French, Spanish).
    /// Used to unlock specific fill-in-the-blank conjugation exercises.
    Conjugation,
}

/// Defines a Language with its morphological system and generation capabilities.
pub trait Language {
    /// The language-specific morphology enum. Each variant represents a PoS category
    /// with its morphological fields (lemma, case, gender, aspect, etc.).
    type Morphology: Debug + Clone + Serialize + for<'de> Deserialize<'de>
        + schemars::JsonSchema + MorphologyInfo + Send + Sync;

    /// The specific extra features this language requires the LLM to provide,
    /// defined as a statically typed struct for compile-time JSON Schema generation.
    type ExtraFields: schemars::JsonSchema + Debug + Clone + Serialize + for<'de> Deserialize<'de>;

    /// The ISO 639-3 language identity. Used to auto-derive `name()`.
    fn iso_code(&self) -> IsoLang;

    /// The English name of the language, auto-derived from `iso_code()`.
    fn name(&self) -> &str {
        self.iso_code().to_name()
    }

    /// The scripts supported by this language (e.g., Kanji, Hiragana for Japanese).
    fn supported_scripts(&self) -> &[Script];

    /// The default script for the language.
    fn default_script(&self) -> Script;

    /// Language-specific extraction directives for the feature extractor.
    fn extraction_directives(&self) -> &str;

    /// Specific typological features of this language that can unlock specific learning exercises.
    fn typological_features(&self) -> &[TypologicalFeature] {
        &[]
    }

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
}

/// Defines the type of card model.
/// Card models own their presentation: `front_html()` / `back_html()` produce
/// the HTML content that goes into the Anki card's front and back fields.
/// The DeckBuilder is fully generic and never matches on template_name.
pub trait CardModel {
    fn template_name(&self) -> &str;
    fn explanation(&self) -> String;
    fn to_fields(&self) -> HashMap<String, String>;
    /// Renders the HTML content for the card's front side.
    fn front_html(&self) -> String;
    /// Renders the HTML content for the card's back side (content only —
    /// IPA, audio, explanation, and metadata are appended by the DeckBuilder).
    fn back_html(&self) -> String;
}

/// Trait for converting a struct's fields into a HashMap<String, String>.
/// Use `#[derive(ToFields)]` (from lc_macro) to auto-implement this trait.
pub trait ToFields {
    fn to_fields(&self) -> HashMap<String, String>;
}

/// Derive macro for the `ToFields` trait. Generates `to_fields()` from struct fields.
/// Fields with `#[serde(flatten)]` are expanded recursively.
/// Field values must implement `IntoFieldString`.
pub use lc_macro::ToFields;

pub trait IntoFieldString {
    fn into_field_string(&self) -> Option<String>;
}

impl IntoFieldString for String {
    fn into_field_string(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl IntoFieldString for Option<String> {
    fn into_field_string(&self) -> Option<String> {
        self.clone()
    }
}

impl IntoFieldString for Vec<String> {
    fn into_field_string(&self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self.join(", "))
        }
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
