use sqlx::{Row, SqlitePool};

use super::LocalStorageProvider;
use super::types::{
    MaterializedGeneration, NewGeneration, StoredGeneration, row_to_generation,
    row_to_generation_card,
};
use crate::storage::{NewCardEntry, NewDeckData};

#[derive(Debug)]
pub enum MaterializeGenerationError {
    NotFound,
    AlreadyMaterialized { deck_id: String },
    Sql(sqlx::Error),
}

impl From<sqlx::Error> for MaterializeGenerationError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sql(value)
    }
}

/// TTL'd cleanup of staged cards. The parent `generations` log row stays —
/// it is permanent.
pub async fn cleanup_expired_generation_cards(
    pool: &SqlitePool,
    cutoff_ms: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM generation_cards WHERE expires_at < ?")
        .bind(cutoff_ms)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

impl LocalStorageProvider {
    pub async fn save_generation(&self, generation: &NewGeneration) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO generations \
             (id, user_id, language_iso, tree_definition_id, tree_node_id, concept_key, \
              card_model_id, card_count, difficulty, user_prompt, default_deck_name, \
              materialized_deck_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(&generation.id)
        .bind(&self.user_id)
        .bind(&generation.language_iso)
        .bind(&generation.tree_definition_id)
        .bind(&generation.tree_node_id)
        .bind(&generation.concept_key)
        .bind(&generation.card_model_id)
        .bind(generation.card_count)
        .bind(generation.difficulty)
        .bind(&generation.user_prompt)
        .bind(&generation.default_deck_name)
        .bind(generation.created_at)
        .execute(&mut *tx)
        .await?;

        for card in &generation.cards {
            sqlx::query(
                "INSERT INTO generation_cards \
                 (id, generation_id, template_name, front_html, back_html, explanation_html, \
                  fields_json, metadata_json, audio_path, created_at, expires_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&card.id)
            .bind(&generation.id)
            .bind(&card.template_name)
            .bind(&card.front_html)
            .bind(&card.back_html)
            .bind(&card.explanation_html)
            .bind(&card.fields_json)
            .bind(&card.metadata_json)
            .bind(&card.audio_path)
            .bind(generation.created_at)
            .bind(generation.expires_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_generation(
        &self,
        generation_id: &str,
    ) -> Result<Option<StoredGeneration>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, language_iso, tree_definition_id, tree_node_id, concept_key, \
             card_model_id, card_count, difficulty, user_prompt, default_deck_name, \
             materialized_deck_id, created_at \
             FROM generations WHERE id = ? AND user_id = ?",
        )
        .bind(generation_id)
        .bind(&self.user_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let card_rows = sqlx::query(
            "SELECT id, template_name, front_html, back_html, explanation_html, fields_json, \
             metadata_json, audio_path, created_at, expires_at \
             FROM generation_cards WHERE generation_id = ? ORDER BY created_at, id",
        )
        .bind(generation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(StoredGeneration {
            generation: row_to_generation(&row),
            cards: card_rows.iter().map(row_to_generation_card).collect(),
        }))
    }

    pub async fn generation_card_audio_path(
        &self,
        generation_id: &str,
        card_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT gc.audio_path \
             FROM generation_cards gc \
             JOIN generations g ON g.id = gc.generation_id \
             WHERE g.id = ? AND gc.id = ? AND g.user_id = ? AND gc.audio_path IS NOT NULL",
        )
        .bind(generation_id)
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Deletes the staged cards for a generation. The generation log row stays.
    /// Use this when the user explicitly discards a preview.
    pub async fn discard_generation_cards(&self, generation_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM generation_cards \
             WHERE generation_id = ? \
               AND generation_id IN (SELECT id FROM generations WHERE user_id = ?)",
        )
        .bind(generation_id)
        .bind(&self.user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn materialize_generation_to_deck(
        &self,
        generation_id: &str,
        deck_name: &str,
        parent_deck_id: Option<&str>,
    ) -> Result<MaterializedGeneration, MaterializeGenerationError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            "SELECT id, language_iso, card_model_id, materialized_deck_id \
             FROM generations WHERE id = ? AND user_id = ?",
        )
        .bind(generation_id)
        .bind(&self.user_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Err(MaterializeGenerationError::NotFound);
        };

        let existing_deck_id: Option<String> = row.get("materialized_deck_id");
        if let Some(deck_id) = existing_deck_id {
            return Err(MaterializeGenerationError::AlreadyMaterialized { deck_id });
        }

        let language_iso: String = row.get("language_iso");
        let card_model_id: String = row.get("card_model_id");
        let deck_path = deck_path_parts(deck_name);
        let leaf_deck_name = deck_path
            .last()
            .cloned()
            .unwrap_or_else(|| deck_name.trim().to_string());
        let mut current_parent_id = parent_deck_id.map(str::to_string);

        let card_rows = sqlx::query(
            "SELECT id, template_name, front_html, back_html, explanation_html, fields_json, \
             metadata_json, audio_path, created_at \
             FROM generation_cards WHERE generation_id = ? ORDER BY created_at, id",
        )
        .bind(generation_id)
        .fetch_all(&mut *tx)
        .await?;

        for parent_name in deck_path.iter().take(deck_path.len().saturating_sub(1)) {
            let deck_id = self
                .get_or_create_empty_deck_in_tx(
                    parent_name,
                    &language_iso,
                    current_parent_id.as_deref(),
                    &mut tx,
                )
                .await?;
            current_parent_id = Some(deck_id);
        }

        let deck_data = NewDeckData {
            name: leaf_deck_name,
            language_code: language_iso,
            generation_id: Some(generation_id.to_string()),
            parent_deck_id: current_parent_id,
            cards: card_rows
                .iter()
                .map(|row| NewCardEntry {
                    front_html: row.get("front_html"),
                    back_html: row.get("back_html"),
                    card_model_id: card_model_id.clone(),
                    template_name: row.get("template_name"),
                    fields_json: row.get("fields_json"),
                    explanation: row.get("explanation_html"),
                    ipa: String::new(),
                    metadata_json: row.get("metadata_json"),
                    audio_path: row.get("audio_path"),
                })
                .collect(),
        };

        let (deck_id, created_card_count) =
            self.save_new_deck_data_in_tx(&deck_data, &mut tx).await?;

        // Drop the staged cards now that they're persisted in `cards`.
        sqlx::query("DELETE FROM generation_cards WHERE generation_id = ?")
            .bind(generation_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE generations SET materialized_deck_id = ? WHERE id = ? AND user_id = ?")
            .bind(&deck_id)
            .bind(generation_id)
            .bind(&self.user_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(MaterializedGeneration {
            deck_id,
            created_card_count,
        })
    }
}

fn deck_path_parts(deck_name: &str) -> Vec<String> {
    let parts: Vec<String> = deck_name
        .split("::")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();

    if parts.is_empty() {
        vec![deck_name.trim().to_string()]
    } else {
        parts
    }
}
