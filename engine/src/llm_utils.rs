/// Strips markdown code fences that LLMs sometimes wrap around JSON responses.
pub fn clean_llm_json(raw: &str) -> &str {
    raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

/// Normalizes `"pos"` field values in a JSON string before deserialization:
/// 1. Lowercases all `"pos": "..."` values
/// 2. Maps common UD abbreviations to canonical enum names (e.g. `adj` → `adjective`)
///
/// This is a defensive pre-processing step so the morphology enum doesn't need
/// dozens of `#[serde(alias)]` annotations.
pub fn normalize_pos_tags(json: &str) -> String {
    // UD abbreviation → canonical lowercase enum variant
    let ud_map: &[(&str, &str)] = &[
        ("adj",   "adjective"),
        ("adp",   "adposition"),
        ("adv",   "adverb"),
        ("aux",   "auxiliary"),
        ("cconj", "coordinating_conjunction"),
        ("det",   "determiner"),
        ("intj",  "interjection"),
        ("n",     "noun"),
        ("num",   "numeral"),
        ("part",  "particle"),
        ("prep",  "adposition"),
        ("pron",  "pronoun"),
        ("propn", "proper_noun"),
        ("sconj", "subordinating_conjunction"),
        ("v",     "verb"),
        ("conj",  "coordinating_conjunction"),
        ("interj","interjection"),
        ("sym",   "symbol"),
        ("punc",   "punctuation"),
        ("punct",   "punctuation"),
        ("x",     "other"),
    ];

    // Regex: match `"pos"` (with optional whitespace) `:` string value
    let re = regex::Regex::new(r#""pos"\s*:\s*"([^"]+)""#).unwrap();

    re.replace_all(json, |caps: &regex::Captures| {
        let raw_val = caps[1].to_lowercase();
        let normalized = ud_map.iter()
            .find(|(abbr, _)| *abbr == raw_val)
            .map(|(_, canonical)| *canonical)
            .unwrap_or_else(|| {
                // Not in the map — use the lowercased value as-is
                // (leak is fine here, it's a small bounded set of POS tags)
                return &raw_val;
            });
        format!(r#""pos": "{}""#, normalized)
    }).to_string()
}

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
