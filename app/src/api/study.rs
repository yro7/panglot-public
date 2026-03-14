use actix_web::{web, HttpResponse, Responder};
use crate::state::{AppState, now_ms};
use crate::auth::AuthUser;
use super::models::ReviewOutcomeBody;

pub async fn get_study_session(
    auth: AuthUser,
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let deck_id = path.into_inner();
    let db = data.db_for(&auth);
    let algorithm = data.srs_for(&auth).await;
    let now = chrono::Utc::now().timestamp_millis();

    match db.get_due_cards_for_deck(&deck_id, 20).await {
        Ok(cards) => {
            let mut enhanced_cards = Vec::with_capacity(cards.len());
            for card in cards {
                let history = db.get_review_history(&card.id).await.unwrap_or_default();
                let choices = algorithm.preview_choices(&history, now);

                let mut card_json = serde_json::to_value(&card).unwrap();
                card_json["next_intervals"] = serde_json::json!({
                    "again": choices.again.interval_days,
                    "hard": choices.hard.interval_days,
                    "good": choices.good.interval_days,
                    "easy": choices.easy.interval_days,
                });
                enhanced_cards.push(card_json);
            }

            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "cards": enhanced_cards
            }))
        },
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Failed to fetch due cards: {}", e)
        }))
    }
}

pub async fn submit_review(
    auth: AuthUser,
    data: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ReviewOutcomeBody>,
) -> impl Responder {
    let _deck_id = path.into_inner();
    let rating = lc_core::srs::Rating::from_str_lossy(&body.rating);
    let algorithm = data.srs_for(&auth).await;
    let now = now_ms();
    let db = data.db_for(&auth);
    match db.submit_review(&body.card_id, rating, algorithm, now).await {
        Ok(output) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "next_due": output.due_date,
            "interval_days": output.interval_days
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Failed to submit review: {}", e)
        }))
    }
}

pub async fn get_srs_algorithms(data: web::Data<AppState>) -> impl Responder {
    let algos: Vec<serde_json::Value> = data.srs_registry.list().into_iter()
        .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
        .collect();
    HttpResponse::Ok().json(algos)
}
