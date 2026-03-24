use actix_web::{web, HttpRequest, HttpResponse, Responder};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;

use crate::state::{AppState, now_ms};
use crate::worker::scan_lexicon_background;

/// Parse a single JWK into an (Algorithm, DecodingKey) pair.
pub fn parse_jwk(key: &serde_json::Value) -> Option<(Algorithm, DecodingKey)> {
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

pub const JWKS_REFRESH_COOLDOWN_MS: i64 = 60_000;

pub async fn refresh_jwks(state: &AppState) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UserRole {
    Free,
    Premium,
    Admin,
}

impl UserRole {
    pub fn from_claims(claims: &serde_json::Value) -> Self {
        claims.get("app_metadata")
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "admin" => Self::Admin,
                "premium" => Self::Premium,
                _ => Self::Free,
            })
            .unwrap_or(Self::Free)
    }

    pub fn is_admin(&self) -> bool { *self == Self::Admin }
    pub fn is_premium_or_above(&self) -> bool { *self >= Self::Premium }
}

pub struct AuthUser {
    pub user_id: String,
    pub claims: Option<serde_json::Value>,
    pub role: UserRole,
}

pub struct AdminUser(pub AuthUser);

impl actix_web::FromRequest for AdminUser {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut actix_web::dev::Payload) -> Self::Future {
        let auth_future = AuthUser::from_request(req, payload);
        Box::pin(async move {
            let auth = auth_future.await?;
            if !auth.role.is_admin() {
                return Err(actix_web::error::ErrorForbidden("Forbidden - Admin access required"));
            }
            Ok(AdminUser(auth))
        })
    }
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
                return Ok(AuthUser { 
                    user_id: "default-user".into(), 
                    claims: None,
                    role: UserRole::Admin, 
                });
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

            let mut role = UserRole::from_claims(&data.claims);
            if state.admin_user_ids.contains(&user_id) {
                role = UserRole::Admin;
            }

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

            Ok(AuthUser { user_id, claims: Some(data.claims), role })
        })
    }
}

// ═══════════════════════════════════════════════
//  Auth Endpoints
// ═══════════════════════════════════════════════

pub async fn get_auth_config(data: web::Data<AppState>) -> impl Responder {
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

pub async fn post_auth_login(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
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
