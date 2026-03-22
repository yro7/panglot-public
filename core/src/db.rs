use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;
use uuid::Uuid;
use chrono::Utc;

use crate::storage::{DeckInfo, NewCardEntry, NewDeckData, StorageProvider, StoredCard};

/// A local study card fetched for a study session
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LocalStudyCard {
    pub id: String,
    pub front_html: String,
    pub back_html: String,
    pub template_name: String,
    pub fields: serde_json::Value,
    pub metadata_json: String,
}

/// A per-user customization applied on top of the base YAML skill tree.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UserTreeCustomization {
    pub user_id: String,
    pub language: String,
    pub node_id: String,
    pub action: String, // "add" | "hide" | "edit"
    pub parent_id: Option<String>,
    pub node_name: Option<String>,
    pub node_instructions: Option<String>,
    pub sort_order: i32,
    pub created_at: i64,
}

/// A temporary generated card stored in DB before the user saves it to a real deck.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DraftCard {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub template_name: String,
    pub fields_json: String,
    pub explanation: String,
    pub metadata_json: String,
    pub created_at: i64,
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// A SQLite-backed implementation of the StorageProvider trait for local studies.
pub struct LocalStorageProvider {
    pub pool: SqlitePool,
    pub user_id: String,
}

impl LocalStorageProvider {
    /// Creates a provider scoped to a specific user (from JWT `sub` claim).
    pub fn for_user(pool: SqlitePool, user_id: String) -> Self {
        Self { pool, user_id }
    }

    /// Auto-provision user from JWT claims. Provider-agnostic:
    /// works with email, GitHub, Google, phone, anonymous sign-in.
    pub async fn ensure_user(&self, claims: &serde_json::Value) -> Result<(), sqlx::Error> {
        let display_name = claims["user_metadata"]["full_name"].as_str()
            .or_else(|| claims["user_metadata"]["preferred_username"].as_str())
            .or_else(|| claims["email"].as_str().and_then(|e| e.split('@').next()))
            .unwrap_or("user");

        let email = claims["email"].as_str();

        sqlx::query(
            "INSERT INTO users (id, display_name, email, settings, created_at)
             VALUES (?, ?, ?, '{}', ?)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, email = COALESCE(excluded.email, email)"
        )
        .bind(&self.user_id)
        .bind(display_name)
        .bind(email)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Initializes the database connection and runs migrations.
    pub async fn init(db_path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
        let db_path = db_path.as_ref();

        // Create parent directories if they don't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
        }

        let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .after_connect(|conn, _meta| Box::pin(async move {
                use sqlx::Executor;
                conn.execute("PRAGMA foreign_keys = ON").await?;
                conn.execute("PRAGMA journal_mode = WAL").await?;
                Ok(())
            }))
            .connect(&database_url)
            .await?;

        let provider = Self {
            pool,
            user_id: "default-user".to_string(),
        };

        sqlx::migrate!("./migrations")
            .run(&provider.pool)
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Migration failed: {e}")))?;

        provider.ensure_default_user().await?;

        Ok(provider)
    }


