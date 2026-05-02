use sqlx::{QueryBuilder, Sqlite};
use uuid::Uuid;

use super::LocalStorageProvider;
use super::sql::{DECK_CLOSURE_CTE, DECK_SUMMARY_SELECT};
use super::types::{DeckSummaryRecord, now_ms, row_to_deck_summary};
use crate::storage::{NewCardEntry, NewDeckData};

#[derive(Debug)]
pub enum MoveDeckError {
    NotFound,
    NotOwned,
    ParentNotFound,
    Cycle,
    SiblingNameConflict,
    Sql(sqlx::Error),
}

impl From<sqlx::Error> for MoveDeckError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sql(value)
    }
}

impl LocalStorageProvider {
    pub(crate) async fn save_new_deck_data_in_tx(
        &self,
        deck_data: &NewDeckData,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<(String, usize), sqlx::Error> {
        let now = now_ms();
        let deck_id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO decks (id, user_id, parent_id, name, target_language, generation_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&deck_id)
        .bind(&self.user_id)
        .bind(&deck_data.parent_deck_id)
        .bind(&deck_data.name)
        .bind(&deck_data.language_code)
        .bind(&deck_data.generation_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        let created_card_count = self
            .save_cards_for_deck_in_tx(&deck_id, &deck_data.cards, tx)
            .await?;

        Ok((deck_id, created_card_count))
    }

    pub(crate) async fn save_cards_for_deck_in_tx(
        &self,
        deck_id: &str,
        cards: &[NewCardEntry],
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<usize, sqlx::Error> {
        if cards.is_empty() {
            return Ok(0);
        }

        let now = now_ms();
        let card_ids: Vec<String> = (0..cards.len())
            .map(|_| Uuid::now_v7().to_string())
            .collect();

        const CARDS_CHUNK: usize = 75;
        for (chunk_idx, id_chunk) in card_ids.chunks(CARDS_CHUNK).enumerate() {
            let offset = chunk_idx * CARDS_CHUNK;
            let entries = &cards[offset..offset + id_chunk.len()];
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT INTO cards (id, deck_id, user_id, card_model_id, template_name, front_html, back_html, fields_json, metadata_json, audio_path, created_at) ",
            );
            qb.push_values(id_chunk.iter().zip(entries), |mut b, (id, entry)| {
                b.push_bind(id)
                    .push_bind(deck_id)
                    .push_bind(&self.user_id)
                    .push_bind(&entry.card_model_id)
                    .push_bind(&entry.template_name)
                    .push_bind(&entry.front_html)
                    .push_bind(&entry.back_html)
                    .push_bind(&entry.fields_json)
                    .push_bind(&entry.metadata_json)
                    .push_bind(&entry.audio_path)
                    .push_bind(now);
            });
            qb.build().execute(&mut **tx).await?;
        }

        const REVIEWS_CHUNK: usize = 200;
        for id_chunk in card_ids.chunks(REVIEWS_CHUNK) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT INTO reviews (card_id, user_id, due_date, interval_days) ",
            );
            qb.push_values(id_chunk.iter(), |mut b, id| {
                b.push_bind(id)
                    .push_bind(&self.user_id)
                    .push_bind(now)
                    .push_bind(0i64);
            });
            qb.build().execute(&mut **tx).await?;
        }

        Ok(card_ids.len())
    }

