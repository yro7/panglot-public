use actix_web::{web, HttpResponse, Responder};
use std::fs;
use anki_bridge::{AnkiStorageProvider, MultiDeckBuilder};
use lc_core::storage::StorageProvider;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::ExportResponse;

pub async fn get_anki_decks(data: web::Data<AppState>) -> impl Responder {
    let Some(ref url) = data.anki_connect_url else {
        return HttpResponse::Ok().json(serde_json::json!({
            "decks": [],
            "message": "No AnkiConnect URL configured"
        }));
    };

    let provider = AnkiStorageProvider::new(url);
    match provider.fetch_decks().await {
        Ok(decks) => HttpResponse::Ok().json(serde_json::json!({ "decks": decks })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to read Anki decks via AnkiConnect: {}", e)
        })),
    }
}

pub async fn get_local_decks(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.fetch_decks().await {
        Ok(decks) => HttpResponse::Ok().json(serde_json::json!({ "decks": decks })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to read local DB decks: {}", e)
        })),
    }
}

pub async fn export_db_to_apkg(
    auth: AuthUser,
    data: web::Data<AppState>,
) -> impl Responder {
    let output_dir = &data.output_dir;
    let _ = fs::create_dir_all(output_dir);

    let db = data.db_for(&auth);
    let decks = match db.fetch_decks_for_export().await {
        Ok(d) => d,
        Err(e) => return HttpResponse::InternalServerError().json(ExportResponse {
            success: false,
            message: format!("Failed to read cards from local DB: {}", e),
            file_path: None,
        }),
    };

    if decks.is_empty() {
        return HttpResponse::Ok().json(ExportResponse {
            success: false,
            message: "No cards found in local database".to_string(),
            file_path: None,
        });
    }

    let total_cards: usize = decks.iter().map(|d| d.cards.len()).sum();
    let output_path = format!("{}/panglot_export.apkg", output_dir);
    let builder = MultiDeckBuilder::new(decks);

    match builder.export_apkg(&output_path) {
        Ok(_) => HttpResponse::Ok().json(ExportResponse {
            success: true,
            message: format!("Exported {} cards to {}", total_cards, output_path),
            file_path: Some(output_path),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
            success: false,
            message: format!("Export failed: {}", e),
            file_path: None,
        }),
    }
}

pub async fn get_cards(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.get_drafts().await {
        Ok(drafts) => HttpResponse::Ok().json(drafts),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to fetch drafts: {}", e)
        })),
    }
}

pub async fn clear_cards(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.clear_drafts().await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true, "message": "Cards cleared"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to clear drafts: {}", e)
        })),
    }
}

pub async fn delete_deck(auth: AuthUser, data: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    let deck_id = path.into_inner();
    let db = data.db_for(&auth);
    match db.delete_deck(&deck_id).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true, "message": "Deck deleted"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to delete deck: {}", e)
        })),
    }
}
