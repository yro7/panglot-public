use lc_core::traits::Language;

// Re-export config types from lc_core (moved there so `langs` can reference them)
pub use lc_core::skill_tree::{SkillTreeConfig, SkillNodeConfig};

// ----- Runtime Layer -----

/// A runtime skill node. No longer generic — the Language parameter
/// lives only on `SkillTree`, keeping nodes lightweight and composable.
#[derive(Clone)]
pub struct SkillNode {
    pub id: String,
    pub name: String,
    pub node_instructions: Option<String>,
    pub children: Vec<SkillNode>,
}

/// The runtime skill tree, parameterized by a Language.
pub struct SkillTree<L: Language> {
    pub language: L,
    pub root: SkillNode,
}

impl<L: Language> SkillTree<L> {
    /// Creates a new SkillTree from a language instance and top-level config.
    pub fn from_config(language: L, config: SkillTreeConfig) -> Self {
        Self {
            language,
            root: build_node(config.root),
        }
    }

    /// Creates a SkillTree from a raw SkillNodeConfig (without the SkillTreeConfig wrapper).
    pub fn new(language: L, config: SkillNodeConfig) -> Self {
        Self {
            language,
            root: build_node(config),
        }
    }

    /// Finds a node by its ID (mutable reference).
    pub fn find_node_mut(&mut self, id: &str) -> Option<&mut SkillNode> {
        find_node_mut(&mut self.root, id)
    }

    /// Finds a node by its ID (immutable reference).
    pub fn find_node(&self, id: &str) -> Option<&SkillNode> {
        find_node(&self.root, id)
    }

    /// Returns the path from root to the target node as a readable string.
    /// e.g. `"Polski > Przypadki (Cases) > Biernik (Accusative)"`
    pub fn get_node_path(&self, id: &str) -> Option<String> {
        get_node_path(&self.root, id)
    }

    /// Returns IDs of all leaf nodes that have `node_instructions` set.
    /// These are the nodes a user can generate cards for.
    pub fn leaf_nodes(&self) -> Vec<String> {
        let mut leaves = Vec::new();
        collect_leaves(&self.root, &mut leaves);
        leaves
    }
}

// ----- Node Construction -----

pub fn build_node(config: SkillNodeConfig) -> SkillNode {
    let children = config
        .children
        .into_iter()
        .map(build_node)
        .collect();
    SkillNode {
        id: config.id,
        name: config.name,
        node_instructions: config.node_instructions,
        children,
    }
}

// ----- Public Tree Traversal Helpers -----
// These operate on any &SkillNode root, making them usable on overlay trees.

pub fn find_node_mut<'a>(current: &'a mut SkillNode, target: &str) -> Option<&'a mut SkillNode> {
    if current.id == target {
        return Some(current);
    }
    for child in &mut current.children {
        if let Some(found) = find_node_mut(child, target) {
            return Some(found);
        }
    }
    None
}

pub fn find_node<'a>(current: &'a SkillNode, target: &str) -> Option<&'a SkillNode> {
    if current.id == target {
        return Some(current);
    }
    for child in &current.children {
        if let Some(found) = find_node(child, target) {
            return Some(found);
        }
    }
    None
}

pub fn get_node_path(root: &SkillNode, id: &str) -> Option<String> {
    let mut path = Vec::new();
    if build_path(root, id, &mut path) {
        Some(path.join(" > "))
    } else {
        None
    }
}

fn build_path(node: &SkillNode, target: &str, path: &mut Vec<String>) -> bool {
    path.push(node.name.clone());
    if node.id == target {
        return true;
    }
    for child in &node.children {
        if build_path(child, target, path) {
            return true;
        }
    }
    path.pop();
    false
}

fn collect_leaves(node: &SkillNode, leaves: &mut Vec<String>) {
    if node.children.is_empty() && node.node_instructions.is_some() {
        leaves.push(node.id.clone());
    }
    for child in &node.children {
        collect_leaves(child, leaves);
    }
}

// ----- Tree Customization Overlay -----

/// A lightweight customization descriptor, decoupled from the DB layer.
/// Built from `lc_core::db::UserTreeCustomization` by the API layer.
pub struct TreeCustomization {
    pub node_id: String,
    pub action: String, // "add" | "hide" | "edit"
    pub parent_id: Option<String>,
    pub node_name: Option<String>,
    pub node_instructions: Option<String>,
}

/// Applies user customizations on top of a base tree, producing a new tree.
/// Graceful degradation: invalid customizations (orphaned refs, conflicts) are silently skipped.
pub fn apply_customizations(base_root: &SkillNode, customizations: &[TreeCustomization]) -> SkillNode {
    let mut root = base_root.clone();

    for c in customizations {
        match c.action.as_str() {
            "hide" => {
                remove_node(&mut root, &c.node_id);
            }
            "edit" => {
                if let Some(node) = find_node_mut(&mut root, &c.node_id) {
                    if let Some(ref name) = c.node_name {
                        node.name = name.clone();
                    }
                    if let Some(ref instructions) = c.node_instructions {
                        node.node_instructions = Some(instructions.clone());
                    }
                }
                // else: node not found in base tree, skip (graceful degradation)
            }
            "add" => {
                let Some(ref parent_id) = c.parent_id else { continue };
                // Skip if node_id already exists
                if find_node(&root, &c.node_id).is_some() { continue }
                let Some(parent) = find_node_mut(&mut root, parent_id) else { continue };
                parent.children.push(SkillNode {
                    id: c.node_id.clone(),
                    name: c.node_name.clone().unwrap_or_else(|| c.node_id.clone()),
                    node_instructions: c.node_instructions.clone(),
                    children: vec![],
                });
            }
            _ => {} // unknown action, skip
        }
    }

    root
}

