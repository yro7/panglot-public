use actix_web::{web, HttpResponse, Responder};
use engine::card_models::CardModelId;
use engine::llm_client::RequestContext;
use engine::pipeline::DynGeneratedCard;
use engine::skill_tree;
use lc_core::db::DraftCard;
use lc_core::storage::StorageProvider;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{
    GenerateRequest, GenerateResponse, GeneratedCardJson,
    PreviewPromptRequest, PreviewPromptResponse, PromptMessageJson, PreviewSchemas,
};
use super::tree::build_user_tree;

// ── Validated request, shared between generate_cards and generate_and_save ──

struct ValidatedRequest<'a> {
    node_id: &'a str,
    card_model_id: CardModelId,
    card_count: u32,
    difficulty: u8,
    skill_name: String,
    user_tree: engine::skill_tree::SkillNode,
    llm_sem: Arc<Semaphore>,
    pp_sem: Arc<Semaphore>,
    req_ctx: Option<RequestContext>,
}

/// Parses and validates the common parts of a generation request.
/// Returns `Err(HttpResponse)` on validation failure so the caller can return early.
async fn validate_request<'a>(
    auth: &AuthUser,
    data: &'a AppState,
    body: &'a GenerateRequest,
    pipeline: &dyn engine::pipeline::DynPipeline,
    endpoint: &str,
) -> Result<ValidatedRequest<'a>, HttpResponse> {
    let node_id = &body.node_id;
    let card_model_id: CardModelId = body.card_model_id.as_deref()
        .unwrap_or(&data.defaults.card_model)
        .parse()
        .map_err(|e| HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: e,
        }))?;
    let card_count = body.card_count.map(|c| c.get()).unwrap_or(data.defaults.card_count_generate);
    let difficulty = body.difficulty.map(|d| d.get()).unwrap_or(data.defaults.difficulty);
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let user_tree = build_user_tree(data, auth, lang, pipeline).await;

    let node = skill_tree::find_node(&user_tree, node_id)
        .ok_or_else(|| HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: "Node not found in tree".to_string(),
        }))?;

    let skill_name = node.name.clone();
    if !node.children.is_empty() {
        return Err(HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![],
            message: format!("Node '{}' is a category node — select a leaf node", node_id),
        }));
    }

    Ok(ValidatedRequest {
        node_id,
        card_model_id,
        card_count,
        difficulty,
        skill_name,
        user_tree,
        llm_sem: data.llm_semaphore.clone(),
        pp_sem: data.post_process_semaphore.clone(),
        req_ctx: Some(RequestContext {
            user_id: auth.user_id.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            endpoint: endpoint.into(),
            language: Some(lang.to_string()),
        }),
    })
}

/// Maps DynGeneratedCard results to JSON and saves drafts.
async fn finalize_cards(
    dyn_cards: Vec<DynGeneratedCard>,
    node_id: &str,
    skill_name: &str,
    data: &AppState,
    auth: &AuthUser,
) -> Vec<GeneratedCardJson> {
    let cards_json: Vec<GeneratedCardJson> = dyn_cards.into_iter().map(|c| GeneratedCardJson {
        card_id: c.card_id,
        skill_id: node_id.to_string(),
        skill_name: skill_name.to_string(),
        template_name: c.template_name,
        fields: c.fields,
        explanation: c.explanation,
        metadata_json: c.metadata_json,
    }).collect();

    let db = data.db_for(auth);
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

    cards_json
}

fn generation_error(e: anyhow::Error) -> HttpResponse {
    tracing::error!(%e, "Failed to generate cards batch");
    HttpResponse::InternalServerError().json(GenerateResponse {
        success: false, cards: vec![],
        message: format!("Failed to generate cards: {}", e),
    })
}

// ── Handlers ──

pub async fn generate_cards(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    if let Err(resp) = data.check_rate_limit(&auth.user_id).await {
        return resp;
    }
    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: format!("Language '{}' not found", lang),
        });
    };

    let req = match validate_request(&auth, &data, &body, pipeline.as_ref(), "/api/generate").await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let result = pipeline.generate_cards_dyn(
        &req.user_tree, req.node_id, req.card_model_id, req.card_count, req.difficulty,
        body.user_profile.clone(), body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(), req.req_ctx, req.llm_sem, req.pp_sem,
    ).await;

    match result {
        Ok(dyn_cards) => {
            let skill_name = req.skill_name.clone();
            drop(pipelines);
            let cards_json = finalize_cards(dyn_cards, req.node_id, &skill_name, &data, &auth).await;
            HttpResponse::Ok().json(GenerateResponse {
                success: true,
                message: format!("Generated {} cards for '{}'", cards_json.len(), skill_name),
                cards: cards_json,
            })
        }
        Err(e) => generation_error(e),
    }
}

pub async fn generate_and_save(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    if let Err(resp) = data.check_rate_limit(&auth.user_id).await {
        return resp;
    }
    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: format!("Language '{}' not found", lang),
        });
    };

    let req = match validate_request(&auth, &data, &body, pipeline.as_ref(), "/api/generate-and-save").await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let result = pipeline.generate_cards_and_deck_dyn(
        &req.user_tree, req.node_id, req.card_model_id, req.card_count, req.difficulty,
        body.user_profile.clone(), body.user_prompt.as_deref().map(String::from),
        body.lexicon_options.clone(), req.req_ctx, req.llm_sem, req.pp_sem,
    ).await;

    match result {
        Ok((dyn_cards, deck_data)) => {
            let skill_name = req.skill_name.clone();
            drop(pipelines);
            let cards_json = finalize_cards(dyn_cards, req.node_id, &skill_name, &data, &auth).await;

            let db = data.db_for(&auth);
            let save_msg = if !deck_data.cards.is_empty() {
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
        Err(e) => generation_error(e),
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
