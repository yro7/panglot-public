use actix_web::{web, HttpResponse, Responder};
use engine::card_models::CardModelId;
use engine::llm_client::RequestContext;
use engine::skill_tree;
use lc_core::db::DraftCard;
use lc_core::storage::StorageProvider;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{
    GenerateRequest, GenerateResponse, GeneratedCardJson,
    PreviewPromptRequest, PreviewPromptResponse, PromptMessageJson, PreviewSchemas,
};
use super::tree::build_user_tree;

pub async fn generate_cards(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: e,
        }),
    };
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_generate);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false,
            cards: vec![],
            message: format!("Language '{}' not found", lang),
        });
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    let node = match skill_tree::find_node(&user_tree, node_id) {
        Some(n) => n,
        None => {
            return HttpResponse::BadRequest().json(GenerateResponse {
                success: false,
                cards: vec![],
                message: "Node not found in tree".to_string(),
            });
        }
    };

    let skill_name = node.name.clone();
    let is_category = !node.children.is_empty();

    if is_category {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false,
            cards: vec![],
            message: format!("Node '{}' is a category node — select a leaf node", node_id),
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let req_ctx = Some(RequestContext {
        user_id: auth.user_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        endpoint: "/api/generate".into(),
        language: Some(lang.to_string()),
    });

    let generation_result = pipeline.generate_cards_dyn(
        &user_tree, node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(), body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(),
        req_ctx,
        llm_sem, pp_sem,
    ).await;

    match generation_result {
        Ok(dyn_cards) => {
            let cards_json: Vec<GeneratedCardJson> = dyn_cards.into_iter().map(|c| GeneratedCardJson {
                card_id: c.card_id,
                skill_id: node_id.clone(),
                skill_name: skill_name.clone(),
                template_name: c.template_name,
                fields: c.fields,
                explanation: c.explanation,
                metadata_json: c.metadata_json,
            }).collect();

            drop(pipelines);

            let db = data.db_for(&auth);
            let drafts: Vec<DraftCard> = cards_json.iter().map(|c| DraftCard {
                id: c.card_id.clone(),
                skill_id: c.skill_id.clone(),
                skill_name: c.skill_name.clone(),
                template_name: c.template_name.clone(),
                fields_json: serde_json::to_string(&c.fields).unwrap_or_default(),
                explanation: c.explanation.clone(),
                metadata_json: c.metadata_json.clone(),
                created_at: 0, // set by save_drafts
            }).collect();
            if let Err(e) = db.save_drafts(&drafts).await {
                tracing::error!(%e, "Failed to save drafts");
            }

            HttpResponse::Ok().json(GenerateResponse {
                success: true,
                message: format!("Generated {} cards for '{}'", cards_json.len(), skill_name),
                cards: cards_json,
            })
        }
        Err(e) => {
            tracing::error!(%e, "Failed to generate cards batch");
            HttpResponse::InternalServerError().json(GenerateResponse {
                success: false,
                cards: vec![],
                message: format!("Failed to generate cards: {}", e),
            })
        }
    }
}

pub async fn generate_and_save(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: e,
        }),
    };
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_generate);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: format!("Language '{}' not found", lang),
        });
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    let node = match skill_tree::find_node(&user_tree, node_id) {
        Some(n) => n,
        None => return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: "Node not found in tree".to_string(),
        }),
    };

    let skill_name = node.name.clone();
    if !node.children.is_empty() {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![],
            message: format!("Node '{}' is a category node — select a leaf node", node_id),
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let req_ctx = Some(RequestContext {
        user_id: auth.user_id.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
        endpoint: "/api/generate-and-save".into(),
        language: Some(lang.to_string()),
    });

    let result = pipeline.generate_cards_and_deck_dyn(
        &user_tree, node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(), body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(),
        req_ctx,
        llm_sem, pp_sem,
    ).await;

    match result {
        Ok((dyn_cards, deck_data)) => {
            let cards_json: Vec<GeneratedCardJson> = dyn_cards.into_iter().map(|c| GeneratedCardJson {
                card_id: c.card_id,
                skill_id: node_id.clone(),
                skill_name: skill_name.clone(),
                template_name: c.template_name,
                fields: c.fields,
                explanation: c.explanation,
                metadata_json: c.metadata_json,
            }).collect();

            drop(pipelines);

            let db = data.db_for(&auth);
            let drafts: Vec<DraftCard> = cards_json.iter().map(|c| DraftCard {
                id: c.card_id.clone(),
                skill_id: c.skill_id.clone(),
                skill_name: c.skill_name.clone(),
                template_name: c.template_name.clone(),
                fields_json: serde_json::to_string(&c.fields).unwrap_or_default(),
                explanation: c.explanation.clone(),
                metadata_json: c.metadata_json.clone(),
                created_at: 0,
            }).collect();
            if let Err(e) = db.save_drafts(&drafts).await {
                tracing::error!(%e, "Failed to save drafts");
            }

            // Save to local DB
            let card_count = deck_data.cards.len();
            let save_msg = if card_count > 0 {
                match db.save_deck(&deck_data).await {
                    Ok(saved) => format!(" — saved {} cards to local DB", saved),
                    Err(e) => {
                        tracing::error!(%e, "Failed to save deck to local DB");
                        format!(" — failed to save to local DB: {}", e)
                    }
                }
            } else {
                String::new()
            };

            HttpResponse::Ok().json(GenerateResponse {
                success: true,
                message: format!("Generated {} cards for '{}'{}",
                    cards_json.len(), skill_name, save_msg),
                cards: cards_json,
            })
        }
        Err(e) => {
            tracing::error!(%e, "Failed to generate cards batch");
            HttpResponse::InternalServerError().json(GenerateResponse {
                success: false, cards: vec![],
                message: format!("Failed to generate cards: {}", e),
            })
        }
    }
}

pub async fn preview_prompt(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<PreviewPromptRequest>,
) -> impl Responder {
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    };
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Language '{}' not found", lang)
        }));
    };

    let user_tree = build_user_tree(&data, &auth, lang, pipeline.as_ref()).await;

    if skill_tree::find_node(&user_tree, &body.node_id).is_none() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Node not found: {}", body.node_id)
        }));
    }

    let preview = match pipeline.preview_prompt_dyn(
        &user_tree, &body.node_id, card_model_id, difficulty,
        body.user_profile.clone().unwrap_or_default(),
        body.lexicon_options.clone(),
    ) {
        Ok(p) => p,
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({ "error": e.to_string() })),
    };

    let user_content_call_1 = format!(
        "Difficulty level: {}/10.\n\nRespond with valid JSON only, no markdown.",
        difficulty
    );

    HttpResponse::Ok().json(PreviewPromptResponse {
        messages: vec![
            PromptMessageJson { role: "Call 1 — System (Content Generator)".to_string(), content: preview.system_prompt_call_1 },
            PromptMessageJson { role: "Call 1 — User".to_string(), content: user_content_call_1 },
            PromptMessageJson {
                role: "Call 2 — System (Feature Extractor & Explainer)".to_string(),
                content: preview.system_prompt_call_2,
            },
        ],
        temperature: data.generator_config.temperature,
        max_tokens: Some(data.generator_config.max_tokens),
        schemas: PreviewSchemas {
            call_1_content_generator: preview.schema_call_1,
            call_2_feature_extractor: preview.schema_call_2,
        },
    })
}