/// Recursively removes a node (and its subtree) from the tree.
fn remove_node(root: &mut SkillNode, target_id: &str) -> bool {
    let len_before = root.children.len();
    root.children.retain(|child| child.id != target_id);
    if root.children.len() < len_before {
        return true;
    }
    for child in &mut root.children {
        if remove_node(child, target_id) {
            return true;
        }
    }
    false
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use langs::Polish;

    fn sample_config() -> SkillNodeConfig {

        SkillNodeConfig {
            id: "root".to_string(),
            name: "Polski".to_string(),
            node_instructions: None,
            children: vec![
                SkillNodeConfig {
                    id: "cases".to_string(),
                    name: "Przypadki".to_string(),
                    node_instructions: None,
                    children: vec![SkillNodeConfig {
                        id: "accusative".to_string(),
                        name: "Biernik".to_string(),
                        node_instructions: Some(
                            "Generate a Polish accusative cloze test.".to_string(),
                        ),
                        children: vec![],
                    }],
                },
                SkillNodeConfig {
                    id: "vocabulary".to_string(),
                    name: "Słownictwo".to_string(),
                    node_instructions: Some(
                        "Generate vocabulary comprehension.".to_string(),
                    ),
                    children: vec![],
                },
            ],
        }
    }

    #[test]
    fn build_tree_from_config() {
        let tree = SkillTree::new(Polish, sample_config());

        assert_eq!(tree.root.name, "Polski");
        assert_eq!(tree.root.children.len(), 2);
    }

    #[test]
    fn find_node_by_id() {
        let tree = SkillTree::new(Polish, sample_config());

        let found = tree.find_node("accusative").unwrap();
        assert_eq!(found.name, "Biernik");
        assert!(found.node_instructions.is_some());
    }

    #[test]
    fn leaf_nodes_returns_nodes_with_instructions() {
        let tree = SkillTree::new(Polish, sample_config());

        let leaves = tree.leaf_nodes();
        assert_eq!(leaves.len(), 2); // accusative + vocabulary
    }

    #[test]
    fn get_node_path_returns_full_path() {
        let tree = SkillTree::new(Polish, sample_config());

        let path = tree.get_node_path("accusative").unwrap();
        assert!(path.contains("Polski"));
        assert!(path.contains("Przypadki"));
        assert!(path.contains("Biernik"));
    }

    #[test]
    fn from_yaml_config() {
        let yaml = r#"
language_name: Polish
root:
  id: root
  name: Polski
  children:
    - id: greetings
      name: Powitania
      node_instructions: Generate a greeting exercise.
      children: []
"#;
        let config: SkillTreeConfig = serde_yaml::from_str(yaml).unwrap();
        let tree = SkillTree::from_config(Polish, config);

        assert_eq!(tree.root.name, "Polski");
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].name, "Powitania");
        assert!(tree.root.children[0].node_instructions.is_some());
    }

    #[test]
    fn apply_customizations_add_node() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![TreeCustomization {
            node_id: "dative".to_string(),
            action: "add".to_string(),
            parent_id: Some("cases".to_string()),
            node_name: Some("Celownik".to_string()),
            node_instructions: Some("Generate dative exercises.".to_string()),
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let cases = find_node(&result, "cases").unwrap();
        assert_eq!(cases.children.len(), 2);
        let dative = find_node(&result, "dative").unwrap();
        assert_eq!(dative.name, "Celownik");
    }

    #[test]
    fn apply_customizations_hide_node() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![TreeCustomization {
            node_id: "vocabulary".to_string(),
            action: "hide".to_string(),
            parent_id: None,
            node_name: None,
            node_instructions: None,
        }];
        let result = apply_customizations(&tree.root, &customizations);
        assert!(find_node(&result, "vocabulary").is_none());
        assert_eq!(result.children.len(), 1);
    }

    #[test]
    fn apply_customizations_edit_node() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![TreeCustomization {
            node_id: "accusative".to_string(),
            action: "edit".to_string(),
            parent_id: None,
            node_name: Some("Biernik (Accusative) - Edited".to_string()),
            node_instructions: None,
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let acc = find_node(&result, "accusative").unwrap();
        assert_eq!(acc.name, "Biernik (Accusative) - Edited");
    }

    #[test]
    fn apply_customizations_graceful_degradation() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![
            // Hide non-existent node — should be skipped
            TreeCustomization {
                node_id: "nonexistent".to_string(),
                action: "hide".to_string(),
                parent_id: None, node_name: None, node_instructions: None,
            },
            // Add under non-existent parent — should be skipped
            TreeCustomization {
                node_id: "orphan".to_string(),
                action: "add".to_string(),
                parent_id: Some("ghost_parent".to_string()),
                node_name: Some("Orphan".to_string()),
                node_instructions: None,
            },
            // Add with duplicate id — should be skipped
            TreeCustomization {
                node_id: "accusative".to_string(),
                action: "add".to_string(),
                parent_id: Some("root".to_string()),
                node_name: Some("Duplicate".to_string()),
                node_instructions: None,
            },
        ];
        let result = apply_customizations(&tree.root, &customizations);
        // Tree should be unchanged
        assert_eq!(result.children.len(), 2);
        assert!(find_node(&result, "orphan").is_none());
    }
}
