use include_dir::{include_dir, Dir};
use serde::Deserialize;

/// Skill-tree YAML files embedded at compile time from `core/trees/`.
static TREES: Dir = include_dir!("$CARGO_MANIFEST_DIR/trees");

/// Resolve the default skill-tree config for a language by its ISO 639-3 code.
pub fn resolve_config(iso_639_3: &str) -> SkillTreeConfig {
    let filename = format!("{iso_639_3}_tree.yaml");
    let file = TREES
        .get_file(&filename)
        .unwrap_or_else(|| panic!("no embedded skill-tree for language '{iso_639_3}'"));
    let yaml = file
        .contents_utf8()
        .unwrap_or_else(|| panic!("skill-tree for '{iso_639_3}' is not valid UTF-8"));
    serde_yaml::from_str(yaml)
        .unwrap_or_else(|e| panic!("invalid skill-tree YAML for '{iso_639_3}': {e}"))
}

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
