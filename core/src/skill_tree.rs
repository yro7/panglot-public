use serde::Deserialize;

/// Top-level configuration for a skill tree, loaded from YAML.
#[derive(Debug, Deserialize)]
pub struct SkillTreeConfig {
    pub language_name: String,
    pub root: SkillNodeConfig,
}

/// A node in the skill tree configuration.
/// `node_instructions` holds optional LLM instructions specific to this node.
/// The card model is chosen by the user at runtime, not here.
#[derive(Debug, Deserialize)]
pub struct SkillNodeConfig {
    pub id: String,
    pub name: String,
    pub node_instructions: Option<String>,
    pub children: Vec<SkillNodeConfig>,
}
