use std::fs;
use std::pin::Pin;
use std::future::Future;
use actix_files::Files;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use sqlx::SqlitePool;

use lc_core::traits::Language;
use lc_core::user::UserProfile;
use lc_core::db::DraftCard;
use engine::card_models::CardModelId;
use engine::llm_client::{LlmHttpClient, LlmProvider};
use engine::pipeline::{DynPipeline, LexiconStatus, Pipeline};
use engine::prompts::PromptConfig;
use engine::python_sidecar::PythonSidecar;
use engine::skill_tree::{SkillNode, SkillTree, SkillTreeConfig};
use lc_core::storage::StorageProvider;
use anki_bridge::{AnkiStorageProvider, DeckBuilder, MultiDeckBuilder};
// Language types are auto-registered via langs::dispatch_iso!

mod config;

// ═══════════════════════════════════════════════
//  Shared Application State
// ═══════════════════════════════════════════════

struct AppState {
    pipelines: RwLock<HashMap<String, Box<dyn DynPipeline>>>,
    llm_semaphore: Arc<Semaphore>,
    post_process_semaphore: Arc<Semaphore>,
    defaults: config::DefaultsConfig,
    generator_config: config::LlmCallConfig,
    /// Runtime-mutable LLM config (provider, model, keys).
    llm_runtime: tokio::sync::RwLock<LlmRuntimeConfig>,
    /// Full config (for model lists lookup).
    llm_config: config::LlmConfig,
    output_dir: String,
    /// Resolved AnkiConnect URL.
    anki_connect_url: Option<String>,

    /// Raw SQLite pool — never use directly, always go through db_for().
    db_pool: SqlitePool,
    /// SRS algorithm registry (SM-2, Leitner, etc.).
    srs_registry: lc_core::srs::SrsRegistry,
    auth_enabled: bool,
    jwks_keys: Arc<RwLock<HashMap<String, (Algorithm, DecodingKey)>>>,
    jwks_url: Option<String>,
    jwks_last_refresh: Arc<std::sync::atomic::AtomicI64>,
    /// HS256 fallback key (from SUPABASE_JWT_SECRET).
    jwt_hs256_key: Option<DecodingKey>,
    known_users: dashmap::DashSet<String>,
}

impl AppState {
    fn db_for(&self, user: &AuthUser) -> lc_core::db::LocalStorageProvider {
        lc_core::db::LocalStorageProvider::for_user(self.db_pool.clone(), user.user_id.clone())
    }

    /// Resolve the SRS algorithm for a given user.
    /// For the MVP: always returns the default (SM-2).
    /// Later: read from users.settings.srs_algorithm.
    fn srs_for(&self, _auth: &AuthUser) -> &dyn lc_core::srs::SrsAlgorithm {
        self.srs_registry.default()
    }
}

/// Mutable subset of LLM config that can be changed at runtime.
struct LlmRuntimeConfig {
    provider: LlmProvider,
    model: String,
    api_key: String,
    base_url: String,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ═══════════════════════════════════════════════
//  Auth — JWT verification via JWKS (RS256)
// ═══════════════════════════════════════════════

/// Parse a single JWK into an (Algorithm, DecodingKey) pair.
fn parse_jwk(key: &serde_json::Value) -> Option<(Algorithm, DecodingKey)> {
    match key["kty"].as_str()? {
        "RSA" => {
            let n = key["n"].as_str()?;
            let e = key["e"].as_str()?;
            let dk = DecodingKey::from_rsa_components(n, e).ok()?;
            Some((Algorithm::RS256, dk))
        }
        "EC" => {
            let x = key["x"].as_str()?;
            let y = key["y"].as_str()?;
            let dk = DecodingKey::from_ec_components(x, y).ok()?;
            let alg = match key["alg"].as_str().unwrap_or("ES256") {
                "ES384" => Algorithm::ES384,
                _ => Algorithm::ES256,
            };
            Some((alg, dk))
        }
        _ => None,
    }
}

const JWKS_REFRESH_COOLDOWN_MS: i64 = 60_000;

async fn refresh_jwks(state: &AppState) -> bool {
    let url = match &state.jwks_url {
        Some(u) => u,
        None => return false,
    };

    let now = now_ms();
    let last = state.jwks_last_refresh.load(Ordering::Relaxed);
    if now - last < JWKS_REFRESH_COOLDOWN_MS {
        return false;
    }
    if state.jwks_last_refresh.compare_exchange(last, now, Ordering::AcqRel, Ordering::Relaxed).is_err() {
        return false;
    }

    log::info!("Re-fetching JWKS from {}", url);
    let resp: serde_json::Value = match reqwest::get(url).await {
        Ok(r) => match r.json().await {
            Ok(j) => j,
            Err(e) => { log::error!("JWKS parse error: {}", e); return false; }
        },
        Err(e) => { log::error!("JWKS fetch error: {}", e); return false; }
    };

    let keys = match resp["keys"].as_array() {
        Some(k) => k,
        None => { log::error!("JWKS 'keys' is not an array"); return false; }
    };

    let mut new_map = HashMap::new();
    for key in keys {
        if let Some(kid) = key["kid"].as_str() {
            if let Some(entry) = parse_jwk(key) {
                new_map.insert(kid.to_string(), entry);
            }
        }
    }
    log::info!("Refreshed JWKS: {} key(s)", new_map.len());
    *state.jwks_keys.write().await = new_map;
    true
}

struct AuthUser {
    user_id: String,
    claims: Option<serde_json::Value>,
}

impl actix_web::FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            let state = req.app_data::<web::Data<AppState>>()
                .ok_or_else(|| actix_web::error::ErrorInternalServerError("AppState missing"))?;

