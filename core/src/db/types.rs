use chrono::Utc;
use sqlx::{Row, sqlite::SqliteRow};

use crate::sanitize::escape_html;

pub(super) const DEFAULT_USER_ID: &str = "default-user";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyMode {
    Practice,
    Review,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckCountsRecord {
    pub total_cards: usize,
    pub due_new_cards: usize,
    pub due_learning_cards: usize,
    pub due_review_cards: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckSummaryRecord {
    pub id: String,
    pub parent_deck_id: Option<String>,
    pub name: String,
    pub full_path: String,
    pub target_language_iso: String,
    pub counts: DeckCountsRecord,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StudyCardRecord {
    pub card_id: String,
    pub deck_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub card_model_id: String,
    pub front_html: String,
    pub back_html: String,
    pub explanation_html: String,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationBatchRecord {
    pub id: String,
    pub language_iso: String,
    pub tree_definition_id: String,
    pub node_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub card_model_id: String,
    pub default_deck_name: String,
    pub materialized_deck_id: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationBatchCardRecord {
    pub id: String,
    pub template_name: String,
    pub front_html: String,
    pub back_html: String,
    pub explanation_html: String,
    pub fields_json: String,
    pub metadata_json: String,
    pub audio_path: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGenerationBatch {
    pub batch: GenerationBatchRecord,
    pub cards: Vec<GenerationBatchCardRecord>,
}

/// A per-user customization applied on top of the base YAML skill tree.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UserTreeCustomization {
    pub user_id: String,
    pub tree_definition_id: String,
    pub node_id: String,
    pub action: String, // "add" | "hide" | "edit"
    pub parent_id: Option<String>,
    pub node_name: Option<String>,
    pub node_instructions: Option<String>,
    /// JSON-encoded `Vec<String>` of prerequisite node IDs.
    /// None = field not set (for `edit`, preserves existing base-tree prereqs).
    /// Some("[]") = empty list (clears prereqs for `edit`).
    pub prerequisites_json: Option<String>,
    pub sort_order: i32,
    pub created_at: i64,
}

/// A temporary generated card stored in DB before the user saves it to a real deck.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DraftCard {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub template_name: String,
    pub fields_json: String,
    pub explanation: String,
    pub metadata_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGenerationBatchCard {
    pub id: String,
    pub template_name: String,
    pub front_html: String,
    pub back_html: String,
    pub explanation_html: String,
    pub fields_json: String,
    pub metadata_json: String,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGenerationBatch {
    pub id: String,
    pub language_iso: String,
    pub tree_definition_id: String,
    pub node_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub card_model_id: String,
    pub default_deck_name: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub cards: Vec<NewGenerationBatchCard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedGenerationBatch {
    pub deck_id: String,
    pub created_card_count: usize,
}

pub(super) fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub(super) fn parse_card_metadata(metadata_json: &str) -> (String, String) {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or_default();
    let explanation = metadata
        .get("pedagogical_explanation")
        .and_then(|value| value.as_str())
        .map(|value| escape_html(value).replace('\n', "<br>"))
        .unwrap_or_default();
    let ipa = metadata
        .get("ipa")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    (explanation, ipa)
}

pub(super) fn explanation_html_from_metadata(metadata_json: &str) -> String {
    parse_card_metadata(metadata_json).0
}

pub(super) fn row_to_deck_summary(row: &SqliteRow) -> DeckSummaryRecord {
    DeckSummaryRecord {
        id: row.get("id"),
        parent_deck_id: row.get("parent_id"),
        name: row.get("name"),
        full_path: row.get("full_path"),
        target_language_iso: row.get("target_language"),
        counts: DeckCountsRecord {
            total_cards: row.get::<i64, _>("total_cards") as usize,
            due_new_cards: row.get::<Option<i64>, _>("due_new_cards").unwrap_or(0) as usize,
            due_learning_cards: row.get::<Option<i64>, _>("due_learning_cards").unwrap_or(0)
                as usize,
            due_review_cards: row.get::<Option<i64>, _>("due_review_cards").unwrap_or(0) as usize,
        },
    }
}

pub(super) fn row_to_study_card(row: &SqliteRow) -> StudyCardRecord {
    let metadata_json: String = row.get("metadata_json");
    StudyCardRecord {
        card_id: row.get("id"),
        deck_id: row.get("deck_id"),
        skill_id: row.get("skill_id"),
        skill_name: row.get("skill_name"),
        card_model_id: row.get("card_model_id"),
        front_html: row.get("front_html"),
        back_html: row.get("back_html"),
        explanation_html: explanation_html_from_metadata(&metadata_json),
        audio_path: row.get("audio_path"),
    }
}

pub(super) fn row_to_generation_batch(row: &SqliteRow) -> GenerationBatchRecord {
    GenerationBatchRecord {
        id: row.get("id"),
        language_iso: row.get("language_iso"),
        tree_definition_id: row.get("tree_definition_id"),
        node_id: row.get("node_id"),
        skill_id: row.get("skill_id"),
        skill_name: row.get("skill_name"),
        card_model_id: row.get("card_model_id"),
        default_deck_name: row.get("default_deck_name"),
        materialized_deck_id: row.get("materialized_deck_id"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    }
}

pub(super) fn row_to_generation_batch_card(row: &SqliteRow) -> GenerationBatchCardRecord {
    GenerationBatchCardRecord {
        id: row.get("id"),
        template_name: row.get("template_name"),
        front_html: row.get("front_html"),
        back_html: row.get("back_html"),
        explanation_html: row.get("explanation_html"),
        fields_json: row.get("fields_json"),
        metadata_json: row.get("metadata_json"),
        audio_path: row.get("audio_path"),
        created_at: row.get("created_at"),
    }
}

pub(super) fn row_to_draft_card(row: &SqliteRow) -> DraftCard {
    DraftCard {
        id: row.get("id"),
        skill_id: row.get("skill_id"),
        skill_name: row.get("skill_name"),
        template_name: row.get("template_name"),
        fields_json: row.get("fields_json"),
        explanation: row.get("explanation"),
        metadata_json: row.get("metadata_json"),
        created_at: row.get("created_at"),
    }
}

pub(super) fn row_to_customization(row: &SqliteRow) -> UserTreeCustomization {
    UserTreeCustomization {
        user_id: row.get("user_id"),
        tree_definition_id: row.get("tree_definition_id"),
        node_id: row.get("node_id"),
        action: row.get("action"),
        parent_id: row.get("parent_id"),
        node_name: row.get("node_name"),
        node_instructions: row.get("node_instructions"),
        prerequisites_json: row.get("prerequisites_json"),
        sort_order: row.get("sort_order"),
        created_at: row.get("created_at"),
    }
}

pub(super) fn row_to_review_event(row: &SqliteRow) -> crate::srs::ReviewEvent {
    let rating_u8 = row.get::<i64, _>("rating") as u8;
    crate::srs::ReviewEvent {
        rating: crate::srs::Rating::from_u8(rating_u8).unwrap_or(crate::srs::Rating::Again),
        reviewed_at: row.get("reviewed_at"),
    }
}
