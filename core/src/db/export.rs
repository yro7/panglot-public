use sqlx::Row;
use std::collections::BTreeMap;

use super::LocalStorageProvider;
use super::types::parse_card_metadata;
use crate::storage::{NewCardEntry, NewDeckData};

impl LocalStorageProvider {
    pub async fn fetch_decks_for_export(&self) -> Result<Vec<NewDeckData>, sqlx::Error> {
        let records = sqlx::query(
            r#"
            SELECT
                d.full_path,
                d.target_language,
                c.front_html,
                c.back_html,
                c.skill_id,
                c.skill_name,
                c.template_name,
                c.metadata_json,
                c.audio_path,
                c.fields_json
            FROM cards c
            JOIN decks d ON c.deck_id = d.id
            WHERE d.user_id = ?
            ORDER BY d.full_path, c.created_at
            "#,
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut decks_map: BTreeMap<String, NewDeckData> = BTreeMap::new();

        for rec in records {
            let full_path: String = rec.get("full_path");
            let language_code: String = rec.get("target_language");
            let metadata_json: String = rec.get("metadata_json");
            let (explanation, ipa) = parse_card_metadata(&metadata_json);

            let entry = NewCardEntry {
                front_html: rec.get("front_html"),
                back_html: rec.get("back_html"),
                skill_id: rec.get::<Option<String>, _>("skill_id").unwrap_or_default(),
                skill_name: rec
                    .get::<Option<String>, _>("skill_name")
                    .unwrap_or_default(),
                template_name: rec
                    .get::<Option<String>, _>("template_name")
                    .unwrap_or_default(),
                fields_json: rec
                    .get::<Option<String>, _>("fields_json")
                    .unwrap_or_else(|| "{}".to_string()),
                explanation,
                ipa,
                metadata_json,
                audio_path: rec.get("audio_path"),
            };

            decks_map
                .entry(full_path.clone())
                .or_insert_with(|| NewDeckData {
                    name: full_path,
                    language_code: language_code.clone(),
                    cards: Vec::new(),
                })
                .cards
                .push(entry);
        }

        Ok(decks_map.into_values().collect())
    }
}