            if !state.auth_enabled {
                return Ok(AuthUser { user_id: "default-user".into(), claims: None });
            }

            let token = req.headers().get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .ok_or_else(|| actix_web::error::ErrorUnauthorized("Missing token"))?;

            let header = jsonwebtoken::decode_header(token)
                .map_err(|e| actix_web::error::ErrorUnauthorized(format!("Invalid JWT header: {}", e)))?;

            let kid = header.kid
                .ok_or_else(|| actix_web::error::ErrorUnauthorized("JWT missing kid"))?;

            // Try JWKS first (supports RSA + EC keys)
            let try_jwks = |keys: &HashMap<String, (Algorithm, DecodingKey)>| -> Option<jsonwebtoken::TokenData<serde_json::Value>> {
                let (alg, key) = keys.get(&kid)?;
                let mut validation = Validation::new(*alg);
                validation.set_audience(&["authenticated"]);
                decode::<serde_json::Value>(token, key, &validation).ok()
            };

            let data = {
                let keys = state.jwks_keys.read().await;
                try_jwks(&keys)
            };

            let data = match data {
                Some(d) => d,
                None => {
                    // Try refreshing JWKS (key rotation)
                    refresh_jwks(&state).await;
                    let keys = state.jwks_keys.read().await;
                    match try_jwks(&keys) {
                        Some(d) => d,
                        None => {
                            // HS256 fallback
                            if let Some(hs_key) = &state.jwt_hs256_key {
                                let mut validation = Validation::new(Algorithm::HS256);
                                validation.set_audience(&["authenticated"]);
                                decode::<serde_json::Value>(token, hs_key, &validation)
                                    .map_err(|e| actix_web::error::ErrorUnauthorized(format!("Token verification failed: {}", e)))?
                            } else {
                                return Err(actix_web::error::ErrorUnauthorized(
                                    format!("Unknown kid '{}' and no HS256 fallback", kid)
                                ));
                            }
                        }
                    }
                }
            };

            let user_id = data.claims["sub"].as_str()
                .ok_or_else(|| actix_web::error::ErrorUnauthorized("Missing sub"))?
                .to_string();

            if state.known_users.insert(user_id.clone()) {
                sqlx::query("INSERT OR IGNORE INTO users (id, display_name, email, settings, created_at) VALUES (?, 'user', NULL, '{}', ?)")
                    .bind(&user_id)
                    .bind(now_ms())
                    .execute(&state.db_pool)
                    .await
                    .map_err(|e| {
                        log::error!("Failed to provision user {}: {}", user_id, e);
                        state.known_users.remove(&user_id);
                        actix_web::error::ErrorInternalServerError("User provisioning failed")
                    })?;
            }

            Ok(AuthUser { user_id, claims: Some(data.claims) })
        })
    }
}

// ═══════════════════════════════════════════════
//  JSON Serialization Types (API layer)
// ═══════════════════════════════════════════════

#[derive(Serialize)]
struct TreeNodeJson {
    id: String,
    name: String,
    is_leaf: bool,
    node_instructions: Option<String>,
    children: Vec<TreeNodeJson>,
}

#[derive(Serialize, Clone)]
struct GeneratedCardJson {
    card_id: String,
    skill_id: String,
    skill_name: String,
    template_name: String,
    fields: std::collections::HashMap<String, String>,
    explanation: String,
    metadata_json: String,
}

#[derive(Deserialize)]
struct GenerateRequest {
    language: Option<String>,
    node_id: String,
    card_model_id: Option<String>,
    card_count: Option<u32>,
    difficulty: Option<u8>,
    user_prompt: Option<String>,
    user_profile: UserProfile,
    lexicon_options: Option<engine::generator::LexiconOption>,
}

