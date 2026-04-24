use sqlx::{Row, SqlitePool};

use super::LocalStorageProvider;
use super::types::{
    GenerationBatchRecord, MaterializedGenerationBatch, NewGenerationBatch,
    row_to_generation_batch, row_to_generation_batch_card,
};
use crate::storage::{NewCardEntry, NewDeckData};

#[derive(Debug)]
pub enum MaterializeGenerationBatchError {
    NotFound,
    AlreadyMaterialized { deck_id: String },
    Sql(sqlx::Error),
}

impl From<sqlx::Error> for MaterializeGenerationBatchError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sql(value)
    }
}

pub async fn cleanup_expired_generation_batches(
    pool: &SqlitePool,
    cutoff_ms: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM generation_batches WHERE expires_at < ?")
        .bind(cutoff_ms)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

impl LocalStorageProvider {
    pub async fn save_generation_batch(
        &self,
        batch: &NewGenerationBatch,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO generation_batches \
             (id, user_id, language_iso, tree_definition_id, node_id, skill_id, skill_name, card_model_id, default_deck_name, materialized_deck_id, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(&batch.id)
        .bind(&self.user_id)
        .bind(&batch.language_iso)
        .bind(&batch.tree_definition_id)
        .bind(&batch.node_id)
        .bind(&batch.skill_id)
        .bind(&batch.skill_name)
        .bind(&batch.card_model_id)
        .bind(&batch.default_deck_name)
        .bind(batch.created_at)
        .bind(batch.expires_at)
        .execute(&mut *tx)
        .await?;

        for card in &batch.cards {
            sqlx::query(
                "INSERT INTO generation_batch_cards \
                 (id, generation_batch_id, template_name, front_html, back_html, explanation_html, fields_json, metadata_json, audio_path, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&card.id)
            .bind(&batch.id)
            .bind(&card.template_name)
            .bind(&card.front_html)
            .bind(&card.back_html)
            .bind(&card.explanation_html)
            .bind(&card.fields_json)
            .bind(&card.metadata_json)
            .bind(&card.audio_path)
            .bind(batch.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_generation_batch(
        &self,
        batch_id: &str,
    ) -> Result<Option<super::types::StoredGenerationBatch>, sqlx::Error> {
        let batch = sqlx::query(
            "SELECT id, language_iso, tree_definition_id, node_id, skill_id, skill_name, card_model_id, default_deck_name, materialized_deck_id, created_at, expires_at \
             FROM generation_batches WHERE id = ? AND user_id = ?",
        )
        .bind(batch_id)
        .bind(&self.user_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(batch_row) = batch else {
            return Ok(None);
        };

        let cards = sqlx::query(
            "SELECT id, template_name, front_html, back_html, explanation_html, fields_json, metadata_json, audio_path, created_at \
             FROM generation_batch_cards WHERE generation_batch_id = ? ORDER BY created_at, id",
        )
        .bind(batch_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(super::types::StoredGenerationBatch {
            batch: row_to_generation_batch(&batch_row),
            cards: cards.iter().map(row_to_generation_batch_card).collect(),
        }))
    }

    pub async fn generation_batch_card_audio_path(
        &self,
        batch_id: &str,
        card_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT gbc.audio_path \
             FROM generation_batch_cards gbc \
             JOIN generation_batches gb ON gb.id = gbc.generation_batch_id \
             WHERE gb.id = ? AND gbc.id = ? AND gb.user_id = ? AND gbc.audio_path IS NOT NULL",
        )
        .bind(batch_id)
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn delete_generation_batch(&self, batch_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM generation_batches WHERE id = ? AND user_id = ?")
            .bind(batch_id)
            .bind(&self.user_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn materialize_generation_batch_to_deck(
        &self,
        batch_id: &str,
        deck_name: &str,
    ) -> Result<MaterializedGenerationBatch, MaterializeGenerationBatchError> {
        let mut tx = self.pool.begin().await?;

        let batch_row = sqlx::query(
            "SELECT id, language_iso, skill_id, skill_name, card_model_id, materialized_deck_id \
             FROM generation_batches WHERE id = ? AND user_id = ?",
        )
        .bind(batch_id)
        .bind(&self.user_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(batch_row) = batch_row else {
            return Err(MaterializeGenerationBatchError::NotFound);
        };

        let existing_deck_id: Option<String> = batch_row.get("materialized_deck_id");
        if let Some(deck_id) = existing_deck_id {
            return Err(MaterializeGenerationBatchError::AlreadyMaterialized { deck_id });
        }

        let batch = GenerationBatchRecord {
            id: batch_row.get("id"),
            language_iso: batch_row.get("language_iso"),
            tree_definition_id: String::new(),
            node_id: String::new(),
            skill_id: batch_row.get("skill_id"),
            skill_name: batch_row.get("skill_name"),
            card_model_id: batch_row.get("card_model_id"),
            default_deck_name: String::new(),
            materialized_deck_id: None,
            created_at: 0,
            expires_at: 0,
        };

        let card_rows = sqlx::query(
            "SELECT id, template_name, front_html, back_html, explanation_html, fields_json, metadata_json, audio_path, created_at \
             FROM generation_batch_cards WHERE generation_batch_id = ? ORDER BY created_at, id",
        )
        .bind(batch_id)
        .fetch_all(&mut *tx)
        .await?;

        let deck_data = NewDeckData {
            name: deck_name.to_string(),
            language_code: batch.language_iso.clone(),
            cards: card_rows
                .iter()
                .map(|row| NewCardEntry {
                    front_html: row.get("front_html"),
                    back_html: row.get("back_html"),
                    skill_id: batch.skill_id.clone(),
                    skill_name: batch.skill_name.clone(),
                    card_model_id: batch.card_model_id.clone(),
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

        sqlx::query(
            "UPDATE generation_batches SET materialized_deck_id = ? WHERE id = ? AND user_id = ?",
        )
        .bind(&deck_id)
        .bind(batch_id)
        .bind(&self.user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(MaterializedGenerationBatch {
            deck_id,
            created_card_count,
        })
    }
}
