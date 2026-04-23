use include_dir::{Dir, include_dir};
use serde::Deserialize;

/// Skill-tree YAML files embedded at compile time from `core/trees/`.
static TREES: Dir = include_dir!("$CARGO_MANIFEST_DIR/trees");

/// Resolve the default skill-tree config for a language by its ISO 639-3 code.
///
/// # Panics
///
/// Panics if no matching YAML file is found in the embedded trees directory,
/// or if the file contains invalid UTF-8 or YAML.
pub fn resolve_config(iso_639_3: &str) -> SkillTreeConfig {
    resolve_tree_definitions(iso_639_3)
        .into_iter()
        .find(|cfg| cfg.is_default)
        .unwrap_or_else(|| panic!("no default skill-tree for language '{iso_639_3}'"))
}

/// Resolve all embedded tree definitions for a language.
///
/// # Panics
///
/// Panics if any matching YAML file is invalid.
pub fn resolve_tree_definitions(iso_639_3: &str) -> Vec<SkillTreeConfig> {
    let mut configs: Vec<SkillTreeConfig> = TREES
        .files()
        .filter_map(|file| {
            let filename = file.path().file_name()?.to_str()?;
            if !belongs_to_language(filename, iso_639_3) {
                return None;
            }
            let yaml = file.contents_utf8().unwrap_or_else(|| {
                panic!("skill-tree for '{iso_639_3}' is not valid UTF-8");
            });
            let cfg: SkillTreeConfig = serde_yml::from_str(yaml).unwrap_or_else(|e| {
                panic!("invalid skill-tree YAML for '{iso_639_3}' in '{filename}': {e}")
            });
            Some(cfg)
        })
        .collect();

    if configs.is_empty() {
        panic!("no embedded skill-tree for language '{iso_639_3}'");
    }

    configs.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.tree_key.cmp(&b.tree_key))
            .then_with(|| a.tree_version.cmp(&b.tree_version))
    });
    configs
}

/// Resolve a specific embedded tree definition by id.
pub fn resolve_tree_definition(iso_639_3: &str, tree_id: &str) -> Option<SkillTreeConfig> {
    resolve_tree_definitions(iso_639_3)
        .into_iter()
        .find(|cfg| cfg.tree_id == tree_id)
}

fn belongs_to_language(filename: &str, iso_639_3: &str) -> bool {
    if !filename.ends_with("_tree.yaml") {
        return false;
    }
    filename == format!("{iso_639_3}_tree.yaml") || filename.starts_with(&format!("{iso_639_3}_"))
}

/// Top-level configuration for a skill tree, loaded from YAML.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillTreeConfig {
    pub tree_id: String,
    pub tree_key: String,
    pub tree_slug: String,
    pub tree_version: u32,
    pub tree_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    pub language_name: String,
    pub root: SkillNodeConfig,
}

/// A node in the skill tree configuration.
/// `node_instructions` holds optional LLM instructions specific to this node.
/// `prerequisites` holds IDs of other nodes that should be learned first (DAG edges).
/// The card model is chosen by the user at runtime, not here.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillNodeConfig {
    pub id: String,
    #[serde(default)]
    pub skill_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub node_instructions: Option<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub children: Vec<Self>,
    #[serde(default)]
    pub concept_key: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
}
