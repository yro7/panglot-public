use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, sqlite::SqlitePoolOptions};
use std::path::Path;
use uuid::Uuid;

use crate::storage::{DeckInfo, NewDeckData, StorageProvider, StoredCard};

mod cards;
mod customizations;
mod decks;
mod drafts;
mod export;
mod sql;
mod srs_ops;
mod types;

pub use types::{
    DeckCountsRecord, DeckSummaryRecord, DraftCard, StudyCardRecord, StudyMode,
    UserTreeCustomization,
};

use types::{DEFAULT_USER_ID, now_ms};

/// A SQLite-backed implementation of the `StorageProvider` trait for local studies.
pub struct LocalStorageProvider {
    pub pool: SqlitePool,
    pub user_id: String,
    pub settings_defaults: crate::user::UserSettings,
}

impl LocalStorageProvider {
    /// Creates a provider scoped to a specific user (from JWT `sub` claim).
    pub fn for_user(
        pool: SqlitePool,
        user_id: String,
        settings_defaults: crate::user::UserSettings,
    ) -> Self {
        Self {
            pool,
            user_id,
            settings_defaults,
        }
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

        let settings_json = serde_json::to_string(&self.settings_defaults)
            .expect("default user settings should serialize");

        sqlx::query(
            "INSERT INTO users (id, display_name, email, settings, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, email = COALESCE(excluded.email, email)",
        )
        .bind(&self.user_id)
        .bind(display_name)
        .bind(email)
        .bind(settings_json)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Initializes the database connection and runs migrations.
    ///
    /// # Errors
    /// Returns a database error if the connection fails or migrations fail.
    pub async fn init(
        db_path: impl AsRef<Path>,
        settings_defaults: crate::user::UserSettings,
    ) -> Result<Self, sqlx::Error> {
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
                    conn.execute("PRAGMA synchronous = NORMAL").await?;
                    conn.execute("PRAGMA busy_timeout = 5000").await?;
                    conn.execute("PRAGMA cache_size = -20000").await?;
                    conn.execute("PRAGMA temp_store = MEMORY").await?;
                    Ok(())
                })
            })
            .connect(&database_url)
            .await?;

        let provider = Self {
            pool,
            user_id: DEFAULT_USER_ID.to_string(),
            settings_defaults,
        };

        sqlx::migrate!("./migrations")
            .run(&provider.pool)
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Migration failed: {e}")))?;

        provider.ensure_default_user().await?;
        Ok(provider)
    }

    async fn ensure_default_user(&self) -> Result<(), sqlx::Error> {
        let settings_json = serde_json::to_string(&self.settings_defaults)
            .expect("default user settings should serialize");
        let query = "INSERT INTO users (id, display_name, email, settings, created_at) VALUES (?, ?, NULL, ?, ?) ON CONFLICT(id) DO NOTHING";
        sqlx::query(query)
            .bind(&self.user_id)
            .bind("Default User")
            .bind(settings_json)
            .bind(now_ms())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Fetch user settings from the database.
    pub async fn get_user_settings(&self) -> Result<crate::user::UserSettings, sqlx::Error> {
        let row = sqlx::query("SELECT settings FROM users WHERE id = ?")
            .bind(&self.user_id)
            .fetch_one(&self.pool)
            .await?;

        let settings_json: String = row.get("settings");
        crate::user::parse_persisted_user_settings(&settings_json, &self.settings_defaults)
            .map_err(|error| sqlx::Error::Protocol(error))
    }

    /// Update user settings and rebuild the scheduling cache for the selected algorithm.
    pub async fn update_user_settings_and_rebuild_scheduling_cache(
        &self,
        settings: &crate::user::UserSettings,
        algorithm: &dyn crate::srs::SrsAlgorithm,
    ) -> Result<(), sqlx::Error> {
        let settings_json = serde_json::to_string(settings).expect("UserSettings serializes");
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE users SET settings = ? WHERE id = ?")
            .bind(settings_json)
            .bind(&self.user_id)
            .execute(&mut *tx)
            .await?;
        self.rebuild_scheduling_cache_in_tx(algorithm, &mut tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn fetch_cards(
        &self,
    ) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>> {
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
                let fields = format!("{}\x1f{}", record.get::<String, _>("front_html"), metadata);
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
        if deck_data.cards.is_empty() {
            let mut tx = self.pool.begin().await?;
            self.get_or_create_deck_hierarchy(&deck_data.name, &deck_data.language_code, &mut tx)
                .await?;
            tx.commit().await?;
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let deck_id = self
            .get_or_create_deck_hierarchy(&deck_data.name, &deck_data.language_code, &mut tx)
            .await?;
        let now = now_ms();

        // Pre-generate one UUID per card so both batched INSERTs align by index.
        let card_ids: Vec<String> = (0..deck_data.cards.len())
            .map(|_| Uuid::now_v7().to_string())
            .collect();

        // cards: 12 params per row → cap at 75 rows per chunk to stay under SQLite's 999-param limit.
        const CARDS_CHUNK: usize = 75;
        for (chunk_idx, id_chunk) in card_ids.chunks(CARDS_CHUNK).enumerate() {
            let offset = chunk_idx * CARDS_CHUNK;
            let entries = &deck_data.cards[offset..offset + id_chunk.len()];
            let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT INTO cards (id, deck_id, user_id, skill_id, skill_name, template_name, front_html, back_html, fields_json, metadata_json, audio_path, created_at) ",
            );
            qb.push_values(id_chunk.iter().zip(entries), |mut b, (id, entry)| {
                b.push_bind(id)
                    .push_bind(&deck_id)
                    .push_bind(&self.user_id)
                    .push_bind(&entry.skill_id)
                    .push_bind(&entry.skill_name)
                    .push_bind(&entry.template_name)
                    .push_bind(&entry.front_html)
                    .push_bind(&entry.back_html)
                    .push_bind(&entry.fields_json)
                    .push_bind(&entry.metadata_json)
                    .push_bind(&entry.audio_path)
                    .push_bind(now);
            });
            qb.build().execute(&mut *tx).await?;
        }

        // reviews: 4 params per row → cap at 200 rows per chunk.
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
            qb.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(card_ids.len())
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
    use crate::srs::{Rating, SrsAlgorithmId, SrsRegistry};
    use crate::storage::{NewCardEntry, StorageProvider};

    fn temp_db_path(test_name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("panglot_{test_name}_{}.sqlite", Uuid::new_v4()))
    }

    async fn init_provider(test_name: &str) -> LocalStorageProvider {
        LocalStorageProvider::init(
            temp_db_path(test_name),
            crate::user::UserSettings::default(),
        )
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
    async fn select_study_cards_recurses_for_practice_and_review() {
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

        let practice_cards = provider
            .select_study_cards(&parent_id, StudyMode::Practice, 20)
            .await
            .expect("practice cards should load");
        let review_cards = provider
            .select_study_cards(&parent_id, StudyMode::Review, 20)
            .await
            .expect("review cards should load");

        assert_eq!(practice_cards.len(), 2);
        assert_eq!(review_cards.len(), 2);
        assert!(
            practice_cards
                .iter()
                .all(|card| !card.explanation_html.is_empty())
        );
    }

    #[tokio::test]
    async fn practice_rating_writes_only_practice_log() {
        let provider = init_provider("practice_rating").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");

        let card_id = first_card_id_for_path(&provider, "Vocabulary::Family").await;
        let before_interval: f64 =
            sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
                .bind(&card_id)
                .bind(&provider.user_id)
                .fetch_one(&provider.pool)
                .await
                .expect("review cache should exist")
                .get("interval_days");

        let algorithm = SrsRegistry::new();
        let output = provider
            .submit_practice(
                &card_id,
                Rating::Easy,
                algorithm.default(),
                1_700_000_000_000,
            )
            .await
            .expect("practice rating should succeed");

        let practice_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM practice_log WHERE card_id = ? AND user_id = ?",
        )
        .bind(&card_id)
        .bind(&provider.user_id)
        .fetch_one(&provider.pool)
        .await
        .expect("practice count should load");
        let review_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM review_log WHERE card_id = ? AND user_id = ?")
                .bind(&card_id)
                .bind(&provider.user_id)
                .fetch_one(&provider.pool)
                .await
                .expect("review count should load");
        let after_interval: f64 =
            sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
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
            .submit_review(
                &card_id,
                Rating::Easy,
                algorithm.default(),
                1_700_000_000_000,
            )
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
        let review_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM review_log WHERE card_id = ? AND user_id = ?")
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
    async fn updating_settings_rebuilds_review_cache_for_new_algorithm() {
        let provider = init_provider("settings_rebuild_cache").await;
        provider
            .save_deck(&test_deck("Vocabulary::Family", "skill-family", "Family"))
            .await
            .expect("deck should save");

        let card_id = first_card_id_for_path(&provider, "Vocabulary::Family").await;
        let registry = SrsRegistry::new();
        let sm2 = registry
            .get_typed(SrsAlgorithmId::Sm2)
            .expect("sm2 should exist");
        let leitner = registry
            .get_typed(SrsAlgorithmId::Leitner)
            .expect("leitner should exist");

        provider
            .submit_review(&card_id, Rating::Good, sm2, 1_700_000_000_000)
            .await
            .expect("review should succeed");
        provider
            .submit_review(&card_id, Rating::Good, sm2, 1_700_000_600_000)
            .await
            .expect("second review should succeed");

        let interval_before: f64 =
            sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
                .bind(&card_id)
                .bind(&provider.user_id)
                .fetch_one(&provider.pool)
                .await
                .expect("review cache should exist")
                .get("interval_days");

        let mut settings = provider
            .get_user_settings()
            .await
            .expect("settings should load");
        settings.study_preferences.srs.algorithm_id = SrsAlgorithmId::Leitner;
        provider
            .update_user_settings_and_rebuild_scheduling_cache(&settings, leitner)
            .await
            .expect("settings update should rebuild cache");

        let interval_after: f64 =
            sqlx::query("SELECT interval_days FROM reviews WHERE card_id = ? AND user_id = ?")
                .bind(&card_id)
                .bind(&provider.user_id)
                .fetch_one(&provider.pool)
                .await
                .expect("review cache should exist")
                .get("interval_days");

        assert_eq!(interval_before, 1.0);
        assert_eq!(interval_after, 3.0);
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
            .select_study_cards(&deck_id, StudyMode::Practice, 20)
            .await
            .expect("study cards should load");

        assert_eq!(
            row.get::<Option<String>, _>("skill_id").unwrap_or_default(),
            "skill-accusative"
        );
        assert_eq!(row.get::<String, _>("skill_name"), "Accusative");
        assert_eq!(study_cards[0].skill_id, "skill-accusative");
        assert_eq!(study_cards[0].skill_name, "Accusative");
    }
}
