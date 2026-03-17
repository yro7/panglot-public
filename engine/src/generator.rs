use lc_core::domain::ExtractedFeature;
use lc_core::traits::Language;
use serde::{Deserialize, Serialize};

use crate::card_models::CardModelId;

// ----- Generation Request -----

/// Parameters for a card generation run, chosen by the user at runtime.
pub struct GenerationRequest<L: Language> {
    pub card_model_id: CardModelId,
    pub num_cards: u32,
    pub difficulty: u8,
    pub user_profile: lc_core::user::UserSettings,
    pub user_prompt: Option<String>,
    /// Reserved for roadmap: transliteration script to request from the LLM.
    pub transliteration: Option<String>,
    pub injected_vocabulary: Vec<ExtractedFeature<L::Morphology>>,
    pub excluded_vocabulary: Vec<ExtractedFeature<L::Morphology>>,
}

// ----- Lexicon Options -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LexiconMode {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LexiconLevel {
    Known, // Only words that have been mastered
    All,   // All tracked words, even learning/struggling
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconOption {
    pub mode: LexiconMode,
    pub pos_filter: Option<String>, // e.g. "Verb", "Noun", None means "All"
    pub level: LexiconLevel,
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use langs::Polish;

    #[test]
    fn test_generation_request_creation() {
        let req = GenerationRequest::<Polish> {
            card_model_id: CardModelId::ClozeTest,
            num_cards: 5,
            difficulty: 3,
            user_profile: lc_core::user::UserSettings::new(
                "French".to_string(),
                lc_core::user::UserSettings::DEFAULT_SRS.to_string(),
                lc_core::user::UserSettings::DEFAULT_LEARN_AHEAD,
            ),
            user_prompt: None,
            transliteration: None,
            injected_vocabulary: vec![],
            excluded_vocabulary: vec![],
        };
        assert_eq!(req.card_model_id, CardModelId::ClozeTest);
    }
}
