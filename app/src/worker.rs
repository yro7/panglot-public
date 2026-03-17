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
                log::warn!("Draft cleanup failed: {}", e);
            }
        }
    });
}

/// Background task: scans Anki + local DB and loads lexicon into each pipeline for a given user.
pub async fn scan_lexicon_background(state: Arc<AppState>, anki_connect_url: Option<String>, user_id: String) {
    println!("\n📚 Starting lexicon scan for user {}...", &user_id[..8.min(user_id.len())]);
    let mut all_cards = Vec::new();

    if let Some(ref url) = anki_connect_url {
        let anki = AnkiStorageProvider::new(url);
        match anki.fetch_cards().await {
            Ok(cards) => {
                println!("   📡 Anki: fetched {} cards", cards.len());
                all_cards.extend(cards);
            }
            Err(e) => eprintln!("   ⚠️  Anki fetch failed: {e}"),
        }
    }

    let local = lc_core::db::LocalStorageProvider::for_user(state.db_pool.clone(), user_id.clone());
    match local.fetch_cards().await {
        Ok(cards) => {
            println!("   💾 Local DB: fetched {} cards", cards.len());
            all_cards.extend(cards);
        }
        Err(e) => eprintln!("   ⚠️  Local DB fetch failed: {e}"),
    }

    // Wrap the pre-fetched cards in a simple provider for load_lexicon
    let snapshot = lc_core::storage::SnapshotProvider::new(all_cards);
    let languages = state.languages.read().await;
    for (iso, runtime) in languages.iter() {
        match runtime.pipeline.load_lexicon(&snapshot).await {
            Ok(count) => println!("   ✅ {iso}: {count} words loaded into lexicon"),
            Err(e) => eprintln!("   ⚠️  {iso}: lexicon scan failed: {e}"),
        }
    }
    println!("📚 Background lexicon scan complete.");
}