#[derive(Serialize)]
struct GenerateResponse {
    success: bool,
    cards: Vec<GeneratedCardJson>,
    message: String,
}

#[derive(Serialize)]
struct ExportResponse {
    success: bool,
    message: String,
    file_path: Option<String>,
}

#[derive(Deserialize)]
struct PreviewPromptRequest {
    language: Option<String>,
    node_id: String,
    card_model_id: Option<String>,
    difficulty: Option<u8>,
    user_profile: Option<UserProfile>,
    lexicon_options: Option<engine::generator::LexiconOption>,
}

#[derive(Serialize)]
struct PreviewPromptResponse {
    messages: Vec<PromptMessageJson>,
    temperature: f32,
    max_tokens: Option<u32>,
    /// JSON schemas sent via structured output (response_format) for debug inspection.
    schemas: PreviewSchemas,
}

#[derive(Serialize)]
struct PreviewSchemas {
    call_1_content_generator: serde_json::Value,
    call_2_feature_extractor: serde_json::Value,
}

#[derive(Serialize)]
struct PromptMessageJson {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AddNodeRequest {
    language: Option<String>,
    parent_id: String,
    node_id: String,
    node_name: String,
    node_instructions: Option<String>,
}

#[derive(Serialize)]
struct AddNodeResponse {
    success: bool,
    message: String,
}

#[cfg(test)]
mod type_assertions;

// ═══════════════════════════════════════════════
//  Helper: Convert SkillTree to JSON
// ═══════════════════════════════════════════════

fn tree_node_to_json(node: &SkillNode) -> TreeNodeJson {
    TreeNodeJson {
        id: node.id.clone(),
        name: node.name.clone(),
        is_leaf: node.children.is_empty(),
        node_instructions: node.node_instructions.clone(),
        children: node
            .children
            .iter()
            .map(tree_node_to_json)
            .collect(),
    }
}

// ═══════════════════════════════════════════════
//  Pipeline construction helper
// ═══════════════════════════════════════════════

fn build_typed_pipeline<L>(
    lang: L,
    tree_config: SkillTreeConfig,
    cfg: &config::AppConfig,
    make_client: &dyn Fn(&str) -> LlmHttpClient,
    sidecar: &Arc<tokio::sync::Mutex<PythonSidecar>>,
    prompt_config: &PromptConfig,
) -> Box<dyn DynPipeline>
where
    L: Language + Send + Sync + 'static,
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    L::ExtraFields: schemars::JsonSchema + Send + Sync,
{
    let tree = SkillTree::from_config(lang, tree_config);
    let model = cfg.llm.active_model().expect("model already validated at startup");
    let client = make_client(model);
    let mut pipeline = Pipeline::new(
        tree, Box::new(client),
        cfg.generator.temperature, cfg.generator.max_tokens,
        cfg.feature_extractor.temperature, cfg.feature_extractor.max_tokens,
        prompt_config.clone(),
    );
    pipeline.add_early_processor(Box::new(engine::post_process::IpaGenerator::new(sidecar.clone())));
    pipeline.add_early_processor(Box::new(engine::post_process::TtsGenerator::new(sidecar.clone())));
    Box::new(pipeline)
}

/// Maps an ISO 639-3 code to a concrete Language type and builds the pipeline.
/// Match arms are auto-generated by the `langs::dispatch_iso!` macro.
fn build_pipeline_for_iso(
    iso: &str,
    tree_config: SkillTreeConfig,
    cfg: &config::AppConfig,
    make_client: &dyn Fn(&str) -> LlmHttpClient,
    sidecar: &Arc<tokio::sync::Mutex<PythonSidecar>>,
    prompt_config: &PromptConfig,
) -> Option<Box<dyn DynPipeline>> {
    langs::dispatch_iso!(iso, lang => {
        build_typed_pipeline(lang, tree_config, cfg, make_client, sidecar, prompt_config)
    })
}

// ═══════════════════════════════════════════════
//  API Handlers
// ═══════════════════════════════════════════════

#[derive(Deserialize)]
struct GetTreeQuery {
    lang: Option<String>,
}

async fn get_tree(
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

    HttpResponse::Ok().json(tree_node_to_json(pipeline.tree_root()))
}

async fn generate_cards(
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
    let card_count = body.card_count.unwrap_or(data.defaults.card_count_generate);
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false,
            cards: vec![],
            message: format!("Language '{}' not found", lang),
        });
    };

    let node = match pipeline.find_node(node_id) {
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

    let generation_result = pipeline.generate_cards_dyn(
        node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(), body.user_prompt.clone(),
        body.lexicon_options.clone(),
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
                log::error!("Failed to save drafts: {}", e);
            }

            HttpResponse::Ok().json(GenerateResponse {
                success: true,
                message: format!("Generated {} cards for '{}'", cards_json.len(), skill_name),
                cards: cards_json,
            })
        }
        Err(e) => {
            eprintln!("Failed to generate cards batch: {}", e);
            HttpResponse::InternalServerError().json(GenerateResponse {
                success: false,
                cards: vec![],
                message: format!("Failed to generate cards: {}", e),
            })
        }
    }
}

