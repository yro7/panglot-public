use sqlx::{QueryBuilder, Row, Sqlite};

use super::LocalStorageProvider;
use super::sql::UPSERT_REVIEW_CACHE_SQL;
use super::types::row_to_review_event;

impl LocalStorageProvider {
    pub async fn preview_rating(
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
        // Single fetch, pre-sorted by card_id so we can group in a linear pass.
        let rows = sqlx::query(
            "SELECT card_id, rating, reviewed_at FROM review_log \
             WHERE user_id = ? ORDER BY card_id, reviewed_at",
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        // Group rows by card_id, fold history → final SchedulingOutput per card.
        let mut outputs: Vec<(String, crate::srs::SchedulingOutput)> = Vec::new();
        let mut current_card: Option<String> = None;
        let mut history: Vec<crate::srs::ReviewEvent> = Vec::new();
        let mut last_output: Option<crate::srs::SchedulingOutput> = None;

        for row in &rows {
            let card_id: String = row.get("card_id");
            if current_card.as_deref() != Some(card_id.as_str()) {
                if let (Some(prev), Some(out)) = (current_card.take(), last_output.take()) {
                    outputs.push((prev, out));
                }
                current_card = Some(card_id);
                history.clear();
            }
            let event = row_to_review_event(row);
            let output = algorithm.schedule(&history, event.rating, event.reviewed_at);
            last_output = Some(output);
            history.push(event);
        }
        if let (Some(prev), Some(out)) = (current_card, last_output) {
            outputs.push((prev, out));
        }

        if outputs.is_empty() {
            return Ok(0);
        }

        // Batched UPSERT: 4 params/row, chunk at 200 rows to stay under SQLite's 999-param limit.
        const CHUNK: usize = 200;
        let count = outputs.len();
        let mut tx = self.pool.begin().await?;
        for chunk in outputs.chunks(CHUNK) {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT INTO reviews (card_id, user_id, due_date, interval_days) ",
            );
            qb.push_values(chunk, |mut b, (card_id, out)| {
                b.push_bind(card_id)
                    .push_bind(&self.user_id)
                    .push_bind(out.due_date)
                    .push_bind(out.interval_days);
            });
            qb.push(
                " ON CONFLICT(card_id, user_id) DO UPDATE \
                 SET due_date = excluded.due_date, interval_days = excluded.interval_days",
            );
            qb.build().execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(count)
    }
}
