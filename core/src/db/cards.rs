use super::LocalStorageProvider;
use super::sql::{DECK_TREE_CTE, STUDY_CARD_PROJECTION};
use super::types::{StudyCardRecord, StudyMode, row_to_study_card};

impl LocalStorageProvider {
    pub async fn verify_card_ownership(&self, card_id: &str) -> Result<bool, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cards WHERE id = ? AND user_id = ?")
                .bind(card_id)
                .bind(&self.user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count > 0)
    }

    pub async fn card_audio_path(&self, card_id: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT audio_path FROM cards WHERE id = ? AND user_id = ? AND audio_path IS NOT NULL",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn select_study_cards(
        &self,
        deck_id: &str,
        mode: StudyMode,
        limit: i64,
    ) -> Result<Vec<StudyCardRecord>, sqlx::Error> {
        let due_cutoff = self.due_cutoff().await?;

        let mut sql = String::with_capacity(512);
        sql.push_str(DECK_TREE_CTE);
        sql.push_str("SELECT ");
        sql.push_str(STUDY_CARD_PROJECTION);
        sql.push_str(" FROM cards c ");
        match mode {
            StudyMode::Practice => sql.push_str(
                "LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ? \
                 WHERE c.deck_id IN deck_tree \
                 ORDER BY COALESCE(r.due_date, 0) ASC, c.created_at ASC LIMIT ?",
            ),
            StudyMode::Review => sql.push_str(
                "JOIN reviews r ON c.id = r.card_id \
                 WHERE c.deck_id IN deck_tree AND r.user_id = ? AND r.due_date <= ? \
                 ORDER BY r.due_date ASC, c.created_at ASC LIMIT ?",
            ),
        }

        let mut q = sqlx::query(&sql)
            .bind(deck_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id);
        if mode == StudyMode::Review {
            q = q.bind(due_cutoff);
        }
        let records = q.bind(limit).fetch_all(&self.pool).await?;

        Ok(records.iter().map(row_to_study_card).collect())
    }
}