async fn generate_and_save(
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
    let card_count = body.card_count.unwrap_or(data.defaults.card_count_generate);
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(GenerateResponse {
            success: false, cards: vec![], message: format!("Language '{}' not found", lang),
        });
    };

    let node = match pipeline.find_node(node_id) {
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

    let result = pipeline.generate_cards_and_deck_dyn(
        node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(), body.user_prompt.clone(),
        body.lexicon_options.clone(),
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
                log::error!("Failed to save drafts: {}", e);
            }

            // Save to local DB
            let card_count = deck_data.cards.len();
            let save_msg = if card_count > 0 {
                match db.save_deck(&deck_data).await {
                    Ok(saved) => format!(" — saved {} cards to local DB", saved),
                    Err(e) => {
                        eprintln!("Failed to save deck to local DB: {}", e);
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
            eprintln!("Failed to generate cards batch: {}", e);
            HttpResponse::InternalServerError().json(GenerateResponse {
                success: false, cards: vec![],
                message: format!("Failed to generate cards: {}", e),
            })
        }
    }
}

async fn export_deck(
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(ExportResponse {
            success: false, message: e, file_path: None,
        }),
    };
    let card_count = body.card_count.unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

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

    if pipeline.find_node(node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let export_result = pipeline.generate_deck_data_dyn(
        node_id, card_model_id, card_count, difficulty,
        UserProfile::new(data.defaults.user_language.clone()),
        body.user_prompt.clone(),
        body.lexicon_options.clone(),
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

async fn push_to_anki(
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
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
    let card_count = body.card_count.unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
            file_path: None,
        });
    };

    if pipeline.find_node(node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let push_result = pipeline.generate_deck_data_dyn(
        node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(),
        body.user_prompt.clone(),
        body.lexicon_options.clone(),
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

async fn push_to_local_db(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<GenerateRequest>,
) -> impl Responder {
    let node_id = &body.node_id;
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(ExportResponse {
            success: false, message: e, file_path: None,
        }),
    };
    let card_count = body.card_count.unwrap_or(data.defaults.card_count_export);
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
            file_path: None,
        });
    };

    if pipeline.find_node(node_id).is_none() {
        return HttpResponse::BadRequest().json(ExportResponse {
            success: false,
            message: format!("Unknown node ID: '{}' in language '{}'", node_id, lang),
            file_path: None,
        });
    }

    let llm_sem = data.llm_semaphore.clone();
    let pp_sem = data.post_process_semaphore.clone();

    let push_result = pipeline.generate_deck_data_dyn(
        node_id, card_model_id, card_count, difficulty,
        body.user_profile.clone(),
        body.user_prompt.clone(),
        body.lexicon_options.clone(),
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

async fn get_cards(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.get_drafts().await {
        Ok(drafts) => HttpResponse::Ok().json(drafts),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to fetch drafts: {}", e)
        })),
    }
}

async fn clear_cards(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.clear_drafts().await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true, "message": "Cards cleared"})),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to clear drafts: {}", e)
        })),
    }
}

async fn preview_prompt(
    data: web::Data<AppState>,
    body: web::Json<PreviewPromptRequest>,
) -> impl Responder {
    let card_model_id: CardModelId = match body.card_model_id.as_deref().unwrap_or(&data.defaults.card_model).parse() {
        Ok(id) => id,
        Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    };
    let difficulty = body.difficulty.unwrap_or(data.defaults.difficulty);

    let pipelines = data.pipelines.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get(lang) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Language '{}' not found", lang)
        }));
    };

    if pipeline.find_node(&body.node_id).is_none() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Node not found: {}", body.node_id)
        }));
    }

    let preview = match pipeline.preview_prompt_dyn(
        &body.node_id, card_model_id, difficulty,
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

#[derive(Deserialize)]
struct CardModelQuery {
    lang: Option<String>,
}

async fn get_card_models(
    data: web::Data<AppState>,
    query: web::Query<CardModelQuery>,
) -> impl Responder {
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);
    let pipelines = data.pipelines.read().await;

    let models: Vec<String> = if let Some(pipeline) = pipelines.get(lang) {
        pipeline.available_models().into_iter().map(|id| id.to_string()).collect()
    } else {
        CardModelId::ALL.iter().map(|id| id.to_string()).collect()
    };

    HttpResponse::Ok().json(serde_json::json!({ "models": models }))
}

