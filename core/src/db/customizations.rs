use super::LocalStorageProvider;
use super::types::{UserTreeCustomization, row_to_customization};

impl LocalStorageProvider {
    pub async fn get_tree_customizations(
        &self,
        tree_definition_id: &str,
    ) -> Result<Vec<UserTreeCustomization>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT user_id, tree_definition_id, node_id, action, parent_id, node_name, node_instructions, prerequisites_json, sort_order, created_at \
             FROM user_tree_customizations WHERE user_id = ? AND tree_definition_id = ? ORDER BY sort_order, created_at",
        )
        .bind(&self.user_id)
        .bind(tree_definition_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records.iter().map(row_to_customization).collect())
    }

    pub async fn upsert_tree_customization(
        &self,
        customization: &UserTreeCustomization,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO user_tree_customizations \
             (user_id, tree_definition_id, node_id, action, parent_id, node_name, node_instructions, prerequisites_json, sort_order, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&self.user_id)
        .bind(&customization.tree_definition_id)
        .bind(&customization.node_id)
        .bind(&customization.action)
        .bind(&customization.parent_id)
        .bind(&customization.node_name)
        .bind(&customization.node_instructions)
        .bind(&customization.prerequisites_json)
        .bind(customization.sort_order)
        .bind(customization.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_tree_customization(
        &self,
        tree_definition_id: &str,
        node_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM user_tree_customizations WHERE user_id = ? AND tree_definition_id = ? AND node_id = ?",
        )
        .bind(&self.user_id)
        .bind(tree_definition_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
