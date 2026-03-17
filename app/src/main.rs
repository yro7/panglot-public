use std::fs;
use actix_files::Files;
use actix_web::{web, App, HttpServer};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use std::collections::HashMap;

use jsonwebtoken::DecodingKey;

use lc_core::traits::Language;
use engine::llm_client::{LlmHttpClient, LlmProvider};
use engine::pipeline::{DynPipeline, Pipeline};
use engine::prompts::PromptConfig;
use engine::python_sidecar::PythonSidecar;
use engine::skill_tree::{SkillTree, SkillTreeConfig};

mod config;
pub mod state;
pub mod auth;
pub mod worker;
pub mod api;

use state::{AppState, LlmRuntimeConfig, now_ms};
use auth::parse_jwk;

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
    worker::spawn_draft_cleanup(pool);

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
            .route("/api/audio/{filename}", web::get().to(api::audio::get_audio))
            .service(Files::new("/", &static_path).index_file("index.html"))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