async fn add_node(
    data: web::Data<AppState>,
    body: web::Json<AddNodeRequest>,
) -> impl Responder {
    let mut pipelines = data.pipelines.write().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(pipeline) = pipelines.get_mut(lang) else {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
        });
    };

    if pipeline.find_node(&body.node_id).is_some() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Node ID '{}' already exists in the tree", body.node_id),
        });
    }

    let new_node = SkillNode {
        id: body.node_id.clone(),
        name: body.node_name.clone(),
        node_instructions: body.node_instructions.clone(),
        children: vec![],
    };

    match pipeline.find_node_mut(&body.parent_id) {
        Some(parent) => {
            parent.children.push(new_node);
            HttpResponse::Ok().json(AddNodeResponse {
                success: true,
                message: format!("Node '{}' added under '{}'", body.node_name, body.parent_id),
            })
        }
        None => HttpResponse::InternalServerError().json(AddNodeResponse {
            success: false,
            message: format!("Failed to find parent node '{}' in tree", body.parent_id),
        }),
    }
}

async fn get_languages(data: web::Data<AppState>) -> impl Responder {
    let pipelines = data.pipelines.read().await;
    let languages: Vec<serde_json::Value> = pipelines.iter().map(|(iso, p)| {
        serde_json::json!({ "iso": iso, "name": p.language_name() })
    }).collect();
    HttpResponse::Ok().json(languages)
}

// ═══════════════════════════════════════════════
//  Lexicon & Decks API Handlers
// ═══════════════════════════════════════════════

async fn get_anki_decks(data: web::Data<AppState>) -> impl Responder {
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

async fn get_local_decks(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.fetch_decks().await {
        Ok(decks) => HttpResponse::Ok().json(serde_json::json!({ "decks": decks })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to read local DB decks: {}", e)
        })),
    }
}

async fn export_db_to_apkg(
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

async fn get_study_session(
    auth: AuthUser,
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let deck_id = path.into_inner();
    let db = data.db_for(&auth);
    let algorithm = data.srs_for(&auth);
    let now = chrono::Utc::now().timestamp_millis();

    match db.get_due_cards_for_deck(&deck_id, 20).await {
        Ok(cards) => {
            let mut enhanced_cards = Vec::with_capacity(cards.len());
            for mut card in cards {
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

#[derive(Deserialize)]
struct ReviewOutcomeBody {
    card_id: String,
    rating: String,
}

async fn submit_review(
    auth: AuthUser,
    data: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ReviewOutcomeBody>,
) -> impl Responder {
    let _deck_id = path.into_inner();
    let rating = lc_core::srs::Rating::from_str_lossy(&body.rating);
    let algorithm = data.srs_for(&auth);
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

async fn get_srs_algorithms(data: web::Data<AppState>) -> impl Responder {
    let algos: Vec<serde_json::Value> = data.srs_registry.list().into_iter()
        .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
        .collect();
    HttpResponse::Ok().json(algos)
}

#[derive(Deserialize)]
struct LexiconQuery {
    lang: Option<String>,
    pos: Option<String>,
}

async fn get_lexicon(
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

async fn get_lexicon_all(
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

async fn get_lexicon_status(data: web::Data<AppState>) -> impl Responder {
    let pipelines = data.pipelines.read().await;
    let statuses: std::collections::HashMap<String, LexiconStatus> = pipelines.iter()
        .map(|(iso, p)| (iso.clone(), p.lexicon_status()))
        .collect();
    HttpResponse::Ok().json(serde_json::json!({
        "anki_connect_url": data.anki_connect_url,
        "statuses": statuses,
    }))
}

async fn rescan_lexicon(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let anki_url = data.anki_connect_url.clone();
    let user_id = auth.user_id.clone();

    let pipelines = data.pipelines.read().await;

    // Set all pipelines to loading
    for p in pipelines.values() {
        p.set_lexicon_status(LexiconStatus::Loading);
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

// ═══════════════════════════════════════════════
//  Runtime LLM Config
// ═══════════════════════════════════════════════

#[derive(Deserialize)]
struct UpdateLlmConfigRequest {
    /// Switch provider (e.g. "google", "anthropic")
    provider: Option<String>,
    /// Override the model (must be in the provider's model list). If omitted, uses first model.
    model: Option<String>,
}

async fn get_llm_config(data: web::Data<AppState>) -> impl Responder {
    let rt = data.llm_runtime.read().await;
    HttpResponse::Ok().json(serde_json::json!({
        "provider": rt.provider.to_string(),
        "model": rt.model,
        "available_models": data.llm_config.models,
    }))
}

async fn update_llm_config(
    data: web::Data<AppState>,
    body: web::Json<UpdateLlmConfigRequest>,
) -> impl Responder {
    // Resolve new provider (or keep current)
    let new_provider: LlmProvider = if let Some(ref p) = body.provider {
        match p.parse() {
            Ok(prov) => prov,
            Err(e) => return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false, "message": format!("{}", e)
            })),
        }
    } else {
        data.llm_runtime.read().await.provider
    };

    let provider_key = new_provider.to_string();

    // Resolve model
    let new_model = if let Some(ref m) = body.model {
        // Validate that the model is in the provider's list
        let models = data.llm_config.models.get(&provider_key);
        if let Some(list) = models {
            if !list.contains(m) {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "message": format!("Model '{}' not in {}'s model list: {:?}", m, provider_key, list)
                }));
            }
        }
        m.clone()
    } else {
        // Use first model from the new provider's list
        match data.llm_config.models.get(&provider_key) {
            Some(list) if !list.is_empty() => list[0].clone(),
            _ => return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": format!("No models configured for provider '{}'", provider_key)
            })),
        }
    };

    // Resolve API key for the new provider
    let api_key_env = data.llm_config.api_key_env.as_deref()
        .unwrap_or(new_provider.default_api_key_env());
    let api_key = match std::env::var(api_key_env) {
        Ok(k) => k,
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("API key env '{}' not set for provider '{}'", api_key_env, provider_key)
        })),
    };

    let base_url = data.llm_config.base_url.clone()
        .unwrap_or_else(|| new_provider.default_base_url().to_string());

    // Build the new LLM client and swap it into every pipeline
    let new_client_factory = || -> Box<dyn engine::llm_client::LlmClient> {
        Box::new(LlmHttpClient::custom(
            api_key.clone(), base_url.clone(), new_model.clone(), new_provider,
        ))
    };

    {
        let pipelines = data.pipelines.read().await;
        for pipeline in pipelines.values() {
            pipeline.swap_llm_client(new_client_factory()).await;
        }
    }

    // Update runtime config
    {
        let mut rt = data.llm_runtime.write().await;
        rt.provider = new_provider;
        rt.model = new_model.clone();
        rt.api_key = api_key;
        rt.base_url = base_url;
    }

    println!("[Config] Switched to provider={}, model={}", provider_key, new_model);

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "provider": provider_key,
        "model": new_model,
    }))
}

