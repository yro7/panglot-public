use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use lc_core::domain::CardMetadata;
use lc_core::storage::{NewDeckData, NewCardEntry};

/// Status of async lexicon loading.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LexiconStatus {
    NotStarted,
    Loading,
    Ready { word_count: usize },
    Failed { error: String },
}

/// A single generated card with its model and metadata.
pub struct GeneratedCard<P: std::fmt::Debug + Clone + Serialize + for<'de> Deserialize<'de>, F = ()> {
    pub model: crate::card_models::AnyCard,
    pub metadata: CardMetadata<P, F>,
}

/// Configuration for LLM call parameters (temperatures, token limits, timeouts).
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub generator_temperature: f32,
    pub generator_max_tokens: u32,
    pub extractor_temperature: f32,
    pub extractor_max_tokens: u32,
    /// Timeout for each individual LLM call (generation, retry, extraction).
    pub llm_call_timeout: std::time::Duration,
}

/// A generated card with all language-specific types erased to strings/JSON.
pub struct DynGeneratedCard {
    pub card_id: String,
    pub template_name: String,
    pub fields: HashMap<String, String>,
    pub explanation: String,
    pub metadata_json: String,
    // Storage-ready fields (used by cards_to_deck_data)
    pub front_html: String,
    pub back_html: String,
    pub skill_name: String,
    pub ipa: String,
    pub audio_path: Option<String>,
}

/// Converts a list of `DynGeneratedCard` into a `NewDeckData` ready for storage/export.
pub fn cards_to_deck_data(
    cards: &[DynGeneratedCard],
    deck_name: String,
    language_code: String,
) -> NewDeckData {
    let new_cards = cards.iter().map(|c| {
        let fields_json = serde_json::to_string(&c.fields).unwrap_or_default();
        NewCardEntry {
            front_html: c.front_html.clone(),
            back_html: c.back_html.clone(),
            skill_name: c.skill_name.clone(),
            template_name: c.template_name.clone(),
            fields_json,
            explanation: c.explanation.clone(),
            ipa: c.ipa.clone(),
            metadata_json: c.metadata_json.clone(),
            audio_path: c.audio_path.clone(),
        }
    }).collect();
    NewDeckData { name: deck_name, language_code, cards: new_cards }
}

/// Preview data for both LLM calls, with schemas.
pub struct DynPromptPreview {
    pub system_prompt_call_1: String,
    pub system_prompt_call_2: String,
    pub schema_call_1: serde_json::Value,
    pub schema_call_2: serde_json::Value,
}

pub(crate) fn to_panini_language_levels(
    levels: &[lc_core::user::KnownLanguage],
) -> Vec<panini_engine::prompts::LanguageLevel> {
    levels.iter().map(|l| panini_engine::prompts::LanguageLevel {
        iso_639_3: l.iso_639_3.clone(),
        level: format!("{:?}", l.level),
    }).collect()
}
