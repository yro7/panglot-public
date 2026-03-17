use actix_web::{web, HttpResponse, Responder};
use lc_core::db::UserTreeCustomization;
use engine::skill_tree::{self, TreeCustomization};

use crate::state::AppState;
use crate::auth::AuthUser;
use super::models::{
    GetTreeQuery, AddNodeRequest, AddNodeResponse,
    HideNodeRequest, EditNodeRequest, DeleteCustomizationQuery,
    tree_node_to_json,
};

/// Builds the user's personalized tree: base tree + DB overlay.
/// Returns the owned SkillNode root. If no customizations exist, clones the base tree.
pub async fn build_user_tree(
    data: &AppState,
    auth: &AuthUser,
    lang: &str,
    base_tree: &engine::skill_tree::SkillNode,
) -> engine::skill_tree::SkillNode {
    let db = data.db_for(auth);
    let customizations = db.get_tree_customizations(lang).await.unwrap_or_default();
    if customizations.is_empty() {
        return base_tree.clone();
    }
    let tree_customizations: Vec<TreeCustomization> = customizations.into_iter().map(|c| TreeCustomization {
        node_id: c.node_id,
        action: c.action,
        parent_id: c.parent_id,
        node_name: c.node_name,
        node_instructions: c.node_instructions,
    }).collect();
    skill_tree::apply_customizations(base_tree, &tree_customizations)
}

pub async fn get_tree(
    auth: AuthUser,
    data: web::Data<AppState>,
    query: web::Query<GetTreeQuery>,
) -> impl Responder {
    let languages = data.languages.read().await;
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);

    let Some(runtime) = languages.get(lang) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Language '{}' not found", lang)
        }));
    };

    let user_tree = build_user_tree(&data, &auth, lang, &runtime.base_tree).await;
    HttpResponse::Ok().json(tree_node_to_json(&user_tree))
}

pub async fn add_node(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<AddNodeRequest>,
) -> impl Responder {
    let languages = data.languages.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(runtime) = languages.get(lang) else {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
        });
    };

    // Build the user's current tree to validate against
    let user_tree = build_user_tree(&data, &auth, lang, &runtime.base_tree).await;

    // Check for duplicate node_id
    if skill_tree::find_node(&user_tree, &body.node_id).is_some() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Node ID '{}' already exists in the tree", body.node_id),
        });
    }

    // Check parent exists
    if skill_tree::find_node(&user_tree, &body.parent_id).is_none() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Parent node '{}' not found in tree", body.parent_id),
        });
    }

    let db = data.db_for(&auth);
    let customization = UserTreeCustomization {
        user_id: auth.user_id.clone(),
        language: lang.to_string(),
        node_id: body.node_id.clone(),
        action: "add".to_string(),
        parent_id: Some(body.parent_id.clone()),
        node_name: Some(body.node_name.clone()),
        node_instructions: body.node_instructions.clone(),
        sort_order: 0,
        created_at: crate::state::now_ms(),
    };

    match db.upsert_tree_customization(&customization).await {
        Ok(_) => HttpResponse::Ok().json(AddNodeResponse {
            success: true,
            message: format!("Node '{}' added under '{}'", body.node_name, body.parent_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(AddNodeResponse {
            success: false,
            message: format!("Failed to save customization: {}", e),
        }),
    }
}

pub async fn hide_node(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<HideNodeRequest>,
) -> impl Responder {
    let languages = data.languages.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(runtime) = languages.get(lang) else {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
        });
    };

    // Verify node exists in the user's current tree
    let user_tree = build_user_tree(&data, &auth, lang, &runtime.base_tree).await;
    if skill_tree::find_node(&user_tree, &body.node_id).is_none() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Node '{}' not found in tree", body.node_id),
        });
    }

    let db = data.db_for(&auth);
    let customization = UserTreeCustomization {
        user_id: auth.user_id.clone(),
        language: lang.to_string(),
        node_id: body.node_id.clone(),
        action: "hide".to_string(),
        parent_id: None,
        node_name: None,
        node_instructions: None,
        sort_order: 0,
        created_at: crate::state::now_ms(),
    };

    match db.upsert_tree_customization(&customization).await {
        Ok(_) => HttpResponse::Ok().json(AddNodeResponse {
            success: true,
            message: format!("Node '{}' hidden", body.node_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(AddNodeResponse {
            success: false,
            message: format!("Failed to save customization: {}", e),
        }),
    }
}

pub async fn edit_node(
    auth: AuthUser,
    data: web::Data<AppState>,
    body: web::Json<EditNodeRequest>,
) -> impl Responder {
    let languages = data.languages.read().await;
    let lang = body.language.as_deref().unwrap_or(&data.defaults.language);

    let Some(runtime) = languages.get(lang) else {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Language '{}' not found", lang),
        });
    };

    if body.node_name.is_none() && body.node_instructions.is_none() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: "At least one of node_name or node_instructions must be provided".to_string(),
        });
    }

    let user_tree = build_user_tree(&data, &auth, lang, &runtime.base_tree).await;
    if skill_tree::find_node(&user_tree, &body.node_id).is_none() {
        return HttpResponse::BadRequest().json(AddNodeResponse {
            success: false,
            message: format!("Node '{}' not found in tree", body.node_id),
        });
    }

    let db = data.db_for(&auth);
    let customization = UserTreeCustomization {
        user_id: auth.user_id.clone(),
        language: lang.to_string(),
        node_id: body.node_id.clone(),
        action: "edit".to_string(),
        parent_id: None,
        node_name: body.node_name.clone(),
        node_instructions: body.node_instructions.clone(),
        sort_order: 0,
        created_at: crate::state::now_ms(),
    };

    match db.upsert_tree_customization(&customization).await {
        Ok(_) => HttpResponse::Ok().json(AddNodeResponse {
            success: true,
            message: format!("Node '{}' updated", body.node_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(AddNodeResponse {
            success: false,
            message: format!("Failed to save customization: {}", e),
        }),
    }
}

pub async fn delete_customization(
    auth: AuthUser,
    data: web::Data<AppState>,
    query: web::Query<DeleteCustomizationQuery>,
) -> impl Responder {
    let lang = query.lang.as_deref().unwrap_or(&data.defaults.language);
    let db = data.db_for(&auth);

    match db.delete_tree_customization(lang, &query.node_id).await {
        Ok(true) => HttpResponse::Ok().json(AddNodeResponse {
            success: true,
            message: format!("Customization for '{}' removed", query.node_id),
        }),
        Ok(false) => HttpResponse::NotFound().json(AddNodeResponse {
            success: false,
            message: format!("No customization found for '{}'", query.node_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(AddNodeResponse {
            success: false,
            message: format!("Failed to delete customization: {}", e),
        }),
    }
}
