use actix_web::{web, HttpResponse, Responder};
use engine::pipeline::LexiconStatus;
use std::collections::HashMap;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{LexiconQuery, GetTreeQuery};
use crate::worker::scan_lexicon_background;

pub async fn get_lexicon(
    data: web::Data<AppState>,
    query: web::Query<LexiconQuery>,
) -> impl Responder {
    let pipelines = data.pipelines.read().await;
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Language '{}' not found", lang)
        }));
    };

    let status = pipeline.lexicon_status();
    let summary = pipeline.lexicon_summary();
    let words = pipeline.lexicon_known_words(query.pos.as_deref());

    HttpResponse::Ok().json(serde_json::json!({
        "status": status,
        "summary": summary,
        "words": words,
        "total_known": words.len(),
    }))
}

pub async fn get_lexicon_all(
    data: web::Data<AppState>,
    query: web::Query<GetTreeQuery>,
) -> impl Responder {
    let pipelines = data.pipelines.read().await;
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Language '{}' not found", lang)
        }));
    };

    let words = pipeline.lexicon_all_words();
    HttpResponse::Ok().json(serde_json::json!({
        "words": words,
        "total": words.len(),
    }))
}

pub async fn get_lexicon_status(data: web::Data<AppState>) -> impl Responder {
    let pipelines = data.pipelines.read().await;
    let statuses: HashMap<String, LexiconStatus> = pipelines.iter()
        .map(|(iso, pipeline)| (iso.clone(), pipeline.lexicon_status()))
        .collect();
    HttpResponse::Ok().json(serde_json::json!({
        "anki_connect_url": data.anki_connect_url,
        "statuses": statuses,
    }))
}

pub async fn rescan_lexicon(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let anki_url = data.anki_connect_url.clone();
    let user_id = auth.user_id.clone();

    let pipelines = data.pipelines.read().await;

    // Set all pipelines to loading
    for pipeline in pipelines.values() {
        pipeline.set_lexicon_status(LexiconStatus::Loading);
    }
    drop(pipelines);

    let state = data.into_inner();
    tokio::spawn(async move {
        scan_lexicon_background(state, anki_url, user_id).await;
    });

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Lexicon rescan started in background"
    }))
}
