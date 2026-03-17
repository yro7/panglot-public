use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use tokio::sync::{RwLock, Semaphore};
use sqlx::SqlitePool;
use jsonwebtoken::{Algorithm, DecodingKey};
use dashmap::DashSet;

use lc_core::srs::SrsRegistry;
use engine::pipeline::DynPipeline;
use engine::skill_tree::SkillNode;
use engine::llm_client::LlmProvider;

use crate::config::{DefaultsConfig, LlmCallConfig, LlmConfig};
use crate::auth::AuthUser;

/// Groups a pipeline (generation engine) with its base skill tree.
/// The pipeline is stateless w.r.t. the tree — the tree is injected at each call.
pub struct LanguageRuntime {
    pub pipeline: Box<dyn DynPipeline>,
    pub base_tree: SkillNode,
}

pub struct AppState {
    pub languages: RwLock<HashMap<String, LanguageRuntime>>,
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
