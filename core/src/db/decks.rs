use sqlx::Row;
use uuid::Uuid;

use super::LocalStorageProvider;
use super::sql::{DECK_CLOSURE_CTE, DECK_SUMMARY_SELECT};
use super::types::{DeckSummaryRecord, now_ms, row_to_deck_summary};

impl LocalStorageProvider {
    pub(super) async fn get_or_create_deck_hierarchy(
        &self,
        full_path: &str,
        language_code: &str,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<String, sqlx::Error> {
        let parts: Vec<&str> = full_path.split("::").collect();
        let mut current_parent_id: Option<String> = None;
        let mut current_path = String::new();
        let mut last_deck_id = String::new();
        let now = now_ms();

        for part in parts {
            if !current_path.is_empty() {
                current_path.push_str("::");
            }
            current_path.push_str(part);

            let deck_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO decks (id, user_id, parent_id, name, full_path, target_language, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&deck_id)
            .bind(&self.user_id)
            .bind(&current_parent_id)
            .bind(part)
            .bind(&current_path)
            .bind(language_code)
            .bind(now)
            .execute(&mut **tx)
            .await?;

            let row = sqlx::query("SELECT id FROM decks WHERE full_path = ? AND user_id = ?")
                .bind(&current_path)
                .bind(&self.user_id)
                .fetch_one(&mut **tx)
                .await?;
            last_deck_id = row.get::<String, _>("id");
            current_parent_id = Some(last_deck_id.clone());
        }

        Ok(last_deck_id)
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
            " WHERE d.user_id = ? GROUP BY d.id ORDER BY d.full_path"
        });
        sql
    }

    pub async fn list_deck_summaries(&self) -> Result<Vec<DeckSummaryRecord>, sqlx::Error> {
        let due_cutoff = self.due_cutoff().await?;
        let sql = Self::build_deck_summary_sql(false);
        let records = sqlx::query(&sql)
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
