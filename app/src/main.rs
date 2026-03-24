use actix_files::Files;
use actix_web::{web, App, HttpResponse, HttpServer};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use std::collections::HashMap;

use jsonwebtoken::DecodingKey;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use lc_core::traits::Language;
use engine::llm_client::{LlmHttpClient, LlmProvider};
use engine::pipeline::{Pipeline, DynPipeline};
use engine::prompts::PromptConfig;
use engine::python_sidecar::PythonSidecar;

mod config;
pub mod state;
pub mod auth;
pub mod billing;
pub mod usage_analytics_impl;
pub mod rate_limit_impl;
pub mod worker;
pub mod api;

use state::{AppState, LlmRuntimeConfig, now_ms};
use auth::parse_jwk;

fn build_pipeline<L>(
    lang: L,
    cfg: &config::AppConfig,
    make_client: &dyn Fn(&str) -> LlmHttpClient,
    sidecar: &Arc<tokio::sync::Mutex<PythonSidecar>>,
    prompt_config: &PromptConfig,
    recorder: &engine::usage::UsageRecorder,
    provider: &LlmProvider,
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
    let model = cfg.llm.active_model().expect("model already validated at startup");
    let base_client = make_client(model);
    let instrumented = engine::usage::InstrumentedLlmClient::new(
        base_client, recorder.clone(), provider.to_string(), model.to_string(),
    );
    let mut pipeline = Pipeline::new(
        lang, Box::new(instrumented),
        cfg.generator.temperature, cfg.generator.max_tokens,
        cfg.feature_extractor.temperature, cfg.feature_extractor.max_tokens,
        prompt_config.clone(),
    );
    pipeline.set_usage_recorder(recorder.clone());
    pipeline.add_early_processor(Box::new(engine::post_process::IpaGenerator::new(sidecar.clone())));
    pipeline.add_early_processor(Box::new(engine::post_process::TtsGenerator::new(sidecar.clone())));
    Box::new(pipeline)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with_target(false)
        .init();
    tracing::info!("Panglot — Beta Test Server starting");

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

    tracing::info!(provider = %provider, model = %active_model, base_url = %base_url, "LLM configuration loaded");

    // Setup Database
    let db_path = std::path::Path::new(&cfg.paths.output_dir).join("panglot.db");
    let local_db = lc_core::db::LocalStorageProvider::init(&db_path)
        .await
        .expect("Failed to initialize SQLite database");

    // Spawn a single Python sidecar for IPA + TTS (shared across all languages)
    let sidecar = Arc::new(tokio::sync::Mutex::new(
        PythonSidecar::spawn().expect("Failed to start Python sidecar")
    ));

    // ── Start billing writer (usage_logs) ──
    let recorder = billing::start_billing_writer(local_db.pool.clone());

    let mut pipelines_map: HashMap<String, Box<dyn DynPipeline>> = HashMap::new();

    // Helper: create an LLM client for a given model
    let make_client = |model: &str| -> LlmHttpClient {
        LlmHttpClient::custom(api_key.clone(), base_url.clone(), model.to_string(), provider)
    };

    // ── Build pipelines for all registered languages ──
    for &iso in langs::ALL_ISO_CODES {
        if let Some(pipeline) = langs::dispatch_iso!(iso, lang => {
            build_pipeline(lang, &cfg, &make_client, &sidecar, &prompt_config, &recorder, &provider)
        }) {
            tracing::info!(language = pipeline.language_name(), iso, "Loaded language pipeline");
            pipelines_map.insert(iso.to_string(), pipeline);
        }
    }

    if pipelines_map.is_empty() {
        tracing::error!("No languages registered!");
        std::process::exit(1);
    }

    // ── Resolve AnkiConnect URL ──
    let anki_connect_url = Some(config::resolve_anki_connect_url(cfg.paths.anki_connect_url.as_deref()));
    if let Some(ref url) = anki_connect_url {
        tracing::info!(url, "AnkiConnect URL configured");
    }

    let max_llm_calls = cfg.llm.concurrency.max_llm_calls;
    let max_post_process = cfg.llm.concurrency.max_post_process;

    // ── JWKS fetch for auth ──
    let (jwks_keys, jwks_url) = if cfg.auth_enabled() {
        let supabase_url = std::env::var("SUPABASE_URL")
            .expect("SUPABASE_URL required when auth.enabled = true");
        let url = format!("{}/auth/v1/.well-known/jwks.json", supabase_url);
        tracing::info!(%url, "Fetching JWKS");

        let resp: serde_json::Value = reqwest::get(&url).await
            .expect("Failed to fetch JWKS")
            .json().await
            .expect("Failed to parse JWKS");

        let keys = resp["keys"].as_array().expect("JWKS 'keys' is not an array");
        let mut jwks_map = HashMap::new();
        for key in keys {
            if let Some(kid) = key["kid"].as_str() {
                if let Some(entry) = parse_jwk(key) {
                    tracing::info!(kid, alg = ?entry.0, "Loaded JWKS key");
                    jwks_map.insert(kid.to_string(), entry);
                }
            }
        }
        tracing::info!(count = jwks_map.len(), "Loaded JWKS keys");
        (Arc::new(RwLock::new(jwks_map)), Some(url))
    } else {
        (Arc::new(RwLock::new(HashMap::new())), None)
    };

    // HS256 fallback: load JWT secret if available
    let jwt_hs256_key = if cfg.auth_enabled() {
        match std::env::var("SUPABASE_JWT_SECRET") {
            Ok(secret) => {
                tracing::info!("Loaded SUPABASE_JWT_SECRET for HS256 verification");
                Some(DecodingKey::from_secret(secret.as_bytes()))
            }
            Err(_) => {
                tracing::warn!("SUPABASE_JWT_SECRET not set — HS256 tokens will be rejected");
                None
            }
        }
    } else {
        None
    };

    // ── Rate limiter ──
    let rate_limiter: Option<Box<dyn lc_core::rate_limit::RateLimiter>> = if cfg.rate_limits.enabled {
        tracing::info!(
            free_daily_tokens = cfg.rate_limits.free.daily_token_limit,
            free_hourly_calls = cfg.rate_limits.free.hourly_call_limit,
            free_daily_tts_chars = cfg.rate_limits.free.daily_tts_char_limit,
            premium_daily_tokens = cfg.rate_limits.premium.daily_token_limit,
            "Rate limiting enabled"
        );
        Some(Box::new(rate_limit_impl::SqliteRateLimiter::from_config(
            local_db.pool.clone(),
            &cfg.rate_limits,
        )))
    } else {
        tracing::info!("Rate limiting disabled");
        None
    };

    let auth_enabled = cfg.auth_enabled();
    let pool = local_db.pool.clone();

    let admin_user_ids: std::collections::HashSet<String> = std::env::var("PANGLOT_ADMIN_IDS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let app_state = web::Data::new(AppState {
        pipelines: RwLock::new(pipelines_map),
        user_lexicons: RwLock::new(HashMap::new()),
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
        admin_user_ids,
        jwks_keys,
        jwks_url,
        jwks_last_refresh: Arc::new(std::sync::atomic::AtomicI64::new(now_ms())),
        jwt_hs256_key,
        known_users: dashmap::DashSet::new(),
        rate_limiter,
    });

    // ── Background task: purge old drafts every 10 min ──
    worker::spawn_draft_cleanup(pool);

    let bind_addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    tracing::info!(bind_addr = %bind_addr, "Server starting");

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "Panglot API",
            description = "Language learning card generation backend",
            version = "0.1.0",
        ),
        components(schemas(
            api::models::GenerateRequest,
            api::models::GenerateResponse,
            api::models::GeneratedCardJson,
            api::models::ExportResponse,
            api::models::PreviewPromptRequest,
            api::models::PreviewPromptResponse,
            api::models::PreviewSchemas,
            api::models::PromptMessageJson,
            api::models::AddNodeRequest,
            api::models::AddNodeResponse,
            api::models::HideNodeRequest,
            api::models::EditNodeRequest,
            api::models::ReviewOutcomeBody,
            api::models::UpdateLlmConfigRequest,
            api::models::TreeNodeJson,
            lc_core::validated::CardCount,
            lc_core::validated::Difficulty,
            lc_core::validated::UserPrompt,
            lc_core::validated::NodeName,
            lc_core::validated::NodeInstructions,
            api::usage::UsageSummary,
            api::usage::PeriodUsage,
        ))
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    let static_path = if std::path::Path::new(&cfg.server.static_path).exists() {
        cfg.server.static_path.clone()
    } else if std::path::Path::new("static").exists() {
        "static".to_string()
    } else {
        tracing::warn!(path = %cfg.server.static_path, "Static files directory not found");
        cfg.server.static_path.clone()
    };

    HttpServer::new(move || {
        let json_cfg = web::JsonConfig::default()
            .limit(65_536) // 64 KB max JSON body
            .error_handler(|err, _req| {
                let response = HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "error": "validation_error",
                    "message": err.to_string(),
                }));
                actix_web::error::InternalError::from_response(err, response).into()
            });

        App::new()
            .wrap(tracing_actix_web::TracingLogger::default())
            .app_data(app_state.clone())
            .app_data(json_cfg)
            .service(SwaggerUi::new("/api/docs/{_:.*}").url("/api/docs/openapi.json", openapi.clone()))
            .route("/api/tree", web::get().to(api::tree::get_tree))
            .route("/api/generate", web::post().to(api::generation::generate_cards))
            .route("/api/generate-and-save", web::post().to(api::generation::generate_and_save))
            .route("/api/export", web::post().to(api::export::export_deck))
            .route("/api/push-to-anki", web::post().to(api::export::push_to_anki))
            .route("/api/push-local", web::post().to(api::export::push_to_local_db))
            .route("/api/cards", web::get().to(api::decks::get_cards))
            .route("/api/cards/clear", web::post().to(api::decks::clear_cards))
            .route("/api/preview-prompt", web::post().to(api::generation::preview_prompt))
            .route("/api/card-models", web::get().to(api::config::get_card_models))
            .route("/api/add-node", web::post().to(api::tree::add_node))
            .route("/api/hide-node", web::post().to(api::tree::hide_node))
            .route("/api/edit-node", web::post().to(api::tree::edit_node))
            .route("/api/tree-customization", web::delete().to(api::tree::delete_customization))
            .route("/api/languages", web::get().to(api::config::get_languages))
            .route("/api/anki-decks", web::get().to(api::decks::get_anki_decks))
            .route("/api/local-decks", web::get().to(api::decks::get_local_decks))
            .route("/api/local-decks/{deck_id}", web::delete().to(api::decks::delete_deck))
            .route("/api/local-decks/export-apkg", web::post().to(api::decks::export_db_to_apkg))
            .route("/api/local-decks/{deck_id}/study", web::get().to(api::study::get_study_session))
            .route("/api/local-decks/{deck_id}/review", web::post().to(api::study::submit_review))
            .route("/api/srs/algorithms", web::get().to(api::study::get_srs_algorithms))
            .route("/api/lexicon", web::get().to(api::lexicon::get_lexicon))
            .route("/api/lexicon/all", web::get().to(api::lexicon::get_lexicon_all))
            .route("/api/lexicon/status", web::get().to(api::lexicon::get_lexicon_status))
            .route("/api/lexicon/rescan", web::post().to(api::lexicon::rescan_lexicon))
            .route("/api/config/llm", web::get().to(api::config::get_llm_config))
            .route("/api/config/llm", web::put().to(api::config::update_llm_config))
            .route("/api/auth/config", web::get().to(auth::get_auth_config))
            .route("/api/auth/login", web::post().to(auth::post_auth_login))
            .route("/api/user/settings", web::get().to(api::config::get_user_settings))
            .route("/api/user/settings", web::post().to(api::config::update_user_settings))
            .route("/api/usage/summary", web::get().to(api::usage::get_usage_summary))
            .route("/api/usage/details", web::get().to(api::usage::get_usage_details))
            .route("/api/audio/{filename}", web::get().to(api::audio::get_audio))
            .service(Files::new("/", &static_path).index_file("index.html"))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
