use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use lc_core::user::UserSettings;
use engine::skill_tree::SkillNode;

#[derive(Serialize)]
pub struct TreeNodeJson {
    pub id: String,
    pub name: String,
    pub is_leaf: bool,
    pub node_instructions: Option<String>,
    pub children: Vec<TreeNodeJson>,
}

#[derive(Serialize, Clone)]
pub struct GeneratedCardJson {
    pub card_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub template_name: String,
    pub fields: HashMap<String, String>,
    pub explanation: String,
    pub metadata_json: String,
}

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub language: Option<String>,
    pub node_id: String,
    pub card_model_id: Option<String>,
    pub card_count: Option<u32>,
    pub difficulty: Option<u8>,
    pub user_prompt: Option<String>,
    pub user_profile: UserSettings,
    pub lexicon_options: Option<engine::generator::LexiconOption>,
}

#[derive(Serialize)]
pub struct GenerateResponse {
    pub success: bool,
    pub cards: Vec<GeneratedCardJson>,
    pub message: String,
}

#[derive(Serialize)]
pub struct ExportResponse {
    pub success: bool,
    pub message: String,
    pub file_path: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewPromptRequest {
    pub language: Option<String>,
    pub node_id: String,
    pub card_model_id: Option<String>,
    pub difficulty: Option<u8>,
    pub user_profile: Option<UserSettings>,
    pub lexicon_options: Option<engine::generator::LexiconOption>,
}

#[derive(Serialize)]
pub struct PreviewPromptResponse {
    pub messages: Vec<PromptMessageJson>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub schemas: PreviewSchemas,
}

#[derive(Serialize)]
pub struct PreviewSchemas {
    pub call_1_content_generator: serde_json::Value,
    pub call_2_feature_extractor: serde_json::Value,
}

#[derive(Serialize)]
pub struct PromptMessageJson {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct AddNodeRequest {
    pub language: Option<String>,
    pub parent_id: String,
    pub node_id: String,
    pub node_name: String,
    pub node_instructions: Option<String>,
}

#[derive(Serialize)]
pub struct AddNodeResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct HideNodeRequest {
    pub language: Option<String>,
    pub node_id: String,
}

#[derive(Deserialize)]
pub struct EditNodeRequest {
    pub language: Option<String>,
    pub node_id: String,
    pub node_name: Option<String>,
    pub node_instructions: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteCustomizationQuery {
    pub lang: Option<String>,
    pub node_id: String,
}

#[derive(Deserialize)]
pub struct GetTreeQuery {
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct CardModelQuery {
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct LexiconQuery {
    pub lang: Option<String>,
    pub pos: Option<String>,
}

#[derive(Deserialize)]
pub struct ReviewOutcomeBody {
    pub card_id: String,
    pub rating: String,
}

#[derive(Deserialize)]
pub struct UpdateLlmConfigRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
}

pub fn tree_node_to_json(node: &SkillNode) -> TreeNodeJson {
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
