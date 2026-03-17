use actix_web::{web, HttpResponse, Responder};
use engine::llm_client::{LlmProvider, LlmHttpClient};
use engine::card_models::CardModelId;
use lc_core::user::UserSettings;

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{UpdateLlmConfigRequest, CardModelQuery};

pub async fn get_llm_config(data: web::Data<AppState>) -> impl Responder {
    let rt = data.llm_runtime.read().await;
    HttpResponse::Ok().json(serde_json::json!({
        "provider": rt.provider.to_string(),
        "model": rt.model,
        "available_models": data.llm_config.models,
    }))
}

pub async fn update_llm_config(
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
        let languages = data.languages.read().await;
        for rt in languages.values() {
            rt.pipeline.swap_llm_client(new_client_factory()).await;
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

pub async fn get_card_models(
    data: web::Data<AppState>,
    query: web::Query<CardModelQuery>,
) -> impl Responder {
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);
    let languages = data.languages.read().await;

    let models: Vec<String> = if let Some(runtime) = languages.get(lang) {
        runtime.pipeline.available_models().into_iter().map(|id| id.to_string()).collect()
    } else {
        CardModelId::ALL.iter().map(|id| id.to_string()).collect()
    };

    HttpResponse::Ok().json(serde_json::json!({ "models": models }))
}

pub async fn get_languages(data: web::Data<AppState>) -> impl Responder {
    let languages = data.languages.read().await;
    let list: Vec<serde_json::Value> = languages.iter().map(|(iso, rt)| {
        serde_json::json!({ "iso": iso, "name": rt.pipeline.language_name() })
    }).collect();
    HttpResponse::Ok().json(list)
}

pub async fn get_user_settings(auth: AuthUser, data: web::Data<AppState>) -> impl Responder {
    let db = data.db_for(&auth);
    match db.get_user_settings().await {
        Ok(settings) => HttpResponse::Ok().json(settings),
        Err(e) => {
            log::error!("Failed to fetch user settings for {}: {}", auth.user_id, e);
            HttpResponse::Ok().json(UserSettings::default())
        }
    }
}

pub async fn update_user_settings(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<UserSettings>,
) -> impl Responder {
    let db = data.db_for(&auth);
    let settings = body.into_inner();
    println!("[Settings] Receiving settings update for {}: {:?}", auth.user_id, settings);

    match db.update_user_settings(&settings).await {
        Ok(_) => {
            println!("[Settings] Update successful");
            HttpResponse::Ok().json(serde_json::json!({"success": true}))
        },
        Err(e) => {
            println!("[Settings] Update failed: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to update user settings: {}", e)
            }))
        }
    }
}
