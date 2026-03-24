use actix_web::{web, HttpResponse, Responder};
use engine::pipeline::LexiconStatus;
use std::collections::HashMap;

use crate::state::{AppState, UserLexicon};
use crate::auth::AuthUser;
use super::models::{LexiconQuery, GetTreeQuery};
use crate::worker::scan_lexicon_background;

pub async fn get_lexicon(
    auth: AuthUser,
    data: web::Data<AppState>,
    query: web::Query<LexiconQuery>,
) -> impl Responder {
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);

    let user_lexicons = data.user_lexicons.read().await;
    let (status, summary, words) = match user_lexicons.get(&auth.user_id) {
        Some(ul) => {
            let status = ul.statuses.get(lang).cloned().unwrap_or(LexiconStatus::NotStarted);
            match ul.trackers.get(lang) {
                Some(tracker) => {
                    let summary = tracker.summary_by_pos();
                    let words = tracker.known_words(query.pos.as_deref());
                    (status, summary, words)
                }
                None => (status, HashMap::new(), vec![]),
            }
        }
        None => (LexiconStatus::NotStarted, HashMap::new(), vec![]),
    };

    HttpResponse::Ok().json(serde_json::json!({
        "status": status,
        "summary": summary,
        "words": words,
        "total_known": words.len(),
    }))
}

pub async fn get_lexicon_all(
    auth: AuthUser,
    data: web::Data<AppState>,
    query: web::Query<GetTreeQuery>,
) -> impl Responder {
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);

    let user_lexicons = data.user_lexicons.read().await;
    let words = user_lexicons.get(&auth.user_id)
        .and_then(|ul| ul.trackers.get(lang))
        .map(|tracker| tracker.all_words_with_status())
        .unwrap_or_default();

    HttpResponse::Ok().json(serde_json::json!({
        "words": words,
        "total": words.len(),
    }))
}

pub async fn get_lexicon_status(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let user_lexicons = data.user_lexicons.read().await;
    let statuses: HashMap<String, LexiconStatus> = match user_lexicons.get(&auth.user_id) {
        Some(ul) => ul.statuses.clone(),
        None => HashMap::new(),
    };
    HttpResponse::Ok().json(serde_json::json!({
        "anki_connect_url": data.anki_connect_url,
        "statuses": statuses,
    }))
}

pub async fn rescan_lexicon(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let anki_url = data.anki_connect_url.clone();
    let user_id = auth.user_id.clone();

    // Set all language statuses to Loading for this user
    {
        let pipelines = data.pipelines.read().await;
        let iso_codes: Vec<String> = pipelines.keys().cloned().collect();
        drop(pipelines);

        let mut user_lexicons = data.user_lexicons.write().await;
        let ul = user_lexicons.entry(user_id.clone()).or_insert_with(|| UserLexicon {
            trackers: HashMap::new(),
            statuses: HashMap::new(),
        });
        for iso in iso_codes {
            ul.statuses.insert(iso, LexiconStatus::Loading);
        }
    }

    let state = data.into_inner();
    tokio::spawn(async move {
        scan_lexicon_background(state, anki_url, user_id).await;
    });

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Lexicon rescan started in background"
    }))
}
