use std::sync::Arc;
use sqlx::SqlitePool;
use anki_bridge::AnkiStorageProvider;
use lc_core::storage::StorageProvider;

use crate::state::{AppState, now_ms};

pub fn spawn_draft_cleanup(pool: SqlitePool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            let cutoff = now_ms() - 24 * 60 * 60 * 1000;
            if let Err(e) = sqlx::query("DELETE FROM draft_cards WHERE created_at < ?")
                .bind(cutoff)
                .execute(&pool)
                .await
            {
                tracing::warn!(%e, "Draft cleanup failed");
            }
        }
    });
}

/// Background task: scans Anki + local DB and loads lexicon into each pipeline for a given user.
pub async fn scan_lexicon_background(state: Arc<AppState>, anki_connect_url: Option<String>, user_id: String) {
    tracing::info!(user_id_prefix = &user_id[..8.min(user_id.len())], "Starting lexicon scan");
    let mut all_cards = Vec::new();

    if let Some(ref url) = anki_connect_url {
        let anki = AnkiStorageProvider::new(url);
        match anki.fetch_cards().await {
            Ok(cards) => {
                tracing::info!(count = cards.len(), "Anki: fetched cards");
                all_cards.extend(cards);
            }
            Err(e) => tracing::warn!(%e, "Anki fetch failed"),
        }
    }

    let local = lc_core::db::LocalStorageProvider::for_user(state.db_pool.clone(), user_id.clone());
    match local.fetch_cards().await {
        Ok(cards) => {
            tracing::info!(count = cards.len(), "Local DB: fetched cards");
            all_cards.extend(cards);
        }
        Err(e) => tracing::warn!(%e, "Local DB fetch failed"),
    }

    // Wrap the pre-fetched cards in a simple provider for load_lexicon
    let snapshot = lc_core::storage::SnapshotProvider::new(all_cards);
    let pipelines = state.pipelines.read().await;
    for (iso, pipeline) in pipelines.iter() {
        match pipeline.load_lexicon(&snapshot).await {
            Ok(count) => tracing::info!(iso, count, "Words loaded into lexicon"),
            Err(e) => tracing::warn!(iso, %e, "Lexicon scan failed"),
        }
    }
    tracing::info!("Background lexicon scan complete");
}