    pub(crate) async fn deck_id_by_name_in_tx(
        &self,
        name: &str,
        parent_deck_id: Option<&str>,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM decks WHERE user_id = ? AND name = ? \
             AND IFNULL(parent_id, '') = IFNULL(?, '')",
        )
        .bind(&self.user_id)
        .bind(name)
        .bind(parent_deck_id)
        .fetch_optional(&mut **tx)
        .await
    }

    pub(crate) async fn get_or_create_empty_deck_in_tx(
        &self,
        name: &str,
        language_code: &str,
        parent_deck_id: Option<&str>,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<String, sqlx::Error> {
        if let Some(deck_id) = self.deck_id_by_name_in_tx(name, parent_deck_id, tx).await? {
            return Ok(deck_id);
        }

        let deck_data = NewDeckData {
            name: name.to_string(),
            language_code: language_code.to_string(),
            generation_id: None,
            parent_deck_id: parent_deck_id.map(str::to_string),
            cards: vec![],
        };
        let (deck_id, _created_card_count) = self.save_new_deck_data_in_tx(&deck_data, tx).await?;
        Ok(deck_id)
    }

    pub async fn verify_deck_ownership(&self, deck_id: &str) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM decks WHERE id = ? AND user_id = ?")
                .bind(deck_id)
                .bind(&self.user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count > 0)
    }

    /// Re-parent and/or rename a deck. Validates ownership, parent ownership,
    /// cycle prevention (parent can't be in self subtree), and sibling-name
    /// uniqueness under the new parent.
    pub async fn move_deck(
        &self,
        deck_id: &str,
        new_parent_id: Option<&str>,
        new_name: Option<&str>,
    ) -> Result<(), MoveDeckError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as::<_, (String, Option<String>, String)>(
            "SELECT user_id, parent_id, name FROM decks WHERE id = ?",
        )
        .bind(deck_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((owner, _current_parent, current_name)) = row else {
            return Err(MoveDeckError::NotFound);
        };
        if owner != self.user_id {
            return Err(MoveDeckError::NotOwned);
        }

        if let Some(parent_id) = new_parent_id {
            let parent_row =
                sqlx::query_as::<_, (String,)>("SELECT user_id FROM decks WHERE id = ?")
                    .bind(parent_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some((parent_owner,)) = parent_row else {
                return Err(MoveDeckError::ParentNotFound);
            };
            if parent_owner != self.user_id {
                return Err(MoveDeckError::ParentNotFound);
            }

            // Cycle check: parent must not be in the moved deck's subtree.
            let cycle: Option<i64> = sqlx::query_scalar(
                "WITH RECURSIVE subtree(id) AS ( \
                    SELECT id FROM decks WHERE id = ? \
                    UNION ALL \
                    SELECT d.id FROM decks d JOIN subtree s ON d.parent_id = s.id \
                 ) \
                 SELECT 1 FROM subtree WHERE id = ? LIMIT 1",
            )
            .bind(deck_id)
            .bind(parent_id)
            .fetch_optional(&mut *tx)
            .await?;
            if cycle.is_some() {
                return Err(MoveDeckError::Cycle);
            }
        }

        let target_name = new_name.unwrap_or(&current_name);
        let target_parent = new_parent_id.map(|s| s.to_string());

        let conflict: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM decks \
             WHERE user_id = ? \
               AND IFNULL(parent_id, '') = IFNULL(?, '') \
               AND name = ? \
               AND id != ? \
             LIMIT 1",
        )
        .bind(&self.user_id)
        .bind(&target_parent)
        .bind(target_name)
        .bind(deck_id)
        .fetch_optional(&mut *tx)
        .await?;
        if conflict.is_some() {
            return Err(MoveDeckError::SiblingNameConflict);
        }

        sqlx::query("UPDATE decks SET parent_id = ?, name = ? WHERE id = ? AND user_id = ?")
            .bind(&target_parent)
            .bind(target_name)
            .bind(deck_id)
            .bind(&self.user_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn due_cutoff(&self) -> Result<i64, sqlx::Error> {
        let settings = self.get_user_settings().await?;
        let learn_ahead_ms =
            i64::from(settings.study_preferences.learn_ahead_minutes.get()) * 60 * 1000;
        Ok(now_ms() + learn_ahead_ms)
    }

    fn build_deck_summary_sql(filter_by_id: bool) -> String {
        let mut sql = String::with_capacity(1024);
        sql.push_str(DECK_CLOSURE_CTE);
        sql.push_str(DECK_SUMMARY_SELECT);
        sql.push_str(if filter_by_id {
            " WHERE d.user_id = ? AND d.id = ? GROUP BY d.id"
        } else {
            " WHERE d.user_id = ? GROUP BY d.id ORDER BY dp.full_path"
        });
        sql
    }

    pub async fn list_deck_summaries(&self) -> Result<Vec<DeckSummaryRecord>, sqlx::Error> {
        let due_cutoff = self.due_cutoff().await?;
        let sql = Self::build_deck_summary_sql(false);
        let records = sqlx::query(&sql)
            // CTE binds: closure anchor, closure recursion, deck_path anchor,
            // deck_path recursion, review_counts.
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(due_cutoff)
            .bind(due_cutoff)
            .bind(due_cutoff)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(records.iter().map(row_to_deck_summary).collect())
    }

    pub async fn get_deck_summary(
        &self,
        deck_id: &str,
    ) -> Result<Option<DeckSummaryRecord>, sqlx::Error> {
        let due_cutoff = self.due_cutoff().await?;
        let sql = Self::build_deck_summary_sql(true);
        let record = sqlx::query(&sql)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(due_cutoff)
            .bind(due_cutoff)
            .bind(due_cutoff)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(deck_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(record.as_ref().map(row_to_deck_summary))
    }
}
