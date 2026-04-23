use chrono::Utc;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::path::Path;
use uuid::Uuid; // TODO check plus précisément

use crate::sanitize::escape_html;
use crate::storage::{DeckInfo, NewCardEntry, NewDeckData, StorageProvider, StoredCard};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyMode {
    Preview,
    Review,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckCountsRecord {
    pub total_cards: usize,
    pub due_new_cards: usize,
    pub due_learning_cards: usize,
    pub due_review_cards: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckSummaryRecord {
    pub id: String,
    pub parent_deck_id: Option<String>,
    pub name: String,
    pub full_path: String,
    pub target_language_iso: String,
    pub counts: DeckCountsRecord,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StudyCardRecord {
    pub card_id: String,
    pub deck_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub template_name: String,
    pub front_html: String,
    pub back_html: String,
    pub explanation_html: String,
    pub audio_path: Option<String>,
}

/// A per-user customization applied on top of the base YAML skill tree.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UserTreeCustomization {
    pub user_id: String,
    pub tree_definition_id: String,
    pub node_id: String,
    pub action: String, // "add" | "hide" | "edit"
    pub parent_id: Option<String>,
    pub node_name: Option<String>,
    pub node_instructions: Option<String>,
    /// JSON-encoded `Vec<String>` of prerequisite node IDs.
    /// None = field not set (for `edit`, preserves existing base-tree prereqs).
    /// Some("[]") = empty list (clears prereqs for `edit`).
    pub prerequisites_json: Option<String>,
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

fn explanation_html_from_metadata(metadata_json: &str) -> String {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or_default();
    metadata
        .get("pedagogical_explanation")
        .and_then(|value| value.as_str())
        .map(|value| escape_html(value).replace('\n', "<br>"))
        .unwrap_or_default()
}

fn ipa_from_metadata(metadata_json: &str) -> String {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or_default();
    metadata
        .get("ipa")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string()
}

/// A SQLite-backed implementation of the `StorageProvider` trait for local studies.
pub struct LocalStorageProvider {
    pub pool: SqlitePool,
    pub user_id: String,
}

impl LocalStorageProvider {
    /// Creates a provider scoped to a specific user (from JWT `sub` claim).
    pub const fn for_user(pool: SqlitePool, user_id: String) -> Self {
        Self { pool, user_id }
    }

    /// Auto-provision user from JWT claims. Provider-agnostic:
    /// works with email, GitHub, Google, phone, anonymous sign-in.
    ///
    /// # Errors
    /// Returns a database error if the user cannot be provisioned.
    pub async fn ensure_user(&self, claims: &serde_json::Value) -> Result<(), sqlx::Error> {
        let display_name = claims["user_metadata"]["full_name"]
            .as_str()
            .or_else(|| claims["user_metadata"]["preferred_username"].as_str())
            .or_else(|| claims["email"].as_str().and_then(|e| e.split('@').next()))
            .unwrap_or("user");

        let email = claims["email"].as_str();

        sqlx::query(
            "INSERT INTO users (id, display_name, email, settings, created_at)
             VALUES (?, ?, ?, '{}', ?)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, email = COALESCE(excluded.email, email)",
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
    ///
    /// # Errors
    /// Returns a database error if the connection fails or migrations fail.
    pub async fn init(db_path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
        let db_path = db_path.as_ref();

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
        }

        let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    use sqlx::Executor;
                    conn.execute("PRAGMA foreign_keys = ON").await?;
                    conn.execute("PRAGMA journal_mode = WAL").await?;
                    Ok(())
                })
            })
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
        let query = "INSERT INTO users (id, display_name, email, settings, created_at) VALUES (?, ?, NULL, '{}', ?) ON CONFLICT(id) DO NOTHING";
        sqlx::query(query)
            .bind(&self.user_id)
            .bind("Default User")
            .bind(now_ms())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

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
        let now = now_ms();

        for part in parts {
            if !current_path.is_empty() {
                current_path.push_str("::");
            }
            current_path.push_str(part);

            let deck_id = Uuid::new_v4().to_string();
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

    async fn compute_scheduling_output(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        let history = self.get_review_history(card_id).await?;
        Ok(algorithm.schedule(&history, rating, now))
    }

    /// Fetch user settings from the database.
    pub async fn get_user_settings(&self) -> Result<crate::user::UserSettings, sqlx::Error> {
        use sqlx::Row;

        let row = sqlx::query("SELECT settings FROM users WHERE id = ?")
            .bind(&self.user_id)
            .fetch_one(&self.pool)
            .await?;

        let settings_json: String = row.get("settings");
        let settings = serde_json::from_str(&settings_json).unwrap_or_default();
        Ok(settings)
    }

    /// Update user settings.
    pub async fn update_user_settings(
        &self,
        settings: &crate::user::UserSettings,
    ) -> Result<(), sqlx::Error> {
        let settings_json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".to_string());
        sqlx::query("UPDATE users SET settings = ? WHERE id = ?")
            .bind(settings_json)
            .bind(&self.user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    pub async fn verify_card_ownership(&self, card_id: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cards c JOIN decks d ON c.deck_id = d.id WHERE c.id = ? AND d.user_id = ?",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn list_deck_summaries(&self) -> Result<Vec<DeckSummaryRecord>, sqlx::Error> {
        use sqlx::Row;

        let settings = self.get_user_settings().await.unwrap_or_default();
        let learn_ahead_ms = i64::from(settings.learn_ahead_minutes.get()) * 60 * 1000;
        let due_cutoff = now_ms() + learn_ahead_ms;

        let records = sqlx::query(
            r#"
            WITH RECURSIVE deck_closure(ancestor_id, descendant_id) AS (
                SELECT id, id
                FROM decks
                WHERE user_id = ?
                UNION ALL
                SELECT dc.ancestor_id, d.id
                FROM deck_closure dc
                JOIN decks d ON d.parent_id = dc.descendant_id
                WHERE d.user_id = ?
            ),
            review_counts AS (
                SELECT card_id, COUNT(*) as review_count
                FROM review_log
                WHERE user_id = ?
                GROUP BY card_id
            )
            SELECT
                d.id,
                d.parent_id,
                d.name,
                d.full_path,
                COALESCE(d.target_language, '') as target_language,
                COUNT(c.id) as total_cards,
                SUM(CASE
                    WHEN r.due_date <= ?
                     AND r.interval_days = 0
                     AND COALESCE(rc.review_count, 0) = 0
                    THEN 1 ELSE 0
                END) as due_new_cards,
                SUM(CASE
                    WHEN ((r.interval_days > 0 AND r.interval_days < 1)
                      OR (r.interval_days = 0 AND COALESCE(rc.review_count, 0) > 0))
                     AND r.due_date <= ?
                    THEN 1 ELSE 0
                END) as due_learning_cards,
                SUM(CASE
                    WHEN r.interval_days >= 1
                     AND r.due_date <= ?
                    THEN 1 ELSE 0
                END) as due_review_cards
            FROM decks d
            LEFT JOIN deck_closure dc ON dc.ancestor_id = d.id
            LEFT JOIN cards c ON c.deck_id = dc.descendant_id
            LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
            LEFT JOIN review_counts rc ON c.id = rc.card_id
            WHERE d.user_id = ?
            GROUP BY d.id
            ORDER BY d.full_path
            "#,
        )
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

        Ok(records
            .into_iter()
            .map(|row| DeckSummaryRecord {
                id: row.get("id"),
                parent_deck_id: row.get("parent_id"),
                name: row.get("name"),
                full_path: row.get("full_path"),
                target_language_iso: row.get("target_language"),
                counts: DeckCountsRecord {
                    total_cards: row.get::<i64, _>("total_cards") as usize,
                    due_new_cards: row.get::<Option<i64>, _>("due_new_cards").unwrap_or(0)
                        as usize,
                    due_learning_cards: row
                        .get::<Option<i64>, _>("due_learning_cards")
                        .unwrap_or(0) as usize,
                    due_review_cards: row.get::<Option<i64>, _>("due_review_cards").unwrap_or(0)
                        as usize,
                },
            })
            .collect())
    }

    pub async fn get_deck_summary(
        &self,
        deck_id: &str,
    ) -> Result<Option<DeckSummaryRecord>, sqlx::Error> {
        use sqlx::Row;

        let settings = self.get_user_settings().await.unwrap_or_default();
        let learn_ahead_ms = i64::from(settings.learn_ahead_minutes.get()) * 60 * 1000;
        let due_cutoff = now_ms() + learn_ahead_ms;

        let record = sqlx::query(
            r#"
            WITH RECURSIVE deck_closure(ancestor_id, descendant_id) AS (
                SELECT id, id
                FROM decks
                WHERE user_id = ?
                UNION ALL
                SELECT dc.ancestor_id, d.id
                FROM deck_closure dc
                JOIN decks d ON d.parent_id = dc.descendant_id
                WHERE d.user_id = ?
            ),
            review_counts AS (
                SELECT card_id, COUNT(*) as review_count
                FROM review_log
                WHERE user_id = ?
                GROUP BY card_id
            )
            SELECT
                d.id,
                d.parent_id,
                d.name,
                d.full_path,
                COALESCE(d.target_language, '') as target_language,
                COUNT(c.id) as total_cards,
                SUM(CASE
                    WHEN r.due_date <= ?
                     AND r.interval_days = 0
                     AND COALESCE(rc.review_count, 0) = 0
                    THEN 1 ELSE 0
                END) as due_new_cards,
                SUM(CASE
                    WHEN ((r.interval_days > 0 AND r.interval_days < 1)
                      OR (r.interval_days = 0 AND COALESCE(rc.review_count, 0) > 0))
                     AND r.due_date <= ?
                    THEN 1 ELSE 0
                END) as due_learning_cards,
                SUM(CASE
                    WHEN r.interval_days >= 1
                     AND r.due_date <= ?
                    THEN 1 ELSE 0
                END) as due_review_cards
            FROM decks d
            LEFT JOIN deck_closure dc ON dc.ancestor_id = d.id
            LEFT JOIN cards c ON c.deck_id = dc.descendant_id
            LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
            LEFT JOIN review_counts rc ON c.id = rc.card_id
            WHERE d.user_id = ? AND d.id = ?
            GROUP BY d.id
            "#,
        )
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

        Ok(record.map(|row| DeckSummaryRecord {
            id: row.get("id"),
            parent_deck_id: row.get("parent_id"),
            name: row.get("name"),
            full_path: row.get("full_path"),
            target_language_iso: row.get("target_language"),
            counts: DeckCountsRecord {
                total_cards: row.get::<i64, _>("total_cards") as usize,
                due_new_cards: row.get::<Option<i64>, _>("due_new_cards").unwrap_or(0) as usize,
                due_learning_cards: row.get::<Option<i64>, _>("due_learning_cards").unwrap_or(0)
                    as usize,
                due_review_cards: row.get::<Option<i64>, _>("due_review_cards").unwrap_or(0)
                    as usize,
            },
        }))
    }

    pub async fn select_study_cards(
        &self,
        deck_id: &str,
        mode: StudyMode,
        limit: i64,
    ) -> Result<Vec<StudyCardRecord>, sqlx::Error> {
        use sqlx::Row;

        let settings = self.get_user_settings().await.unwrap_or_default();
        let learn_ahead_ms = i64::from(settings.learn_ahead_minutes.get()) * 60 * 1000;
        let due_cutoff = now_ms() + learn_ahead_ms;

        let query = match mode {
            StudyMode::Preview => {
                r#"
                WITH RECURSIVE deck_tree(id) AS (
                    SELECT id FROM decks WHERE id = ? AND user_id = ?
                    UNION ALL
                    SELECT d.id FROM decks d JOIN deck_tree dt ON d.parent_id = dt.id WHERE d.user_id = ?
                )
                SELECT
                    c.id,
                    c.deck_id,
                    COALESCE(c.skill_id, '') as skill_id,
                    COALESCE(c.skill_name, '') as skill_name,
                    COALESCE(c.template_name, '') as template_name,
                    c.front_html,
                    c.back_html,
                    c.metadata_json,
                    c.audio_path
                FROM cards c
                LEFT JOIN reviews r ON c.id = r.card_id AND r.user_id = ?
                WHERE c.deck_id IN deck_tree
                ORDER BY COALESCE(r.due_date, 0) ASC, c.created_at ASC
                LIMIT ?
                "#
            }
            StudyMode::Review => {
                r#"
                WITH RECURSIVE deck_tree(id) AS (
                    SELECT id FROM decks WHERE id = ? AND user_id = ?
                    UNION ALL
                    SELECT d.id FROM decks d JOIN deck_tree dt ON d.parent_id = dt.id WHERE d.user_id = ?
                )
                SELECT
                    c.id,
                    c.deck_id,
                    COALESCE(c.skill_id, '') as skill_id,
                    COALESCE(c.skill_name, '') as skill_name,
                    COALESCE(c.template_name, '') as template_name,
                    c.front_html,
                    c.back_html,
                    c.metadata_json,
                    c.audio_path
                FROM cards c
                JOIN reviews r ON c.id = r.card_id
                WHERE c.deck_id IN deck_tree
                  AND r.user_id = ?
                  AND r.due_date <= ?
                ORDER BY r.due_date ASC, c.created_at ASC
                LIMIT ?
                "#
            }
        };

        let mut sql = sqlx::query(query)
            .bind(deck_id)
            .bind(&self.user_id)
            .bind(&self.user_id)
            .bind(&self.user_id);

        if mode == StudyMode::Review {
            sql = sql.bind(due_cutoff);
        }

        let records = sql.bind(limit).fetch_all(&self.pool).await?;

        Ok(records
            .into_iter()
            .map(|row| {
                let metadata_json: String = row.get("metadata_json");
                StudyCardRecord {
                    card_id: row.get("id"),
                    deck_id: row.get("deck_id"),
                    skill_id: row.get("skill_id"),
                    skill_name: row.get("skill_name"),
                    template_name: row.get("template_name"),
                    front_html: row.get("front_html"),
                    back_html: row.get("back_html"),
                    explanation_html: explanation_html_from_metadata(&metadata_json),
                    audio_path: row.get("audio_path"),
                }
            })
            .collect())
    }

    pub async fn fetch_decks_for_export(&self) -> Result<Vec<NewDeckData>, sqlx::Error> {
        use sqlx::Row;
        use std::collections::BTreeMap;

        let records = sqlx::query(
            r#"
            SELECT
                d.full_path,
                d.target_language,
                c.front_html,
                c.back_html,
                c.skill_id,
                c.skill_name,
                c.template_name,
                c.metadata_json,
                c.audio_path,
                c.fields_json
            FROM cards c
            JOIN decks d ON c.deck_id = d.id
            WHERE d.user_id = ?
            ORDER BY d.full_path, c.created_at
            "#,
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut decks_map: BTreeMap<String, NewDeckData> = BTreeMap::new();

        for rec in records {
            let full_path: String = rec.get("full_path");
            let language_code: String = rec
                .get::<Option<String>, _>("target_language")
                .unwrap_or_default();
            let metadata_json: String = rec.get("metadata_json");

            let entry = NewCardEntry {
                front_html: rec.get("front_html"),
                back_html: rec.get("back_html"),
                skill_id: rec.get::<Option<String>, _>("skill_id").unwrap_or_default(),
                skill_name: rec.get::<Option<String>, _>("skill_name").unwrap_or_default(),
                template_name: rec.get::<Option<String>, _>("template_name").unwrap_or_default(),
                fields_json: rec
                    .get::<Option<String>, _>("fields_json")
                    .unwrap_or_else(|| "{}".to_string()),
                explanation: explanation_html_from_metadata(&metadata_json),
                ipa: ipa_from_metadata(&metadata_json),
                metadata_json,
                audio_path: rec.get("audio_path"),
            };

            decks_map
                .entry(full_path.clone())
                .or_insert_with(|| NewDeckData {
                    name: full_path,
                    language_code: language_code.clone(),
                    cards: Vec::new(),
                })
                .cards
                .push(entry);
        }

        Ok(decks_map.into_values().collect())
    }

    pub async fn preview_rating(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        let output = self
            .compute_scheduling_output(card_id, rating, algorithm, now)
            .await?;

        sqlx::query(
            "INSERT INTO practice_log (card_id, user_id, rating, practiced_at) VALUES (?, ?, ?, ?)",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(i64::from(rating as u8))
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(output)
    }

    pub async fn submit_review(
        &self,
        card_id: &str,
        rating: crate::srs::Rating,
        algorithm: &dyn crate::srs::SrsAlgorithm,
        now: i64,
    ) -> Result<crate::srs::SchedulingOutput, sqlx::Error> {
        let output = self
            .compute_scheduling_output(card_id, rating, algorithm, now)
            .await?;

        sqlx::query(
            "INSERT INTO review_log (card_id, user_id, rating, reviewed_at) VALUES (?, ?, ?, ?)",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(i64::from(rating as u8))
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO reviews (card_id, user_id, due_date, interval_days)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(card_id, user_id) DO UPDATE
            SET due_date = excluded.due_date, interval_days = excluded.interval_days
            "#,
        )
        .bind(card_id)
        .bind(&self.user_id)
        .bind(output.due_date)
        .bind(output.interval_days)
        .execute(&self.pool)
        .await?;

        Ok(output)
    }

    pub async fn get_review_history(
        &self,
        card_id: &str,
    ) -> Result<Vec<crate::srs::ReviewEvent>, sqlx::Error> {
        use sqlx::Row;

        let records = sqlx::query(
            "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at",
        )
        .bind(card_id)
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| {
                let rating_u8 = record.get::<i64, _>("rating") as u8;
                crate::srs::ReviewEvent {
                    rating: crate::srs::Rating::from_u8(rating_u8)
                        .unwrap_or(crate::srs::Rating::Again),
                    reviewed_at: record.get("reviewed_at"),
                }
            })
            .collect())
    }

    pub async fn rebuild_scheduling_cache(
        &self,
        algorithm: &dyn crate::srs::SrsAlgorithm,
    ) -> Result<usize, sqlx::Error> {
        use sqlx::Row;

        let card_rows = sqlx::query("SELECT DISTINCT card_id FROM review_log WHERE user_id = ?")
            .bind(&self.user_id)
            .fetch_all(&self.pool)
            .await?;

        let mut count = 0;
        let mut tx = self.pool.begin().await?;

        for row in &card_rows {
            let card_id: String = row.get("card_id");
            let records = sqlx::query(
                "SELECT rating, reviewed_at FROM review_log WHERE card_id = ? AND user_id = ? ORDER BY reviewed_at",
            )
            .bind(&card_id)
            .bind(&self.user_id)
            .fetch_all(&self.pool)
            .await?;

            if records.is_empty() {
                continue;
            }

            let mut history: Vec<crate::srs::ReviewEvent> = Vec::new();
            let mut last_output = None;

            for record in &records {
                let rating_u8 = record.get::<i64, _>("rating") as u8;
                let rating =
                    crate::srs::Rating::from_u8(rating_u8).unwrap_or(crate::srs::Rating::Again);
                let reviewed_at: i64 = record.get("reviewed_at");
                let output = algorithm.schedule(&history, rating, reviewed_at);
                last_output = Some(output);
                history.push(crate::srs::ReviewEvent {
                    rating,
                    reviewed_at,
                });
            }

            if let Some(output) = last_output {
                sqlx::query(
                    r#"
                    INSERT INTO reviews (card_id, user_id, due_date, interval_days)
                    VALUES (?, ?, ?, ?)
                    ON CONFLICT(card_id, user_id) DO UPDATE
                    SET due_date = excluded.due_date, interval_days = excluded.interval_days
                    "#,
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
        use sqlx::Row;

        let records = sqlx::query(
            "SELECT id, skill_id, skill_name, template_name, fields_json, explanation, metadata_json, created_at FROM draft_cards WHERE user_id = ? ORDER BY created_at",
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| DraftCard {
                id: record.get("id"),
                skill_id: record.get("skill_id"),
                skill_name: record.get("skill_name"),
                template_name: record.get("template_name"),
                fields_json: record.get("fields_json"),
                explanation: record.get("explanation"),
                metadata_json: record.get("metadata_json"),
                created_at: record.get("created_at"),
            })
            .collect())
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
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "DELETE FROM draft_cards WHERE user_id = ? AND id IN ({})",
                placeholders
            );
            let mut query = sqlx::query(&sql).bind(&self.user_id);
            for id in chunk {
                query = query.bind(id);
            }
            query.execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn get_tree_customizations(
        &self,
        tree_definition_id: &str,
    ) -> Result<Vec<UserTreeCustomization>, sqlx::Error> {
        use sqlx::Row;

        let records = sqlx::query(
            "SELECT user_id, tree_definition_id, node_id, action, parent_id, node_name, node_instructions, prerequisites_json, sort_order, created_at \
             FROM user_tree_customizations WHERE user_id = ? AND tree_definition_id = ? ORDER BY sort_order, created_at",
        )
        .bind(&self.user_id)
        .bind(tree_definition_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| UserTreeCustomization {
                user_id: record.get("user_id"),
                tree_definition_id: record.get("tree_definition_id"),
                node_id: record.get("node_id"),
                action: record.get("action"),
                parent_id: record.get("parent_id"),
                node_name: record.get("node_name"),
                node_instructions: record.get("node_instructions"),
                prerequisites_json: record.get("prerequisites_json"),
                sort_order: record.get("sort_order"),
                created_at: record.get("created_at"),
            })
            .collect())
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

#[async_trait::async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn fetch_cards(
        &self,
    ) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>> {
        use sqlx::Row;

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
            "#,
        )
        .bind(&self.user_id)
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|record| {
                let card_id: String = record.get("card_id");
                let metadata: String = record.get("metadata_json");
                let fields =
                    format!("{}\x1f{}", record.get::<String, _>("front_html"), metadata);
                StoredCard {
                    note_id: card_id.clone(),
                    card_id,
                    fields,
                    tags: String::new(),
                    interval_days: record.get("interval_days"),
                    lapses: record.get::<i64, _>("lapses") as i32,
                }
            })
            .collect())
    }

    async fn fetch_decks(&self) -> Result<Vec<DeckInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let summaries = self.list_deck_summaries().await?;

        Ok(summaries
            .into_iter()
            .map(|summary| DeckInfo {
                deck_id: summary.id,
                name: summary.full_path,
                target_language: summary.target_language_iso,
                card_count: summary.counts.total_cards,
                new_count: summary.counts.due_new_cards,
                learning_count: summary.counts.due_learning_cards,
                review_count: summary.counts.due_review_cards,
                is_lc: true,
            })
            .collect())
    }

    async fn save_deck(
        &self,
        deck_data: &NewDeckData,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut added = 0;
        let mut tx = self.pool.begin().await?;
        let deck_id = self
            .get_or_create_deck_hierarchy(&deck_data.name, &deck_data.language_code, &mut tx)
            .await?;
        let now = now_ms();

        for entry in &deck_data.cards {
            let card_id = Uuid::new_v4().to_string();

            sqlx::query(
                "INSERT INTO cards (id, deck_id, skill_id, skill_name, template_name, front_html, back_html, fields_json, metadata_json, audio_path, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&card_id)
            .bind(&deck_id)
            .bind(&entry.skill_id)
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

            sqlx::query(
                "INSERT INTO reviews (card_id, user_id, due_date, interval_days) VALUES (?, ?, ?, 0)",
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

    async fn delete_deck(
        &self,
        deck_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::srs::{Rating, SrsRegistry};
    use crate::storage::StorageProvider;
    use sqlx::Row;

    fn temp_db_path(test_name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("panglot_{test_name}_{}.sqlite", Uuid::new_v4()))
    }

    async fn init_provider(test_name: &str) -> LocalStorageProvider {
        LocalStorageProvider::init(temp_db_path(test_name))
            .await
            .expect("provider should initialize")
    }

    fn test_deck(name: &str, skill_id: &str, skill_name: &str) -> NewDeckData {
        NewDeckData {
            name: name.to_string(),
            language_code: "pol".to_string(),
            cards: vec![NewCardEntry {
                front_html: format!("<div>{name}</div>"),
                back_html: "<div>answer</div>".to_string(),
                skill_id: skill_id.to_string(),
                skill_name: skill_name.to_string(),
                template_name: "Recognition".to_string(),
                fields_json: r#"{"front":"x","back":"y"}"#.to_string(),
                explanation: "Explanation".to_string(),
                ipa: "/ipa/".to_string(),
                metadata_json: serde_json::json!({
                    "pedagogical_explanation": format!("Pedagogical {skill_name}"),
                    "ipa": "/ipa/"
                })
                .to_string(),
                audio_path: Some("audio/test.mp3".to_string()),
            }],
        }
    }

    async fn deck_id_for_path(provider: &LocalStorageProvider, full_path: &str) -> String {
        sqlx::query("SELECT id FROM decks WHERE user_id = ? AND full_path = ?")
            .bind(&provider.user_id)
            .bind(full_path)
            .fetch_one(&provider.pool)
            .await
            .expect("deck should exist")
            .get("id")
    }

    async fn first_card_id_for_path(provider: &LocalStorageProvider, full_path: &str) -> String {
        sqlx::query(
            "SELECT c.id FROM cards c JOIN decks d ON c.deck_id = d.id WHERE d.user_id = ? AND d.full_path = ? ORDER BY c.created_at LIMIT 1",
        )
        .bind(&provider.user_id)
        .bind(full_path)
        .fetch_one(&provider.pool)
        .await
        .expect("card should exist")
        .get("id")
    }

    #[tokio::test]
    async fn list_deck_summaries_aggregates_descendant_counts() {
        let provider = init_provider("deck_summaries").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");
        provider
            .save_deck(&test_deck("Vocabulary::Months", "skill-months", "Months"))
            .await
            .expect("deck should save");

        let summaries = provider
            .list_deck_summaries()
            .await
            .expect("summaries should load");

        let parent = summaries
            .iter()
            .find(|summary| summary.full_path == "Vocabulary")
            .expect("parent deck should exist");
        assert_eq!(parent.counts.total_cards, 2);
        assert_eq!(parent.counts.due_new_cards, 2);
        assert_eq!(parent.counts.due_learning_cards, 0);
        assert_eq!(parent.counts.due_review_cards, 0);
    }

    #[tokio::test]
    async fn select_study_cards_recurses_for_preview_and_review() {
        let provider = init_provider("study_cards").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");
        provider
            .save_deck(&test_deck("Vocabulary::Months", "skill-months", "Months"))
            .await
            .expect("deck should save");

        let parent_id = deck_id_for_path(&provider, "Vocabulary").await;

        let preview_cards = provider
            .select_study_cards(&parent_id, StudyMode::Preview, 20)
            .await
            .expect("preview cards should load");
        let review_cards = provider
            .select_study_cards(&parent_id, StudyMode::Review, 20)
            .await
            .expect("review cards should load");

        assert_eq!(preview_cards.len(), 2);
        assert_eq!(review_cards.len(), 2);
        assert!(
            preview_cards
                .iter()
                .all(|card| !card.explanation_html.is_empty())
        );
    }

    #[tokio::test]
    async fn preview_rating_writes_only_practice_log() {
        let provider = init_provider("preview_rating").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");

        let card_id = first_card_id_for_path(&provider, "Vocabulary::Family").await;
        let before_interval: f64 = sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
            .bind(&card_id)
            .bind(&provider.user_id)
            .fetch_one(&provider.pool)
            .await
            .expect("review cache should exist")
            .get("interval_days");

        let algorithm = SrsRegistry::new();
        let output = provider
            .preview_rating(&card_id, Rating::Easy, algorithm.default(), 1_700_000_000_000)
            .await
            .expect("preview rating should succeed");

        let practice_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM practice_log WHERE card_id = ? AND user_id = ?",
        )
        .bind(&card_id)
        .bind(&provider.user_id)
        .fetch_one(&provider.pool)
        .await
        .expect("practice count should load");
        let review_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_log WHERE card_id = ? AND user_id = ?",
        )
        .bind(&card_id)
        .bind(&provider.user_id)
        .fetch_one(&provider.pool)
        .await
        .expect("review count should load");
        let after_interval: f64 = sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
            .bind(&card_id)
            .bind(&provider.user_id)
            .fetch_one(&provider.pool)
            .await
            .expect("review cache should exist")
            .get("interval_days");

        assert_eq!(practice_count, 1);
        assert_eq!(review_count, 0);
        assert_eq!(before_interval, after_interval);
        assert!(output.interval_days >= 1.0);
    }

    #[tokio::test]
    async fn review_rating_updates_review_log_and_cache() {
        let provider = init_provider("review_rating").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");

        let card_id = first_card_id_for_path(&provider, "Vocabulary::Family").await;
        let algorithm = SrsRegistry::new();

        provider
            .submit_review(&card_id, Rating::Easy, algorithm.default(), 1_700_000_000_000)
            .await
            .expect("review should succeed");

        let practice_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM practice_log WHERE card_id = ? AND user_id = ?",
        )
        .bind(&card_id)
        .bind(&provider.user_id)
        .fetch_one(&provider.pool)
        .await
        .expect("practice count should load");
        let review_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_log WHERE card_id = ? AND user_id = ?",
        )
        .bind(&card_id)
        .bind(&provider.user_id)
        .fetch_one(&provider.pool)
        .await
        .expect("review count should load");
        let interval_days: f64 =
            sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
                .bind(&card_id)
                .bind(&provider.user_id)
                .fetch_one(&provider.pool)
                .await
                .expect("review cache should exist")
                .get("interval_days");

        assert_eq!(practice_count, 0);
        assert_eq!(review_count, 1);
        assert!(interval_days >= 1.0);
    }

    #[tokio::test]
    async fn save_deck_preserves_skill_identity() {
        let provider = init_provider("skill_identity").await;
        provider
            .save_deck(&test_deck(
                "Grammar::Accusative",
                "skill-accusative",
                "Accusative",
            ))
            .await
            .expect("deck should save");

        let row = sqlx::query(
            "SELECT c.skill_id, c.skill_name FROM cards c JOIN decks d ON c.deck_id = d.id WHERE d.user_id = ? AND d.full_path = ?",
        )
        .bind(&provider.user_id)
        .bind("Grammar::Accusative")
        .fetch_one(&provider.pool)
        .await
        .expect("card should exist");
        let deck_id = deck_id_for_path(&provider, "Grammar::Accusative").await;
        let study_cards = provider
            .select_study_cards(&deck_id, StudyMode::Preview, 20)
            .await
            .expect("study cards should load");

        assert_eq!(row.get::<Option<String>, _>("skill_id").unwrap_or_default(), "skill-accusative");
        assert_eq!(row.get::<String, _>("skill_name"), "Accusative");
        assert_eq!(study_cards[0].skill_id, "skill-accusative");
        assert_eq!(study_cards[0].skill_name, "Accusative");
    }
}