// ═══════════════════════════════════════════════
//  Main — Start the server
// ═══════════════════════════════════════════════

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    println!("🌍 Panglot — Beta Test Server");
    println!("======================================\n");

    // ── Load global config ──
    let cfg = config::load_config("config.yml")
        .expect("Failed to load config.yml");

    // ── Load prompt config ──
    let prompt_config = PromptConfig::load("prompts")
        .expect("Failed to load prompt config from prompts/ directory");

    // Resolve LLM provider from config
    dotenvy::dotenv().ok();
    let provider: LlmProvider = cfg.llm.provider.parse()
        .unwrap_or_else(|e| panic!("Invalid LLM provider in config.yml: {}", e));

    // Resolve API key (provider default or config override)
    let api_key_env = cfg.llm.api_key_env.as_deref()
        .unwrap_or(provider.default_api_key_env());
    let api_key = std::env::var(api_key_env)
        .unwrap_or_else(|_| panic!("{} not set — add it to your .env", api_key_env));

    // Resolve base URL (provider default or config override)
    let base_url = cfg.llm.base_url.clone()
        .unwrap_or_else(|| provider.default_base_url().to_string());

    // Resolve active model from the chosen provider's model list
    let active_model = cfg.llm.active_model()
        .unwrap_or_else(|e| panic!("Config error: {}", e))
        .to_string();

    println!("   Provider:         {}", provider);
    println!("   Active model:     {}", active_model);
    println!("   LLM base URL:     {}", base_url);

    // Setup Database
    let db_path = std::path::Path::new(&cfg.paths.output_dir).join("panglot.db");
    let local_db = lc_core::db::LocalStorageProvider::init(&db_path)
        .await
        .expect("Failed to initialize SQLite database");

    // Spawn a single Python sidecar for IPA + TTS (shared across all languages)
    let sidecar = Arc::new(tokio::sync::Mutex::new(
        PythonSidecar::spawn().expect("Failed to start Python sidecar")
    ));

    let mut pipelines_map: std::collections::HashMap<String, Box<dyn DynPipeline>> =
        std::collections::HashMap::new();

    // Helper: create an LLM client for a given model
    let make_client = |model: &str| -> LlmHttpClient {
        LlmHttpClient::custom(api_key.clone(), base_url.clone(), model.to_string(), provider)
    };

    // ── Auto-discover skill trees from *_tree.yaml files ──
    let tree_dir = &cfg.paths.skill_trees_dir;
    let entries = fs::read_dir(tree_dir)
        .unwrap_or_else(|e| panic!("Cannot read skill_trees_dir '{tree_dir}': {e}"));

    for entry in entries.flatten() {
        let path = entry.path();
        let filename = match path.file_name().and_then(|f| f.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        let iso = match filename.strip_suffix("_tree.yaml") {
            Some(prefix) => prefix.to_string(),
            None => continue,
        };

        let yaml_content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("⚠️  Failed to read skill tree at {path:?}: {e}");
                continue;
            }
        };
        let tree_config: SkillTreeConfig = match serde_yaml::from_str(&yaml_content) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("⚠️  Failed to parse skill tree at {path:?}: {e}");
                continue;
            }
        };

        match build_pipeline_for_iso(&iso, tree_config, &cfg, &make_client, &sidecar, &prompt_config) {
            Some(pipeline) => {
                println!("✅ Loaded {} ({}) skill tree from {path:?}", pipeline.language_name(), iso);
                pipelines_map.insert(iso, pipeline);
            }
            None => {
                eprintln!("⚠️  No language registered for ISO code '{iso}' (from {path:?}) — skipping");
            }
        }
    }

    if pipelines_map.is_empty() {
        eprintln!("❌ No skill trees loaded! Check that {tree_dir}/*_tree.yaml files exist");
        std::process::exit(1);
    }

    // ── Resolve AnkiConnect URL ──
    let anki_connect_url = Some(config::resolve_anki_connect_url(cfg.paths.anki_connect_url.as_deref()));
    if let Some(ref url) = anki_connect_url {
        println!("🌐 AnkiConnect URL: {}", url);
    }

    let max_llm_calls = cfg.llm.concurrency.max_llm_calls;
    let max_post_process = cfg.llm.concurrency.max_post_process;

    // ── JWKS fetch for auth ──
    let (jwks_keys, jwks_url) = if cfg.auth_enabled() {
        let supabase_url = std::env::var("SUPABASE_URL")
            .expect("SUPABASE_URL required when auth.enabled = true");
        let url = format!("{}/auth/v1/.well-known/jwks.json", supabase_url);
        log::info!("Fetching JWKS from {}", url);

        let resp: serde_json::Value = reqwest::get(&url).await
            .expect("Failed to fetch JWKS")
            .json().await
            .expect("Failed to parse JWKS");

        let keys = resp["keys"].as_array().expect("JWKS 'keys' is not an array");
        let mut jwks_map = HashMap::new();
        for key in keys {
            if let Some(kid) = key["kid"].as_str() {
                if let Some(entry) = parse_jwk(key) {
                    log::info!("Loaded JWKS key kid={} alg={:?}", kid, entry.0);
                    jwks_map.insert(kid.to_string(), entry);
                }
            }
        }
        log::info!("Loaded {} JWKS key(s) total", jwks_map.len());
        (Arc::new(RwLock::new(jwks_map)), Some(url))
    } else {
        (Arc::new(RwLock::new(HashMap::new())), None)
    };

    // HS256 fallback: load JWT secret if available
    let jwt_hs256_key = if cfg.auth_enabled() {
        match std::env::var("SUPABASE_JWT_SECRET") {
            Ok(secret) => {
                log::info!("Loaded SUPABASE_JWT_SECRET for HS256 verification");
                Some(DecodingKey::from_secret(secret.as_bytes()))
            }
            Err(_) => {
                log::warn!("SUPABASE_JWT_SECRET not set — HS256 tokens will be rejected");
                None
            }
        }
    } else {
        None
    };

    let auth_enabled = cfg.auth_enabled();
    let pool = local_db.pool.clone();

    let app_state = web::Data::new(AppState {
        pipelines: RwLock::new(pipelines_map),
        llm_semaphore: Arc::new(Semaphore::new(max_llm_calls)),
        post_process_semaphore: Arc::new(Semaphore::new(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(max_post_process)
        )),
        defaults: cfg.defaults,
        generator_config: cfg.generator,
        llm_runtime: tokio::sync::RwLock::new(LlmRuntimeConfig {
            provider,
            model: active_model.clone(),
            api_key: api_key.clone(),
            base_url: base_url.clone(),
        }),
        llm_config: cfg.llm,
        output_dir: cfg.paths.output_dir,
        anki_connect_url: anki_connect_url.clone(),
        db_pool: pool.clone(),
        srs_registry: lc_core::srs::SrsRegistry::new(),
        auth_enabled,
        jwks_keys,
        jwks_url,
        jwks_last_refresh: Arc::new(std::sync::atomic::AtomicI64::new(now_ms())),
        jwt_hs256_key,
        known_users: dashmap::DashSet::new(),
    });

    // ── Background task: purge old drafts every 10 min ──
    spawn_draft_cleanup(pool);

    let bind_addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    println!("\n🚀 Server starting at http://{}", bind_addr);
    println!("   Open this URL in your browser to access the beta interface.\n");

    let static_path = if std::path::Path::new(&cfg.server.static_path).exists() {
        cfg.server.static_path.clone()
    } else if std::path::Path::new("static").exists() {
        "static".to_string()
    } else {
        eprintln!("⚠️  Static files directory not found at '{}'", cfg.server.static_path);
        cfg.server.static_path.clone()
    };

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/api/tree", web::get().to(get_tree))
            .route("/api/generate", web::post().to(generate_cards))
            .route("/api/generate-and-save", web::post().to(generate_and_save))
            .route("/api/export", web::post().to(export_deck))
            .route("/api/push-to-anki", web::post().to(push_to_anki))
            .route("/api/push-local", web::post().to(push_to_local_db))
            .route("/api/cards", web::get().to(get_cards))
            .route("/api/cards/clear", web::post().to(clear_cards))
            .route("/api/preview-prompt", web::post().to(preview_prompt))
            .route("/api/card-models", web::get().to(get_card_models))
            .route("/api/add-node", web::post().to(add_node))
            .route("/api/languages", web::get().to(get_languages))
            .route("/api/anki-decks", web::get().to(get_anki_decks))
            .route("/api/local-decks", web::get().to(get_local_decks))
            .route("/api/local-decks/export-apkg", web::post().to(export_db_to_apkg))
            .route("/api/local-decks/{deck_id}/study", web::get().to(get_study_session))
            .route("/api/local-decks/{deck_id}/review", web::post().to(submit_review))
            .route("/api/srs/algorithms", web::get().to(get_srs_algorithms))
            .route("/api/lexicon", web::get().to(get_lexicon))
            .route("/api/lexicon/all", web::get().to(get_lexicon_all))
            .route("/api/lexicon/status", web::get().to(get_lexicon_status))
            .route("/api/lexicon/rescan", web::post().to(rescan_lexicon))
            .route("/api/config/llm", web::get().to(get_llm_config))
            .route("/api/config/llm", web::put().to(update_llm_config))
            .route("/api/auth/config", web::get().to(get_auth_config))
            .route("/api/auth/login", web::post().to(post_auth_login))
            .service(Files::new("/", &static_path).index_file("index.html"))
    })
    .bind(&bind_addr)?
    .run()
    .await
}

