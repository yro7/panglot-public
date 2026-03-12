use serde::Deserialize;

use lc_core::traits::Language;

// ----- Configuration Layer (deserialized from YAML/JSON) -----

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

// ----- Runtime Layer -----

/// A runtime skill node. No longer generic — the Language parameter
/// lives only on `SkillTree`, keeping nodes lightweight and composable.
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
        recursive_find_mut(&mut self.root, id)
    }

    /// Finds a node by its ID (immutable reference).
    pub fn find_node(&self, id: &str) -> Option<&SkillNode> {
        recursive_find(&self.root, id)
    }

    /// Returns the path from root to the target node as a readable string.
    /// e.g. `"Polski > Przypadki (Cases) > Biernik (Accusative)"`
    pub fn get_node_path(&self, id: &str) -> Option<String> {
        let mut path = Vec::new();
        if build_path(&self.root, id, &mut path) {
            Some(path.join(" > "))
        } else {
            None
        }
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

fn build_node(config: SkillNodeConfig) -> SkillNode {
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

// ----- Tree Traversal Helpers -----

fn recursive_find_mut<'a>(current: &'a mut SkillNode, target: &str) -> Option<&'a mut SkillNode> {
    if current.id == target {
        return Some(current);
    }
    for child in &mut current.children {
        if let Some(found) = recursive_find_mut(child, target) {
            return Some(found);
        }
    }
    None
}

fn recursive_find<'a>(current: &'a SkillNode, target: &str) -> Option<&'a SkillNode> {
    if current.id == target {
        return Some(current);
    }
    for child in &current.children {
        if let Some(found) = recursive_find(child, target) {
            return Some(found);
        }
    }
    None
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
}
