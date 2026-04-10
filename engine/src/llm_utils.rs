// Re-export from panini-engine — single source of truth.
pub use panini_engine::llm_utils::{clean_llm_json, normalize_pos_tags};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_pos_lowercases() {
        let input = r#"{"pos": "Noun", "lemma": "dom"}"#;
        assert_eq!(normalize_pos_tags(input), r#"{"pos": "noun", "lemma": "dom"}"#);
    }

    #[test]
    fn normalize_pos_maps_ud_abbreviations() {
        let input = r#"{"pos": "ADJ", "lemma": "duży"}"#;
        assert_eq!(normalize_pos_tags(input), r#"{"pos": "adjective", "lemma": "duży"}"#);

        let input2 = r#"{"pos": "prep", "lemma": "na"}"#;
        assert_eq!(normalize_pos_tags(input2), r#"{"pos": "adposition", "lemma": "na"}"#);

        let input3 = r#"{"pos": "ADP", "lemma": "na"}"#;
        assert_eq!(normalize_pos_tags(input3), r#"{"pos": "adposition", "lemma": "na"}"#);
    }

    #[test]
    fn normalize_pos_handles_multiple_occurrences() {
        let input = r#"[{"pos": "PREP"}, {"pos": "Verb"}]"#;
        assert_eq!(normalize_pos_tags(input), r#"[{"pos": "adposition"}, {"pos": "verb"}]"#);
    }

    #[test]
    fn normalize_pos_leaves_valid_values_unchanged() {
        let input = r#"{"pos": "noun", "lemma": "dom"}"#;
        assert_eq!(normalize_pos_tags(input), input);
    }
}