    async fn ensure_default_user(&self) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let query = "INSERT INTO users (id, display_name, email, settings, created_at) VALUES (?, ?, NULL, '{}', ?) ON CONFLICT(id) DO NOTHING";
        sqlx::query(query)
            .bind(&self.user_id)
            .bind("Default User")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Recursively creates parent decks if they don't exist based on a "A::B::C" path format.
    /// Uses INSERT OR IGNORE to avoid SELECT-then-INSERT race and reduce round-trips.
    async fn get_or_create_deck_hierarchy(
        &self,
        full_path: &str,
        language_code: &str,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<String, sqlx::Error> {
        use sqlx::Row;
        let parts: Vec<&str> = full_path.split("::").collect();
        let mut current_parent_id: Option<String> = None;
        let mut current_path = String::new();
        let mut last_deck_id = String::new();

        let now = Utc::now().timestamp_millis();

        for part in parts {
            if current_path.is_empty() {
                current_path = part.to_string();
            } else {
                current_path = format!("{}::{}", current_path, part);
            }

            let deck_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO decks (id, user_id, parent_id, name, full_path, target_language, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
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

            // Always fetch the actual ID (may be ours or pre-existing)
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

    /// Fetch user settings from the database
    pub async fn get_user_settings(&self) -> Result<crate::user::UserSettings, sqlx::Error> {
        let row = sqlx::query("SELECT settings FROM users WHERE id = ?")
            .bind(&self.user_id)
            .fetch_one(&self.pool)
            .await?;

        use sqlx::Row;
        let settings_json: String = row.get("settings");
        let settings = serde_json::from_str(&settings_json).unwrap_or_default();
        Ok(settings)
    }

    /// Update user settings
    pub async fn update_user_settings(&self, settings: &crate::user::UserSettings) -> Result<(), sqlx::Error> {
        let settings_json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string());
        sqlx::query("UPDATE users SET settings = ? WHERE id = ?")
            .bind(settings_json)
            .bind(&self.user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Fetches due cards for a given deck, including all sub-decks recursively.
    pub async fn get_due_cards_for_deck(&self, deck_id: &str, limit: i64) -> Result<Vec<LocalStudyCard>, sqlx::Error> {
        let settings = self.get_user_settings().await.unwrap_or_default();
        let learn_ahead_ms = (settings.learn_ahead_minutes.get() as i64) * 60 * 1000;
        let now = Utc::now().timestamp_millis() + learn_ahead_ms;
        
        let records = sqlx::query(
            r#"
            WITH RECURSIVE deck_tree(id) AS (
                SELECT id FROM decks WHERE id = ?
                UNION ALL
                SELECT d.id FROM decks d JOIN deck_tree dt ON d.parent_id = dt.id
            )
            SELECT c.id, c.front_html, c.back_html, c.template_name, c.fields_json, c.metadata_json
            FROM cards c
            JOIN reviews r ON c.id = r.card_id
            WHERE c.deck_id IN deck_tree AND r.user_id = ? AND r.due_date <= ?
            ORDER BY r.due_date ASC
            LIMIT ?
            "#
        )
        .bind(deck_id)
        .bind(&self.user_id)
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let cards = records.into_iter().map(|r| {
            use sqlx::Row;
            let fields_json_str: String = r.get("fields_json");
            let fields: serde_json::Value = serde_json::from_str(&fields_json_str)
                .unwrap_or(serde_json::json!({}));
            LocalStudyCard {
                id: r.get("id"),
                front_html: r.get("front_html"),
                back_html: r.get("back_html"),
                template_name: r.get::<Option<String>, _>("template_name").unwrap_or_default(),
                fields,
                metadata_json: r.get::<String, _>("metadata_json"),
            }
        }).collect();

        Ok(cards)
    }

    /// Fetches all cards from the local DB grouped by deck, ready for .apkg export.
    pub async fn fetch_decks_for_export(&self) -> Result<Vec<NewDeckData>, sqlx::Error> {
        let records = sqlx::query(
            r#"
            SELECT d.full_path, d.target_language,
                   c.front_html, c.back_html, c.skill_id, c.metadata_json, c.audio_path,
                   c.fields_json
            FROM cards c
            JOIN decks d ON c.deck_id = d.id
            WHERE d.user_id = ?
            ORDER BY d.full_path, c.created_at
            "#
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        use sqlx::Row;
        use std::collections::BTreeMap;

        let mut decks_map: BTreeMap<String, NewDeckData> = BTreeMap::new();

        for rec in records {
            let full_path: String = rec.get("full_path");
            let language_code: String = rec.get::<Option<String>, _>("target_language").unwrap_or_default();
            let metadata_json: String = rec.get("metadata_json");

            // Try to extract explanation and IPA from metadata_json
            let (explanation, ipa) = {
                let meta: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or_default();
                (
                    meta.get("pedagogical_explanation").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    meta.get("ipa").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                )
            };

            let entry = NewCardEntry {
                front_html: rec.get("front_html"),
                back_html: rec.get("back_html"),
                skill_name: rec.get::<Option<String>, _>("skill_id").unwrap_or_default(),
                template_name: rec.get::<Option<String>, _>("template_name").unwrap_or_default(),
                fields_json: rec.get::<Option<String>, _>("fields_json").unwrap_or_else(|| "{}".to_string()),
                explanation,
                ipa,
                metadata_json,
                audio_path: rec.get("audio_path"),
            };

            decks_map.entry(full_path.clone())
                .or_insert_with(|| NewDeckData {
                    name: full_path,
                    language_code: language_code.clone(),
                    cards: Vec::new(),
                })
                .cards.push(entry);
        }

        Ok(decks_map.into_values().collect())
    }

    /// Submits a review rating for a specific card using the given SRS algorithm.
    /// Returns the scheduling output (due_date, interval_days).
    pub async fn submit_review(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        // 1. Append raw fact to review_log
        sqlx::query(
            "INSERT INTO review_log (card_id, user_id, rating, reviewed_at) VALUES (?, ?, ?, ?)"
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(rating as u8 as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // 2. Fetch full history
        let history = self.get_review_history(card_id).await?;

        // 3. Compute schedule (history already includes the new review we just inserted)
        //    But schedule() expects history *before* the current rating, so we pass history
        //    without the last element (the one we just inserted) and pass rating separately.
        let history_before: Vec<_> = if history.len() > 1 {
            history[..history.len() - 1].to_vec()
        } else {
            vec![]
        };
        let output = algorithm.schedule(&history_before, rating, now);

        // 4. Update scheduling cache
        sqlx::query(
            r#"
            INSERT INTO reviews (card_id, user_id, due_date, interval_days)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(card_id, user_id) DO UPDATE SET due_date = excluded.due_date, interval_days = excluded.interval_days
            "#
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(output.due_date)
        .bind(output.interval_days)
        .execute(&self.pool)
        .await?;

        Ok(output)
    }

    /// Fetches the full review history for a card (for the current user).
    pub async fn get_review_history(&self, card_id: &str) -> Result<Vec<crate::srs::ReviewEvent>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at"
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let events = records.into_iter().map(|r| {
            use sqlx::Row;
            let rating_u8 = r.get::<i64, _>("rating") as u8;
            crate::srs::ReviewEvent {
                rating: crate::srs::Rating::from_u8(rating_u8).unwrap_or(crate::srs::Rating::Again),
                reviewed_at: r.get("reviewed_at"),
            }
        }).collect();

        Ok(events)
    }

    /// Replays the full review history for this user and recalculates all scheduling caches.
    /// Returns the number of cards rebuilt.
    pub async fn rebuild_scheduling_cache(
        &self,
        algorithm: &dyn crate::srs::SrsAlgorithm,
    ) -> Result<usize, sqlx::Error> {
        // Get all distinct card_ids with reviews for this user
        let card_rows = sqlx::query(
            "SELECT DISTINCT card_id FROM review_log WHERE user_id = ?"
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0;
        let mut tx = self.pool.begin().await?;

        for row in &card_rows {
            use sqlx::Row;
            let card_id: String = row.get("card_id");

            // Fetch history for this card
            let records = sqlx::query(
                "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at"
            )
            .bind(&card_id)
            .bind(&self.user_id)
            .fetch_all(&self.pool)
            .await?;

            if records.is_empty() {
                continue;
            }

            // Replay: schedule each review in sequence
            let mut history: Vec<crate::srs::ReviewEvent> = Vec::new();
            let mut last_output = None;

            for rec in &records {
                let rating_u8 = rec.get::<i64, _>("rating") as u8;
                let rating = crate::srs::Rating::from_u8(rating_u8).unwrap_or(crate::srs::Rating::Again);
                let reviewed_at: i64 = rec.get("reviewed_at");

                let output = algorithm.schedule(&history, rating, reviewed_at);
                last_output = Some(output);

                history.push(crate::srs::ReviewEvent { rating, reviewed_at });
            }

            if let Some(output) = last_output {
                sqlx::query(
                    r#"
                    INSERT INTO reviews (card_id, user_id, due_date, interval_days)
                    VALUES (?, ?, ?, ?)
                    ON CONFLICT(card_id, user_id) DO UPDATE SET due_date = excluded.due_date, interval_days = excluded.interval_days
                    "#
                )
                .bind(&card_id)
                .bind(&self.user_id)
                .bind(output.due_date)
                .bind(output.interval_days)
                .execute(&mut *tx)
                .await?;
                count += 1;
            }
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Store generated cards as drafts (replaces in-memory Vec).
    pub async fn save_drafts(&self, cards: &[DraftCard]) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for card in cards {
            sqlx::query(
                "INSERT OR REPLACE INTO draft_cards (id, user_id, skill_id, skill_name, template_name, fields_json, explanation, metadata_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
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

    /// Fetch all draft cards for the current user
    pub async fn get_drafts(&self) -> Result<Vec<DraftCard>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT id, skill_id, skill_name, template_name, fields_json, explanation, metadata_json, created_at FROM draft_cards WHERE user_id = ? ORDER BY created_at"
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let drafts = records.into_iter().map(|r| {
            use sqlx::Row;
            DraftCard {
                id: r.get("id"),
                skill_id: r.get("skill_id"),
                skill_name: r.get("skill_name"),
                template_name: r.get("template_name"),
                fields_json: r.get("fields_json"),
                explanation: r.get("explanation"),
                metadata_json: r.get("metadata_json"),
                created_at: r.get("created_at"),
            }
        }).collect();

        Ok(drafts)
    }

    /// Clear all draft cards for the current user
    pub async fn clear_drafts(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM draft_cards WHERE user_id = ?")
            .bind(&self.user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete specific drafts by IDs (called after saving drafts to a real deck)
    pub async fn delete_drafts(&self, ids: &[String]) -> Result<(), sqlx::Error> {
        for chunk in ids.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "DELETE FROM draft_cards WHERE user_id = ? AND id IN ({})", placeholders
            );
            let mut q = sqlx::query(&sql).bind(&self.user_id);
            for id in chunk {
                q = q.bind(id);
            }
            q.execute(&self.pool).await?;
        }
        Ok(())
    }

    // ── Tree customizations ──────────────────────────────────────────

    /// Fetch all tree customizations for the current user and language.
    pub async fn get_tree_customizations(&self, language: &str) -> Result<Vec<UserTreeCustomization>, sqlx::Error> {
        let records = sqlx::query(
            "SELECT user_id, language, node_id, action, parent_id, node_name, node_instructions, sort_order, created_at \
             FROM user_tree_customizations WHERE user_id = ? AND language = ? ORDER BY sort_order, created_at"
        )
        .bind(&self.user_id)
        .bind(language)
        .fetch_all(&self.pool)
        .await?;

        let customizations = records.into_iter().map(|r| {
            use sqlx::Row;
            UserTreeCustomization {
                user_id: r.get("user_id"),
                language: r.get("language"),
                node_id: r.get("node_id"),
                action: r.get("action"),
                parent_id: r.get("parent_id"),
                node_name: r.get("node_name"),
                node_instructions: r.get("node_instructions"),
                sort_order: r.get("sort_order"),
                created_at: r.get("created_at"),
            }
        }).collect();

        Ok(customizations)
    }

    /// Insert or replace a tree customization for the current user.
    pub async fn upsert_tree_customization(&self, c: &UserTreeCustomization) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO user_tree_customizations \
             (user_id, language, node_id, action, parent_id, node_name, node_instructions, sort_order, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&self.user_id)
        .bind(&c.language)
        .bind(&c.node_id)
        .bind(&c.action)
        .bind(&c.parent_id)
        .bind(&c.node_name)
        .bind(&c.node_instructions)
        .bind(c.sort_order)
        .bind(c.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a single tree customization by node_id.
    pub async fn delete_tree_customization(&self, language: &str, node_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM user_tree_customizations WHERE user_id = ? AND language = ? AND node_id = ?"
        )
        .bind(&self.user_id)
        .bind(language)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait::async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn fetch_cards(&self) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>> {
        let records = sqlx::query(
            r#"
            SELECT
                c.id as card_id,
                c.front_html,
                c.metadata_json,
                r.interval_days,
                COALESCE(lapse_counts.cnt, 0) as lapses
            FROM cards c
            JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
            LEFT JOIN (
                SELECT card_id, COUNT(*) as cnt
                FROM review_log
                WHERE rating = 1 AND user_id = ?
                GROUP BY card_id
            ) lapse_counts ON c.id = lapse_counts.card_id
            "#
        )
        .bind(&self.user_id)
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut cards = Vec::new();
        for rec in records {
            use sqlx::Row;
            let card_id: String = rec.get("card_id");
            let metadata: String = rec.get("metadata_json");
            let fields = format!("{}\x1f{}", rec.get::<String, _>("front_html"), metadata);
            let interval_days: f64 = rec.get("interval_days");
            let lapses: i64 = rec.get("lapses");

            cards.push(StoredCard {
                note_id: card_id.clone(),
                card_id,
                fields,
                tags: String::new(),
                interval_days,
                lapses: lapses as i32,
            });
        }

        Ok(cards)
    }

    async fn fetch_decks(&self) -> Result<Vec<DeckInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let settings = self.get_user_settings().await.unwrap_or_default();
        let learn_ahead_ms = (settings.learn_ahead_minutes.get() as i64) * 60 * 1000;
        let now = Utc::now().timestamp_millis() + learn_ahead_ms;
        
        let records = sqlx::query(
            r#"
            WITH review_counts AS (
                SELECT card_id, COUNT(*) as review_count
                FROM review_log
                WHERE user_id = ?
                GROUP BY card_id
            )
            SELECT
                d.id,
                d.full_path,
                COUNT(c.id) as card_count,
                SUM(CASE WHEN r.interval_days = 0
                     AND COALESCE(rc.review_count, 0) = 0
                     THEN 1 ELSE 0 END) as new_count,
                SUM(CASE WHEN ((r.interval_days > 0 AND r.interval_days < 1)
                     OR (r.interval_days = 0 AND COALESCE(rc.review_count, 0) > 0))
                     AND r.due_date <= ?
                     THEN 1 ELSE 0 END) as learning_count,
                SUM(CASE WHEN r.interval_days >= 1 AND r.due_date <= ?
                     THEN 1 ELSE 0 END) as review_count
            FROM decks d
            LEFT JOIN cards c ON d.id = c.deck_id
            LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
            LEFT JOIN review_counts rc ON c.id = rc.card_id
            WHERE d.user_id = ?
            GROUP BY d.id
            "#
        )
        .bind(&self.user_id) // CTE
        .bind(now)
        .bind(now)
        .bind(&self.user_id) // JOIN
        .bind(&self.user_id) // WHERE
        .fetch_all(&self.pool)
        .await?;

        let decks = records.into_iter().map(|r| {
            use sqlx::Row;
            DeckInfo {
                deck_id: r.get("id"),
                name: r.get("full_path"), // We use full path internally for names representing hierarchy
                card_count: r.get::<i64, _>("card_count") as usize,
                new_count: r.get::<Option<i64>, _>("new_count").unwrap_or(0) as usize,
                learning_count: r.get::<Option<i64>, _>("learning_count").unwrap_or(0) as usize,
                review_count: r.get::<Option<i64>, _>("review_count").unwrap_or(0) as usize,
                is_lc: true, // Native DB decks are always LC decks
            }
        }).collect();

        Ok(decks)
    }

    async fn save_deck(&self, deck_data: &NewDeckData) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let now = Utc::now().timestamp_millis();
        let mut added = 0;
        let mut tx = self.pool.begin().await?;

        // Create hierarchy inside the transaction so it rolls back with card inserts on failure
        let deck_id = self.get_or_create_deck_hierarchy(&deck_data.name, &deck_data.language_code, &mut tx).await?;

        for entry in &deck_data.cards {
            let card_id = Uuid::new_v4().to_string();
            
            // Insert Card
            sqlx::query(
                "INSERT INTO cards (id, deck_id, skill_id, template_name, front_html, back_html, fields_json, metadata_json, audio_path, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&card_id)
            .bind(&deck_id)
            .bind(&entry.skill_name)
            .bind(&entry.template_name)
            .bind(&entry.front_html)
            .bind(&entry.back_html)
            .bind(&entry.fields_json)
            .bind(&entry.metadata_json)
            .bind(&entry.audio_path)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            // Initialize scheduling cache (new card, due immediately)
            sqlx::query(
                "INSERT INTO reviews (card_id, user_id, due_date, interval_days) VALUES (?, ?, ?, 0)"
            )
            .bind(&card_id)
            .bind(&self.user_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            added += 1;
        }

        tx.commit().await?;

        Ok(added)
    }

    async fn delete_deck(&self, deck_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM decks WHERE id = ? AND user_id = ?")
            .bind(deck_id)
            .bind(&self.user_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