// ═══════════════════════════════════════════════
//  Auth Endpoints
// ═══════════════════════════════════════════════

async fn get_auth_config(data: web::Data<AppState>) -> impl Responder {
    if !data.auth_enabled {
        return HttpResponse::Ok().json(serde_json::json!({ "enabled": false }));
    }

    let url = std::env::var("SUPABASE_URL").ok();
    let key = std::env::var("SUPABASE_ANON_KEY").ok();

    if url.is_none() || key.is_none() {
        log::error!("Auth enabled but SUPABASE_URL or SUPABASE_ANON_KEY not set");
        return HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "Auth misconfigured: missing SUPABASE_URL or SUPABASE_ANON_KEY"
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({
        "enabled": true,
        "supabase_url": url.unwrap(),
        "supabase_anon_key": key.unwrap(),
    }))
}

async fn post_auth_login(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let claims = match &auth.claims {
        Some(c) => c,
        None => return HttpResponse::Ok().json(serde_json::json!({"ok": true})),
    };
    let db = data.db_for(&auth);
    if let Err(e) = db.ensure_user(claims).await {
        log::error!("ensure_user failed for {}: {}", auth.user_id, e);
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": "User sync failed"}));
    }
    // Trigger lexicon scan for this user in background
    let user_id = auth.user_id.clone();
    let state = data.into_inner();
    let anki_url = state.anki_connect_url.clone();
    tokio::spawn(async move {
        scan_lexicon_background(state, anki_url, user_id).await;
    });

    HttpResponse::Ok().json(serde_json::json!({"ok": true}))
}

// ═══════════════════════════════════════════════
//  Background Tasks
// ═══════════════════════════════════════════════

fn spawn_draft_cleanup(pool: SqlitePool) {
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
async fn scan_lexicon_background(state: Arc<AppState>, anki_connect_url: Option<String>, user_id: String) {
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
    let pipelines = state.pipelines.read().await;
    for (iso, pipeline) in pipelines.iter() {
        match pipeline.load_lexicon(&snapshot).await {
            Ok(count) => println!("   ✅ {iso}: {count} words loaded into lexicon"),
            Err(e) => eprintln!("   ⚠️  {iso}: lexicon scan failed: {e}"),
        }
    }
    println!("📚 Background lexicon scan complete.");
}
