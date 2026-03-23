use actix_web::{web, HttpResponse, Responder};
use engine::card_models::CardModelId;
use engine::llm_client::RequestContext;
use engine::skill_tree;
use std::fs;
use anki_bridge::DeckBuilder;
use anki_bridge::AnkiStorageProvider;
use lc_core::storage::StorageProvider;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{GenerateRequest, ExportResponse};
use super::tree::build_user_tree;

pub async fn export_deck(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    if let Err(resp) = data.check_rate_limit(&auth.user_id).await {
        return resp;
    }
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(ExportResponse {
            success: false, message: e, file_path: None,
        }),
    };
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);
    let output_path = format!("{}/{}.apkg", data.output_dir, node_id);
    let _ = fs::create_dir_all(&data.output_dir);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
            file_path: None,
        });
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    if skill_tree::find_node(&user_tree, node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let req_ctx = Some(RequestContext {
        user_id: auth.user_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        endpoint: "/api/export".into(),
        language: Some(lang.to_string()),
    });

    let export_result = pipeline.generate_deck_data_dyn(
        &user_tree, node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(),
        body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(),
        req_ctx,
        llm_sem, pp_sem,
    ).await;

    match export_result {
        Ok(deck_data) => {
            let count = deck_data.cards.len();
            if count > 0 {
                let builder = DeckBuilder::new(deck_data);
                match builder.export_apkg(&output_path) {
                    Ok(_) => HttpResponse::Ok().json(ExportResponse {
                        success: true,
                        message: format!("Exported {} cards to {}", count, output_path),
                        file_path: Some(output_path),
                    }),
                    Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
                        success: false,
                        message: format!("Export failed: {}", e),
                        file_path: None,
                    }),
                }
            } else {
                HttpResponse::Ok().json(ExportResponse {
                    success: false,
                    message: "No cards were generated".to_string(),
                    file_path: None,
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
            success: false,
            message: format!("Generation failed: {}", e),
            file_path: None,
        }),
    }
}

pub async fn push_to_anki(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    if let Err(resp) = data.check_rate_limit(&auth.user_id).await {
        return resp;
    }
    let Some(ref anki_url) = data.anki_connect_url else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: "AnkiConnect URL not configured".to_string(),
            file_path: None,
        });
    };

    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(ExportResponse {
            success: false, message: e, file_path: None,
        }),
    };
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
            file_path: None,
        });
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    if skill_tree::find_node(&user_tree, node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let req_ctx = Some(RequestContext {
        user_id: auth.user_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        endpoint: "/api/push-to-anki".into(),
        language: Some(lang.to_string()),
    });

    let push_result = pipeline.generate_deck_data_dyn(
        &user_tree, node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(),
        body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(),
        req_ctx,
        llm_sem, pp_sem,
    ).await;

    match push_result {
        Ok(deck_data) => {
            let count = deck_data.cards.len();
            if count > 0 {
                let provider = AnkiStorageProvider::new(anki_url);
                match provider.save_deck(&deck_data).await {
                    Ok(saved) => HttpResponse::Ok().json(ExportResponse {
                        success: true,
                        message: format!("Pushed {} cards to Anki", saved),
                        file_path: None,
                    }),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("connection refused") || msg.contains("Connection refused") || msg.contains("error sending request") {
                            HttpResponse::ServiceUnavailable().json(ExportResponse {
                                success: false,
                                message: "Anki is not running or AnkiConnect plugin is not installed".to_string(),
                                file_path: None,
                            })
                        } else {
                            HttpResponse::InternalServerError().json(ExportResponse {
                                success: false,
                                message: format!("Push to Anki failed: {}", msg),
                                file_path: None,
                            })
                        }
                    }
                }
            } else {
                HttpResponse::Ok().json(ExportResponse {
                    success: false,
                    message: "No cards were generated".to_string(),
                    file_path: None,
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
            success: false,
            message: format!("Generation failed: {}", e),
            file_path: None,
        }),
    }
}

pub async fn push_to_local_db(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    if let Err(resp) = data.check_rate_limit(&auth.user_id).await {
        return resp;
    }
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(ExportResponse {
            success: false, message: e, file_path: None,
        }),
    };
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
            file_path: None,
        });
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    if skill_tree::find_node(&user_tree, node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let req_ctx = Some(RequestContext {
        user_id: auth.user_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        endpoint: "/api/push-local".into(),
        language: Some(lang.to_string()),
    });

    let push_result = pipeline.generate_deck_data_dyn(
        &user_tree, node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(),
        body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(),
        req_ctx,
        llm_sem, pp_sem,
    ).await;

    match push_result {
        Ok(deck_data) => {
            let count = deck_data.cards.len();
            if count > 0 {
                let db = data.db_for(&auth);
                match db.save_deck(&deck_data).await {
                    Ok(saved) => HttpResponse::Ok().json(ExportResponse {
                        success: true,
                        message: format!("Saved {} cards to Local Database", saved),
                        file_path: None,
                    }),
                    Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
                        success: false,
                        message: format!("Save to Local DB failed: {}", e),
                        file_path: None,
                    }),
                }
            } else {
                HttpResponse::Ok().json(ExportResponse {
                    success: false,
                    message: "No cards were generated".to_string(),
                    file_path: None,
                })
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(ExportResponse {
            success: false,
            message: format!("Generation failed: {}", e),
            file_path: None,
        }),
    }
}
