use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use tokio::sync::{RwLock, Semaphore};
use sqlx::SqlitePool;
use jsonwebtoken::{Algorithm, DecodingKey};
use dashmap::DashSet;

use lc_core::srs::SrsRegistry;
use engine::analyzer::DynLexiconTracker;
use engine::pipeline::{DynPipeline, LexiconStatus};
use engine::llm_client::LlmProvider;

use crate::config::{DefaultsConfig, LlmCallConfig, LlmConfig};
use crate::auth::AuthUser;

/// Per-user lexicon data, keyed by ISO language code.
pub struct UserLexicon {
    /// ISO code → type-erased tracker (Arc for cheap cloning under lock).
    pub trackers: HashMap<String, Arc<dyn DynLexiconTracker>>,
    /// ISO code → scan status.
    pub statuses: HashMap<String, LexiconStatus>,
}

pub struct AppState {
    pub pipelines: RwLock<HashMap<String, Box<dyn DynPipeline>>>,
    /// Per-user lexicon storage: user_id → UserLexicon.
    pub user_lexicons: RwLock<HashMap<String, UserLexicon>>,
    pub llm_semaphore: Arc<Semaphore>,
    pub post_process_semaphore: Arc<Semaphore>,
    pub defaults: DefaultsConfig,
    pub generator_config: LlmCallConfig,
    pub llm_runtime: RwLock<LlmRuntimeConfig>,
    pub llm_config: LlmConfig,
    pub output_dir: String,
    pub anki_connect_url: Option<String>,
    pub db_pool: SqlitePool,
    pub srs_registry: SrsRegistry,
    pub auth_enabled: bool,
    pub jwks_keys: Arc<RwLock<HashMap<String, (Algorithm, DecodingKey)>>>,
    pub jwks_url: Option<String>,
    pub jwks_last_refresh: Arc<AtomicI64>,
    pub jwt_hs256_key: Option<DecodingKey>,
    pub known_users: DashSet<String>,
    pub rate_limiter: Option<Box<dyn lc_core::rate_limit::RateLimiter>>,
}

pub struct LlmRuntimeConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}

impl AppState {
    pub fn db_for(&self, user: &AuthUser) -> lc_core::db::LocalStorageProvider {
        lc_core::db::LocalStorageProvider::for_user(self.db_pool.clone(), user.user_id.clone())
    }

    /// Checks rate limits for a user. Returns `Ok(())` if within limits,
    /// or an `HttpResponse` (429 or 500) to return early.
    pub async fn check_rate_limit(&self, user_id: &str) -> Result<(), actix_web::HttpResponse> {
        let Some(ref limiter) = self.rate_limiter else {
            return Ok(());
        };
        match limiter.check_limits(user_id).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(exceeded)) => Err(actix_web::HttpResponse::TooManyRequests().json(
                serde_json::json!({
                    "error": "rate_limit_exceeded",
                    "message": exceeded.to_string(),
                    "limit_kind": exceeded.kind,
                    "current_usage": exceeded.current_usage,
                    "max_allowed": exceeded.max_allowed,
                }),
            )),
            Err(e) => {
                tracing::error!(%e, "Rate limiter check failed");
                Err(actix_web::HttpResponse::InternalServerError().json(
                    serde_json::json!({
                        "error": "rate_limit_check_failed",
                        "message": "Could not verify rate limits",
                    }),
                ))
            }
        }
    }

    pub async fn srs_for(&self, auth: &AuthUser) -> &dyn lc_core::srs::SrsAlgorithm {
        let db = self.db_for(auth);
        match db.get_user_settings().await {
            Ok(settings) => self.srs_registry.get(&settings.srs_algorithm).unwrap_or(self.srs_registry.default()),
            Err(_) => self.srs_registry.default(),
        }
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
