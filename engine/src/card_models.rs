use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use lc_core::sanitize::escape_html;
use lc_core::traits::CardModel;

// ----- Concrete Card Models -----
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[schemars(title = "CardModelSchema")]
#[schemars(bound = "L::ExtraFields: schemars::JsonSchema, M: schemars::JsonSchema")]
pub struct LLMResponse<M: schemars::JsonSchema, L: lc_core::traits::Language> 
where
    L::ExtraFields: schemars::JsonSchema,
{
    #[serde(flatten)]
    pub card: M,
    #[serde(flatten)]
    pub extra_features: L::ExtraFields,
}

// ----- Macro to automatically register all card models -----

macro_rules! define_card_models {
    ($($model:ident),+ $(,)?) => {
        /// Compile-time enum of all registered card model IDs.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum CardModelId {
            $($model,)+
            Conjugation,
        }

        impl CardModelId {
            // Note: ALL does not include Conjugation by default, it is feature-gated
            pub const ALL: &'static [CardModelId] = &[$(CardModelId::$model,)+];
        }

        impl std::fmt::Display for CardModelId {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(CardModelId::$model => write!(f, stringify!($model)),)+
                    CardModelId::Conjugation => write!(f, "Conjugation"),
                }
            }
        }

        impl std::str::FromStr for CardModelId {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $(stringify!($model) => Ok(CardModelId::$model),)+
                    "Conjugation" => Ok(CardModelId::Conjugation),
                    _ => Err(format!("Unknown card model id: '{}'", s)),
                }
            }
        }

        impl CardModelId {
             pub fn available_models<L: lc_core::traits::Language>(language: &L) -> Vec<CardModelId> {
                 use lc_core::traits::LinguisticDefinition;
                 let mut models = vec![
                     $(CardModelId::$model,)+
                 ];
                 let features = language.linguistic_def().typological_features();
                 if features.contains(&lc_core::traits::TypologicalFeature::Conjugation) {
                     models.push(CardModelId::Conjugation);
                 }
                 models
             }
        }

        /// An enum encompassing all concrete card models, ensuring 100% compile-time schema generation.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub enum AnyCard {
            $(
                $model($model),
            )+
        }

        impl AnyCard {
            /// Returns the full JSON Schema as a `serde_json::Value`. Single source of truth used
            /// both in prompt text (properties preview) and as `response_schema` for structured output.
            /// as `response_schema` to the LLM's structured-output mode.
            pub fn schema_json_value<L: lc_core::traits::Language>(id: CardModelId) -> serde_json::Value
            where
                L::ExtraFields: schemars::JsonSchema,
            {
                let schema = match id {
                    $(
                        CardModelId::$model => schemars::schema_for!(LLMResponse<$model, L>),
                    )+
                    CardModelId::Conjugation => schemars::schema_for!(LLMResponse<ClozeTest, L>),
                };
                serde_json::to_value(&schema).unwrap()
            }

            /// Parses the raw JSON response from the LLM back into a strongly typed `AnyCard` and the Language's `ExtraFields`.
            pub fn parse<L: lc_core::traits::Language>(id: CardModelId, json: &str) -> Result<(Self, L::ExtraFields), serde_json::Error> {
                match id {
                    $(
                        CardModelId::$model => {
                            let parsed: LLMResponse<$model, L> = serde_json::from_str(json)?;
                            Ok((AnyCard::$model(parsed.card), parsed.extra_features))
                        },
                    )+
                    CardModelId::Conjugation => {
                        let parsed: LLMResponse<ClozeTest, L> = serde_json::from_str(json)?;
                        Ok((AnyCard::ClozeTest(parsed.card), parsed.extra_features))
                    },
                }
            }
        }

        impl CardModel for AnyCard {
            fn template_name(&self) -> &str {
                match self {
                    $(AnyCard::$model(inner) => inner.template_name(),)+
                }
            }

            fn explanation(&self) -> String {
                match self {
                    $(AnyCard::$model(inner) => inner.explanation(),)+
                }
            }

            fn to_fields(&self) -> HashMap<String, String> {
                match self {
                    $(AnyCard::$model(inner) => inner.to_fields(),)+
                }
            }

            fn front_html(&self) -> String {
                match self {
                    $(AnyCard::$model(inner) => inner.front_html(),)+
                }
            }

            fn back_html(&self) -> String {
                match self {
                    $(AnyCard::$model(inner) => inner.back_html(),)+
                }
            }
        }
    };
}

// ----- Core Front-End Architecture -----

/// All the common front-end properties that can theoretically exist on ANY card model.
/// `schemars(skip)` is used for fields that are generated deterministically in a post-LLM step
/// (e.g. IPA tools, Transliteration engines) to ensure the LLM never tries to hallucinate them.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, lc_core::traits::ToFields)]
pub struct CommonCardFront {
    /// The translation of the text in the user's interface language.
    pub translation: String,

