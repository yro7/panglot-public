use std::collections::HashMap;

use sqlx::{QueryBuilder, Row, Sqlite};

use super::LocalStorageProvider;
use super::sql::UPSERT_REVIEW_CACHE_SQL;
use super::types::row_to_review_event;

impl LocalStorageProvider {
    pub async fn submit_practice(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let history = self.read_review_history(card_id, &mut tx).await?;
        let output = algorithm.schedule(&history, rating, now);

        sqlx::query(
            "INSERT INTO practice_log (card_id, user_id, rating, practiced_at) VALUES (?, ?, ?, ?)",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(i64::from(rating as u8))
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(output)
    }

    pub async fn submit_review(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let history = self.read_review_history(card_id, &mut tx).await?;
        let output = algorithm.schedule(&history, rating, now);

        sqlx::query(
            "INSERT INTO review_log (card_id, user_id, rating, reviewed_at) VALUES (?, ?, ?, ?)",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(i64::from(rating as u8))
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(UPSERT_REVIEW_CACHE_SQL)
            .bind(card_id)
            .bind(&self.user_id)
            .bind(output.due_date)
            .bind(output.interval_days)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(output)
    }

    pub async fn schedule_options_for_cards(
        &self,
        card_ids: &[String],
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<HashMap<String, crate::srs::SchedulingChoices>, sqlx::Error> {
        let histories = self.read_review_histories_for_cards(card_ids).await?;
        Ok(card_ids
            .iter()
            .map(|card_id| {
                let history = histories.get(card_id).map(Vec::as_slice).unwrap_or(&[]);
                (card_id.clone(), algorithm.preview_choices(history, now))
            })
            .collect())
    }

    async fn read_review_history(
        &self,
        card_id: &str,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<Vec<crate::srs::ReviewEvent>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_all(&mut **tx)
        .await?;
        Ok(records.iter().map(row_to_review_event).collect())
    }

    async fn read_review_histories_for_cards(
        &self,
        card_ids: &[String],
    ) -> Result<HashMap<String, Vec<crate::srs::ReviewEvent>>, sqlx::Error> {
        let mut histories: HashMap<String, Vec<crate::srs::ReviewEvent>> = card_ids
            .iter()
            .cloned()
            .map(|card_id| (card_id, Vec::new()))
            .collect();

        if card_ids.is_empty() {
            return Ok(histories);
        }

        const CHUNK: usize = 200;
        for chunk in card_ids.chunks(CHUNK) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT card_id, rating, reviewed_at FROM review_log WHERE user_id = ",
            );
            qb.push_bind(&self.user_id);
            qb.push(" AND card_id IN (");
            let mut separated = qb.separated(", ");
            for card_id in chunk {
                separated.push_bind(card_id);
            }
            separated.push_unseparated(")");
            drop(separated);
            qb.push(" ORDER BY card_id, reviewed_at");

            let rows = qb.build().fetch_all(&self.pool).await?;
            for row in &rows {
                let card_id: String = row.get("card_id");
                histories
                    .entry(card_id)
                    .or_default()
                    .push(row_to_review_event(row));
            }
        }

        Ok(histories)
    }

    pub async fn get_review_history(
        &self,
        card_id: &str,
    ) -> Result<Vec<crate::srs::ReviewEvent>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(records.iter().map(row_to_review_event).collect())
    }

    pub async fn rebuild_scheduling_cache(
        &self,
        algorithm: &dyn crate::srs::SrsAlgorithm,
    ) -> Result<usize, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let count = self
            .rebuild_scheduling_cache_in_tx(algorithm, &mut tx)
            .await?;
        tx.commit().await?;
        Ok(count)
    }

    pub(crate) async fn rebuild_scheduling_cache_in_tx(
        &self,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<usize, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT card_id, rating, reviewed_at FROM review_log \
             WHERE user_id = ? ORDER BY card_id, reviewed_at",
        )
        .bind(&self.user_id)
        .fetch_all(&mut **tx)
        .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let mut outputs: Vec<(String, crate::srs::SchedulingOutput)> = Vec::new();
        let mut current_card: Option<String> = None;
        let mut history: Vec<crate::srs::ReviewEvent> = Vec::new();
        let mut last_output: Option<crate::srs::SchedulingOutput> = None;

        for row in &rows {
            let card_id: String = row.get("card_id");
            if current_card.as_deref() != Some(card_id.as_str()) {
                if let (Some(previous_card), Some(output)) =
                    (current_card.take(), last_output.take())
                {
                    outputs.push((previous_card, output));
                }
                current_card = Some(card_id);
                history.clear();
            }

            let event = row_to_review_event(row);
            let output = algorithm.schedule(&history, event.rating, event.reviewed_at);
            last_output = Some(output);
            history.push(event);
        }

        if let (Some(previous_card), Some(output)) = (current_card, last_output) {
            outputs.push((previous_card, output));
        }

        if outputs.is_empty() {
            return Ok(0);
        }

        const CHUNK: usize = 200;
        let count = outputs.len();
        for chunk in outputs.chunks(CHUNK) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT INTO reviews (card_id, user_id, due_date, interval_days) ",
            );
            qb.push_values(chunk, |mut builder, (card_id, output)| {
                builder
                    .push_bind(card_id)
                    .push_bind(&self.user_id)
                    .push_bind(output.due_date)
                    .push_bind(output.interval_days);
            });
            qb.push(
                " ON CONFLICT(card_id, user_id) DO UPDATE \
                 SET due_date = excluded.due_date, interval_days = excluded.interval_days",
            );
            qb.build().execute(&mut **tx).await?;
        }

        Ok(count)
    }
}
