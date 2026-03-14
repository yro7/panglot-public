use actix_web::{web, HttpResponse, Responder};

use crate::state::AppState;
use super::models::{GetTreeQuery, AddNodeRequest, AddNodeResponse, tree_node_to_json};
use engine::skill_tree::SkillNode;

pub async fn get_tree(
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

pub async fn add_node(
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
