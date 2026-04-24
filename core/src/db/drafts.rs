use sqlx::{QueryBuilder, Sqlite};

use super::LocalStorageProvider;
use super::types::{DraftCard, now_ms, row_to_draft_card};

impl LocalStorageProvider {
    pub async fn save_drafts(&self, cards: &[DraftCard]) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        for card in cards {
            sqlx::query(
                "INSERT OR REPLACE INTO draft_cards (id, user_id, skill_id, skill_name, template_name, fields_json, explanation, metadata_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&card.id)
            .bind(&self.user_id)
            .bind(&card.skill_id)
            .bind(&card.skill_name)
            .bind(&card.template_name)
            .bind(&card.fields_json)
            .bind(&card.explanation)
            .bind(&card.metadata_json)
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_drafts(&self) -> Result<Vec<DraftCard>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT id, skill_id, skill_name, template_name, fields_json, explanation, metadata_json, created_at FROM draft_cards WHERE user_id = ? ORDER BY created_at",
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records.iter().map(row_to_draft_card).collect())
    }

    pub async fn clear_drafts(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM draft_cards WHERE user_id = ?")
            .bind(&self.user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_drafts(&self, ids: &[String]) -> Result<(), sqlx::Error> {
        for chunk in ids.chunks(500) {
            let mut qb = QueryBuilder::<Sqlite>::new("DELETE FROM draft_cards WHERE user_id = ");
            qb.push_bind(&self.user_id).push(" AND id IN (");
            let mut sep = qb.separated(", ");
            for id in chunk {
                sep.push_bind(id);
            }
            qb.push(")");
            qb.build().execute(&self.pool).await?;
        }
        Ok(())
    }
}
