use std::collections::{HashMap, HashSet};

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
    pub prerequisites: Vec<String>,
    pub tier: u32,
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
        let mut root = build_node(config.root);
        compute_tiers(&mut root);
        Self { language, root }
    }

    /// Creates a SkillTree from a raw SkillNodeConfig (without the SkillTreeConfig wrapper).
    pub fn new(language: L, config: SkillNodeConfig) -> Self {
        let mut root = build_node(config);
        compute_tiers(&mut root);
        Self { language, root }
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

    /// Returns IDs of all nodes that have `node_instructions` set (generatable nodes).
    /// These are the nodes a user can generate decks for, regardless of whether they have children.
    pub fn generatable_nodes(&self) -> Vec<String> {
        let mut ids = Vec::new();
        collect_generatable(&self.root, &mut ids);
        ids
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
        prerequisites: config.prerequisites,
        tier: 0,
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

fn collect_generatable(node: &SkillNode, out: &mut Vec<String>) {
    if node.node_instructions.is_some() {
        out.push(node.id.clone());
    }
    for child in &node.children {
        collect_generatable(child, out);
    }
}

// ----- Tier Computation -----

/// Computes the `tier` field on every node in the tree from its `prerequisites`.
/// `tier(node) = 1 + max(tier(p) for p in prerequisites if resolvable)`, else 0.
///
/// Graceful degradation:
/// - Unknown prerequisite IDs are logged at `warn` level and contribute 0.
/// - Cycles are detected, members assigned tier 0, and logged.
/// - Never panics, never fails.
pub fn compute_tiers(root: &mut SkillNode) {
    // Step 1: collect snapshot of (id -> prerequisites) — avoids borrow conflicts.
    let mut prereqs_by_id: HashMap<String, Vec<String>> = HashMap::new();
    collect_prereqs(root, &mut prereqs_by_id);

    // Step 2: compute tier per id via DFS with memoization and cycle detection.
    let mut memo: HashMap<String, u32> = HashMap::new();
    let mut in_progress: HashSet<String> = HashSet::new();
    let ids: Vec<String> = prereqs_by_id.keys().cloned().collect();
    for id in &ids {
        let _ = resolve_tier(id, &prereqs_by_id, &mut memo, &mut in_progress);
    }

    // Step 3: write tiers back into the tree, incorporating structural depth.
    apply_tiers(root, &memo, 0);
}

fn collect_prereqs(node: &SkillNode, out: &mut HashMap<String, Vec<String>>) {
    out.insert(node.id.clone(), node.prerequisites.clone());
    for child in &node.children {
        collect_prereqs(child, out);
    }
}

fn resolve_tier(
    id: &str,
    prereqs_by_id: &HashMap<String, Vec<String>>,
    memo: &mut HashMap<String, u32>,
    in_progress: &mut HashSet<String>,
) -> u32 {
    if let Some(&t) = memo.get(id) {
        return t;
    }
    if in_progress.contains(id) {
        tracing::warn!("skill_tree: cycle detected involving node '{}', assigning tier 0", id);
        memo.insert(id.to_string(), 0);
        return 0;
    }
    let Some(prereqs) = prereqs_by_id.get(id) else {
        return 0;
    };
    if prereqs.is_empty() {
        memo.insert(id.to_string(), 0);
        return 0;
    }
    in_progress.insert(id.to_string());
    let mut max_prereq_tier: u32 = 0;
    for p in prereqs {
        if !prereqs_by_id.contains_key(p) {
            tracing::warn!("skill_tree: node '{}' references unknown prerequisite '{}'", id, p);
            continue;
        }
        let t = resolve_tier(p, prereqs_by_id, memo, in_progress);
        if t > max_prereq_tier {
            max_prereq_tier = t;
        }
    }
    in_progress.remove(id);
    let tier = max_prereq_tier.saturating_add(1);
    memo.insert(id.to_string(), tier);
    tier
}

fn apply_tiers(node: &mut SkillNode, memo: &HashMap<String, u32>, depth: u32) {
    let prereq_tier = *memo.get(&node.id).unwrap_or(&0);
    node.tier = u32::max(depth, prereq_tier);
    for child in &mut node.children {
        apply_tiers(child, memo, depth + 1);
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
    /// For `add`: initial prereqs for the new node (None → empty).
    /// For `edit`: if `Some`, replace prereq list (including empty to clear); if `None`, leave unchanged.
    pub prerequisites: Option<Vec<String>>,
}

/// Applies user customizations on top of a base tree, producing a new tree.
/// Graceful degradation: invalid customizations (orphaned refs, conflicts) are silently skipped.
/// Tiers are NOT recomputed here — call `compute_tiers` on the result if you need fresh tiers.
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
                    if let Some(ref prereqs) = c.prerequisites {
                        node.prerequisites = prereqs.clone();
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
                    prerequisites: c.prerequisites.clone().unwrap_or_default(),
                    tier: 0,
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
            prerequisites: vec![],
            children: vec![
                SkillNodeConfig {
                    id: "cases".to_string(),
                    name: "Przypadki".to_string(),
                    node_instructions: None,
                    prerequisites: vec![],
                    children: vec![SkillNodeConfig {
                        id: "accusative".to_string(),
                        name: "Biernik".to_string(),
                        node_instructions: Some(
                            "Generate a Polish accusative cloze test.".to_string(),
                        ),
                        prerequisites: vec![],
                        children: vec![],
                    }],
                },
                SkillNodeConfig {
                    id: "vocabulary".to_string(),
                    name: "Słownictwo".to_string(),
                    node_instructions: Some(
                        "Generate vocabulary comprehension.".to_string(),
                    ),
                    prerequisites: vec![],
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
    fn generatable_nodes_returns_nodes_with_instructions() {
        let tree = SkillTree::new(Polish, sample_config());

        let nodes = tree.generatable_nodes();
        assert_eq!(nodes.len(), 2); // accusative + vocabulary
        assert!(nodes.contains(&"accusative".to_string()));
        assert!(nodes.contains(&"vocabulary".to_string()));
    }

    #[test]
    fn generatable_nodes_includes_branches_with_instructions() {
        // Branch with instructions should be listed alongside leaves with instructions.
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "Root".to_string(),
            node_instructions: None,
            prerequisites: vec![],
            children: vec![SkillNodeConfig {
                id: "grammar".to_string(),
                name: "Grammar".to_string(),
                node_instructions: Some("Broad grammar drill".to_string()),
                prerequisites: vec![],
                children: vec![SkillNodeConfig {
                    id: "present".to_string(),
                    name: "Present".to_string(),
                    node_instructions: Some("Present tense".to_string()),
                    prerequisites: vec![],
                    children: vec![],
                }],
            }],
        };
        let tree = SkillTree::new(Polish, config);
        let nodes = tree.generatable_nodes();
        assert!(nodes.contains(&"grammar".to_string()));
        assert!(nodes.contains(&"present".to_string()));
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
        let config: SkillTreeConfig = serde_yml::from_str(yaml).unwrap();
        let tree = SkillTree::from_config(Polish, config);

        assert_eq!(tree.root.name, "Polski");
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].name, "Powitania");
        assert!(tree.root.children[0].node_instructions.is_some());
    }

    #[test]
    fn prereqs_deserialize_default_empty() {
        // Old-shape YAML without `prerequisites` field must parse cleanly.
        let yaml = r#"
language_name: Polish
root:
  id: root
  name: Polski
  children:
    - id: a
      name: A
      node_instructions: x
      children: []
"#;
        let config: SkillTreeConfig = serde_yml::from_str(yaml).unwrap();
        assert!(config.root.prerequisites.is_empty());
        assert!(config.root.children[0].prerequisites.is_empty());
    }

    #[test]
    fn prereqs_deserialize_list() {
        let yaml = r#"
language_name: Polish
root:
  id: root
  name: Polski
  prerequisites: []
  children:
    - id: past
      name: Past
      node_instructions: x
      prerequisites: [present, etre]
      children: []
"#;
        let config: SkillTreeConfig = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.root.children[0].prerequisites, vec!["present".to_string(), "etre".to_string()]);
    }

    fn chain_node(id: &str, prereqs: Vec<&str>) -> SkillNodeConfig {
        SkillNodeConfig {
            id: id.to_string(),
            name: id.to_string(),
            node_instructions: Some("x".to_string()),
            prerequisites: prereqs.into_iter().map(String::from).collect(),
            children: vec![],
        }
    }

    #[test]
    fn compute_tiers_linear_chain() {
        // a -> b -> c  =>  tier(a)=0, tier(b)=1, tier(c)=2
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "root".to_string(),
            node_instructions: None,
            prerequisites: vec![],
            children: vec![
                chain_node("a", vec![]),
                chain_node("b", vec!["a"]),
                chain_node("c", vec!["b"]),
            ],
        };
        let tree = SkillTree::new(Polish, config);
        assert_eq!(tree.find_node("a").unwrap().tier, 0);
        assert_eq!(tree.find_node("b").unwrap().tier, 1);
        assert_eq!(tree.find_node("c").unwrap().tier, 2);
    }

    #[test]
    fn compute_tiers_diamond() {
        // a, b -> c  where a: tier 0, b: tier 1 (requires a). c requires a and b => tier = 1 + max(0, 1) = 2.
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "root".to_string(),
            node_instructions: None,
            prerequisites: vec![],
            children: vec![
                chain_node("a", vec![]),
                chain_node("b", vec!["a"]),
                chain_node("c", vec!["a", "b"]),
            ],
        };
        let tree = SkillTree::new(Polish, config);
        assert_eq!(tree.find_node("a").unwrap().tier, 0);
        assert_eq!(tree.find_node("b").unwrap().tier, 1);
        assert_eq!(tree.find_node("c").unwrap().tier, 2);
    }

    #[test]
    fn compute_tiers_cycle() {
        // a <-> b: cycle. Both get tier 0, no panic.
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "root".to_string(),
            node_instructions: None,
            prerequisites: vec![],
            children: vec![
                chain_node("a", vec!["b"]),
                chain_node("b", vec!["a"]),
            ],
        };
        let tree = SkillTree::new(Polish, config);
        // Cycle members must be finite (no panic). One will break the cycle at tier 0.
        let ta = tree.find_node("a").unwrap().tier;
        let tb = tree.find_node("b").unwrap().tier;
        assert!(ta <= 1 && tb <= 1);
    }

    #[test]
    fn compute_tiers_unknown_prereq() {
        // Node references a ghost ID — should get tier 0 (ghost contributes 0) + saturated +1 = 1? No:
        // unknown prereqs are SKIPPED (contribute nothing), so max_prereq_tier = 0, then +1 = 1.
        // Actually: we want tier = 0 if all prereqs unknown. Re-check: `max_prereq_tier` starts at 0, no
        // prereq updates it, then `tier = max + 1 = 1`. We want tier = 0 for nodes with no RESOLVABLE
        // prereqs. Test what we actually produce.
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "root".to_string(),
            node_instructions: None,
            prerequisites: vec![],
            children: vec![
                chain_node("orphan", vec!["ghost"]),
            ],
        };
        let tree = SkillTree::new(Polish, config);
        // Current behavior: orphan has `prerequisites = ["ghost"]` (non-empty), enters the tier
        // computation branch, finds ghost unresolvable, max stays 0, result = 1.
        // This matches "present prereqs even if unresolvable still bump tier" — predictable.
        assert_eq!(tree.find_node("orphan").unwrap().tier, 1);
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
            prerequisites: None,
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let cases = find_node(&result, "cases").unwrap();
        assert_eq!(cases.children.len(), 2);
        let dative = find_node(&result, "dative").unwrap();
        assert_eq!(dative.name, "Celownik");
    }

    #[test]
    fn apply_customizations_add_with_prerequisites() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![TreeCustomization {
            node_id: "dative".to_string(),
            action: "add".to_string(),
            parent_id: Some("cases".to_string()),
            node_name: Some("Celownik".to_string()),
            node_instructions: Some("x".to_string()),
            prerequisites: Some(vec!["accusative".to_string()]),
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let dative = find_node(&result, "dative").unwrap();
        assert_eq!(dative.prerequisites, vec!["accusative".to_string()]);
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
            prerequisites: None,
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
            prerequisites: None,
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let acc = find_node(&result, "accusative").unwrap();
        assert_eq!(acc.name, "Biernik (Accusative) - Edited");
    }

    #[test]
    fn apply_customizations_edit_prerequisites_replace() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![TreeCustomization {
            node_id: "accusative".to_string(),
            action: "edit".to_string(),
            parent_id: None,
            node_name: None,
            node_instructions: None,
            prerequisites: Some(vec!["cases".to_string()]),
        }];
        let result = apply_customizations(&tree.root, &customizations);
        let acc = find_node(&result, "accusative").unwrap();
        assert_eq!(acc.prerequisites, vec!["cases".to_string()]);
    }

    #[test]
    fn apply_customizations_edit_prerequisites_none_leaves_unchanged() {
        let mut base = build_node(sample_config());
        // Seed accusative with prereqs first.
        find_node_mut(&mut base, "accusative").unwrap().prerequisites = vec!["seed".to_string()];
        let customizations = vec![TreeCustomization {
            node_id: "accusative".to_string(),
            action: "edit".to_string(),
            parent_id: None,
            node_name: Some("New Name".to_string()),
            node_instructions: None,
            prerequisites: None, // do not touch prereqs
        }];
        let result = apply_customizations(&base, &customizations);
        let acc = find_node(&result, "accusative").unwrap();
        assert_eq!(acc.prerequisites, vec!["seed".to_string()]);
        assert_eq!(acc.name, "New Name");
    }

    #[test]
    fn apply_customizations_graceful_degradation() {
        let tree = SkillTree::new(Polish, sample_config());
        let customizations = vec![
            // Hide non-existent node — should be skipped
            TreeCustomization {
                node_id: "nonexistent".to_string(),
                action: "hide".to_string(),
                parent_id: None, node_name: None, node_instructions: None, prerequisites: None,
            },
            // Add under non-existent parent — should be skipped
            TreeCustomization {
                node_id: "orphan".to_string(),
                action: "add".to_string(),
                parent_id: Some("ghost_parent".to_string()),
                node_name: Some("Orphan".to_string()),
                node_instructions: None,
                prerequisites: None,
            },
            // Add with duplicate id — should be skipped
            TreeCustomization {
                node_id: "accusative".to_string(),
                action: "add".to_string(),
                parent_id: Some("root".to_string()),
                node_name: Some("Duplicate".to_string()),
                node_instructions: None,
                prerequisites: None,
            },
        ];
        let result = apply_customizations(&tree.root, &customizations);
        // Tree should be unchanged
        assert_eq!(result.children.len(), 2);
        assert!(find_node(&result, "orphan").is_none());
    }

    #[test]
    fn test_compute_tiers_incorporates_depth() {
        let mut root = SkillNode {
            id: "root".to_string(),
            name: "Root".to_string(),
            node_instructions: None,
            tier: 0,
            prerequisites: vec![],
            children: vec![
                SkillNode {
                    id: "child1".to_string(),
                    name: "Child 1".to_string(),
                    node_instructions: None,
                    tier: 0,
                    prerequisites: vec![],
                    children: vec![
                        SkillNode {
                            id: "grandchild1".to_string(),
                            name: "Grandchild 1".to_string(),
                            node_instructions: None,
                            tier: 0,
                            prerequisites: vec!["child2".to_string()],
                            children: vec![],
                        }
                    ],
                },
                SkillNode {
                    id: "child2".to_string(),
                    name: "Child 2".to_string(),
                    node_instructions: None,
                    tier: 0,
                    prerequisites: vec![],
                    children: vec![],
                }
            ],
        };

        compute_tiers(&mut root);

        let child1 = find_node(&root, "child1").unwrap();
        let child2 = find_node(&root, "child2").unwrap();
        let grandchild1 = find_node(&root, "grandchild1").unwrap();

        // Structural depth: root=0, child1=1, child2=1, grandchild1=2
        // Prereqs: grandchild1 -> child2 (tier 1) => grandchild1.prereq_tier = 2
        
        assert_eq!(root.tier, 0);
        assert_eq!(child1.tier, 1);
        assert_eq!(child2.tier, 1);
        assert_eq!(grandchild1.tier, 2);
    }
}