    /// The IPA transcription generated deterministically post-LLM by tools like Epitran.
    #[schemars(skip)]
    #[serde(default)]
    pub ipa: Option<String>,

    /// The romanized transcription generated deterministically post-LLM.
    #[schemars(skip)]
    #[serde(default)]
    pub transliteration: Option<String>,
}

/// Cloze deletion test — a sentence with blanked-out target words.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, lc_core::traits::ToFields)]
pub struct ClozeTest {
    /// The sentence in the target language with a word or phrase blanked out automatically with {{c1::word}}.
    pub sentence: String,
    /// The exact target word(s) missing from the sentence.
    pub targets: Vec<String>,

    /// An optional hint for the user : ex the root of the verb for conjugation, etc. Do not fill if you don't get instructions for it.
    #[serde(default)] // Vide par défaut si le LLM ne le génère pas
    pub hint: Option<String>,

    #[serde(flatten)]
    pub common: CommonCardFront,


}

impl CardModel for ClozeTest {
    fn template_name(&self) -> &str {
        "cloze_test"
    }

    fn explanation(&self) -> String {
        format!(
            "Fill in the blank(s): {} → {}",
            self.targets.join(", "),
            self.common.translation
        )
    }

    fn to_fields(&self) -> HashMap<String, String> {
        lc_core::traits::ToFields::to_fields(self)
    }

    fn front_html(&self) -> String {
        let cloze_prompt = replace_cloze_with_blank(&self.sentence);
        let mut html = format!(
            "<div class=\"translation\">{}</div>\n<div class=\"cloze-sentence\">{}</div>",
            escape_html(&self.common.translation), escape_html(&cloze_prompt)
        );
        if let Some(hint) = &self.hint {
            html.push_str(&format!("\n<div class=\"hint\">(Racine: {})</div>", escape_html(hint)));
        }
        html
    }

    fn back_html(&self) -> String {
        let full_sentence = strip_cloze_tags(&self.sentence);
        format!(
            "<div class=\"translation\">{}</div>\n<div class=\"full-sentence\">{}</div>",
            escape_html(&self.common.translation), escape_html(&full_sentence)
        )
    }
}

/// Written comprehension — read a text and translate.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, lc_core::traits::ToFields)]
pub struct WrittenComprehension {
    /// The text prompt to be read by the user in the target language.
    pub text_prompt: String,
    /// The text transcription of the prompt.
    pub transcript: String,
    /// The key target word(s) this card is testing the learner on.
    pub targets: Vec<String>,
    #[serde(flatten)]
    pub common: CommonCardFront,
}

impl CardModel for WrittenComprehension {
    fn template_name(&self) -> &str {
        "written_comprehension"
    }

    fn explanation(&self) -> String {
        format!("Read and understand: {}", self.text_prompt)
    }

    fn to_fields(&self) -> HashMap<String, String> {
        lc_core::traits::ToFields::to_fields(self)
    }

    fn front_html(&self) -> String {
        format!("<div class=\"text-prompt\">{}</div>", escape_html(&self.text_prompt))
    }

    fn back_html(&self) -> String {
        format!(
            "<div class=\"transcript\">{}</div>\n<div class=\"translation\">{}</div>",
            escape_html(&self.transcript), escape_html(&self.common.translation)
        )
    }
}

/// Oral comprehension — listen to audio and translate.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, lc_core::traits::ToFields)]
pub struct OralComprehension {
    /// The generated filename identifier for the spoken audio (can just be a placeholder like `audio.mp3`).
    pub audio_media: String,
    /// The transcript of the spoken audio in the target language.
    pub transcript: String,
    /// The key target word(s) this card is testing the learner on.
    pub targets: Vec<String>,
    #[serde(flatten)]
    pub common: CommonCardFront,
}

impl CardModel for OralComprehension {
    fn template_name(&self) -> &str {
        "oral_comprehension"
    }

    fn explanation(&self) -> String {
        format!("Listen and understand: [audio: {}]", self.audio_media)
    }

    fn to_fields(&self) -> HashMap<String, String> {
        lc_core::traits::ToFields::to_fields(self)
    }

    fn front_html(&self) -> String {
        // Front is just a prompt to listen — audio tag is appended by DeckBuilder
        "<div class=\"listen-prompt\">Listen and translate</div>".to_string()
    }

    fn back_html(&self) -> String {
        format!(
            "<div class=\"transcript\">{}</div>\n<div class=\"translation\">{}</div>",
            escape_html(&self.transcript), escape_html(&self.common.translation)
        )
    }
}

// ----- REGISTER MODELS -----
define_card_models!(
    ClozeTest,
    WrittenComprehension,
    OralComprehension
);

