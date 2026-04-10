/// Strips markdown code fences that LLMs sometimes wrap around JSON responses.
pub fn clean_llm_json(raw: &str) -> &str {
    raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

/// Re-export from panini-core for backwards compatibility.
pub use panini_core::text_processing::normalize_pos_tags;