// ----- AnyCard Helpers -----

impl AnyCard {
    /// Returns the target words for this card.
    pub fn targets(&self) -> &[String] {
        match self {
            AnyCard::ClozeTest(c) => &c.targets,
            AnyCard::WrittenComprehension(c) => &c.targets,
            AnyCard::OralComprehension(c) => &c.targets,
        }
    }

    /// Returns the speakable text for post-processing (TTS/IPA).
    /// OralComprehension returns None since it already has audio.
    pub fn speakable_text(&self) -> Option<String> {
        match self {
            AnyCard::ClozeTest(c) => Some(strip_cloze_tags(&c.sentence)),
            AnyCard::WrittenComprehension(c) => Some(c.text_prompt.clone()),
            AnyCard::OralComprehension(_) => None,
        }
    }
}

/// Replaces cloze deletion tags like `{{c1::word}}` with `[...]` for the card front.
pub fn replace_cloze_with_blank(sentence: &str) -> String {
    let mut result = String::with_capacity(sentence.len());
    let mut chars = sentence.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next();
            // Skip until "}}"
            while let Some(c) = chars.next() {
                if c == '}' && chars.peek() == Some(&'}') {
                    chars.next();
                    break;
                }
            }
            result.push_str("[...]");
        } else {
            result.push(ch);
        }
    }
    result
}

/// Strips Anki cloze deletion tags like `{{c1::word}}` to extract the plain text.
pub fn strip_cloze_tags(sentence: &str) -> String {
    let mut result = String::with_capacity(sentence.len());
    let mut chars = sentence.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // skip second '{'
            // Skip until "::"
            let mut found_colons = false;
            while let Some(c) = chars.next() {
                if c == ':' && chars.peek() == Some(&':') {
                    chars.next(); // skip second ':'
                    found_colons = true;
                    break;
                }
            }
            if found_colons {
                // Collect content until "}}"
                while let Some(c) = chars.next() {
                    if c == '}' && chars.peek() == Some(&'}') {
                        chars.next(); // skip second '}'
                        break;
                    }
                    result.push(c);
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use langs::Polish;

    #[test]
    fn cloze_test_to_fields() {
        let card = ClozeTest {
            sentence: "Czytam {{c1::książkę}}.".to_string(),
            targets: vec!["książkę".to_string()],
            hint: Some("książka".to_string()),
            common: CommonCardFront {
                translation: "I am reading a book.".to_string(),
                ipa: Some("IPA string".to_string()),
                transliteration: None, // No transliteration for polish
            }
        };
        assert_eq!(card.template_name(), "cloze_test");
        let fields = card.to_fields();
        assert_eq!(fields.get("sentence").unwrap(), "Czytam {{c1::książkę}}.");
        assert_eq!(fields.get("translation").unwrap(), "I am reading a book.");
        assert_eq!(fields.get("hint").unwrap(), "książka");
        assert_eq!(fields.get("ipa").unwrap(), "IPA string");
        assert!(fields.get("transliteration").is_none());
    }

    #[test]
    fn anycard_schema_generation() {
        let schema_value = AnyCard::schema_json_value::<Polish>(CardModelId::ClozeTest);
        let schema = serde_json::to_string_pretty(&schema_value).unwrap();
        eprintln!("RAW SCHEMA\n{}", schema);
        assert!(schema.contains("sentence"));
        assert!(schema.contains("targets"));
        assert!(schema.contains("hint"));
        assert!(schema.contains("translation"));
        assert!(!schema.contains("ipa")); // ipa is skipped by schemars
        assert!(!schema.contains("transliteration")); // translit skipped by schemars
        assert!(!schema.contains("disambiguation"));
    }

    #[test]
    fn anycard_schema_generation_conjugation() {
        let schema_value = AnyCard::schema_json_value::<Polish>(CardModelId::Conjugation);
        let schema = serde_json::to_string_pretty(&schema_value).unwrap();
        assert!(schema.contains("sentence"));
        assert!(schema.contains("targets"));
        assert!(schema.contains("hint"));
        assert!(schema.contains("translation"));
    }

    #[test]
    fn anycard_parsing() {
        let json = r#"{"sentence": "Dom", "targets": ["Dom"], "translation": "House"}"#;
        let (card, _extra) = AnyCard::parse::<Polish>(CardModelId::ClozeTest, json).unwrap();
        match card {
            AnyCard::ClozeTest(c) => {
                assert_eq!(c.sentence, "Dom");
                assert_eq!(c.common.translation, "House");
                assert!(c.hint.is_none()); // Defaults to None since not provided in JSON
                assert!(c.common.ipa.is_none());
                assert!(c.common.transliteration.is_none());
            },
            _ => panic!("Parsed wrong enum variant"),
        }
    }
}
