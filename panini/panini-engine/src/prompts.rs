use panini_core::traits::LinguisticDefinition;
use serde::Deserialize;
use std::collections::HashMap;
use regex::Regex;
use isolang::Language as IsoLang;

// ----- Prompt Builder Errors -----

#[derive(Debug, thiserror::Error)]
pub enum PromptBuilderError {
    #[error("Failed to parse JSON schema: {0}")]
    SchemaParseError(#[from] serde_json::Error),
    #[error("Failed to load prompt config: {0}")]
    ConfigLoadError(String),
    #[error("Placeholder '{placeholder}' in template is not available in context")]
    PlaceholderNotAvailable { placeholder: String },
}

// ----- Prompt Config Structs -----

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractorPrompts {
    pub system_role: String,
    pub target_language: String,
    pub extraction_directives: String,
    pub learner_profile: LearnerProfile,
    pub skill_context: SkillContextPrompts,
    pub user_context: String,
    pub output_instruction: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LearnerProfile {
    pub ui_language: String,
    pub linguistic_background_intro: String,
    pub linguistic_background_entry: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillContextPrompts {
    pub skill_tree_path: String,
    pub pedagogical_focus: String,
}

impl ExtractorPrompts {
    pub fn load(path: &str) -> Result<Self, PromptBuilderError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PromptBuilderError::ConfigLoadError(format!("Failed to read {}: {}", path, e)))?;
        serde_yml::from_str(&content)
            .map_err(|e| PromptBuilderError::ConfigLoadError(format!("Failed to parse {}: {}", path, e)))
    }
}

// ----- Extraction Request -----

/// A language-level known language + proficiency.
#[derive(Debug, Clone)]
pub struct LanguageLevel {
    pub iso_639_3: String,
    pub level: String,
}

/// Generic extraction request — decoupled from Panglot's GenerationRequest.
pub struct ExtractionRequest {
    /// The text/card JSON to extract features from.
    pub content: String,
    /// Target words to focus extraction on.
    pub targets: Vec<String>,
    /// Optional pedagogical context (replaces skill node instructions).
    pub pedagogical_context: Option<String>,
    /// Optional skill/topic path for context.
    pub skill_path: Option<String>,
    /// Learner's UI language (for pedagogical explanation).
    pub learner_ui_language: String,
    /// Learner's linguistic background.
    pub linguistic_background: Vec<LanguageLevel>,
    /// Optional user-provided context.
    pub user_prompt: Option<String>,
}

// ----- Helper Functions -----

/// Wraps content in XML tags
pub fn wrap_tag(tag: &str, content: &str) -> String {
    format!("<{}>\n{}\n</{}>", tag, content, tag)
}

/// Interpolates placeholders in a template string
pub fn interpolate<V: AsRef<str>>(template: &str, context: &HashMap<&str, V>) -> Result<String, PromptBuilderError> {
    let placeholder_re = Regex::new(r"\{(\w+)\}").unwrap();
    let mut result = template.to_string();

    for cap in placeholder_re.captures_iter(template) {
        let placeholder = &cap[1];
        let value = context.get(placeholder)
            .ok_or_else(|| PromptBuilderError::PlaceholderNotAvailable {
                placeholder: placeholder.to_string(),
            })?
            .as_ref();
        result = result.replace(&format!("{{{}}}", placeholder), value);
    }

    Ok(result)
}

// ----- Feature Extractor Prompt Context -----

/// Builds the system prompt for the feature extractor.
pub fn build_extraction_prompt<L: LinguisticDefinition>(
    language: &L,
    request: &ExtractionRequest,
    extractor_prompts: &ExtractorPrompts,
) -> Result<String, PromptBuilderError> {
    let cfg = extractor_prompts;

    let ui_lang_name = &request.learner_ui_language;
    let ui_lang_iso_code = IsoLang::from_name(ui_lang_name)
        .map(|lang| lang.to_639_3().to_string())
        .unwrap_or_else(|| "eng".to_string());

    let context_description = request.user_prompt.as_deref().unwrap_or("");
    let skill_path = request.skill_path.as_deref().unwrap_or("");
    let instructions = request.pedagogical_context.as_deref().unwrap_or("");

    let mut global_ctx = HashMap::new();
    global_ctx.insert("language", language.name().to_string());
    global_ctx.insert("directives", language.extraction_directives().to_string());
    global_ctx.insert("path", skill_path.to_string());
    global_ctx.insert("instructions", instructions.to_string());
    global_ctx.insert("iso", ui_lang_iso_code.clone());
    global_ctx.insert("name", ui_lang_name.clone());
    global_ctx.insert("context_description", context_description.to_string());

    let mut blocks = Vec::new();

    // System role
    blocks.push(cfg.system_role.clone());

    // Target language section
    let language_context = interpolate(&cfg.target_language, &global_ctx)?;
    blocks.push(wrap_tag("target_language", &language_context));

    // Extraction directives section
    let extraction_directives = interpolate(&cfg.extraction_directives, &global_ctx)?;
    blocks.push(wrap_tag("extraction_directives", &extraction_directives));

    // Learner profile section
    let mut learner_profile_content = String::new();

    let mut ui_lang_ctx = global_ctx.clone();
    ui_lang_ctx.insert("language", ui_lang_name.clone());
    let ui_lang_str = interpolate(&cfg.learner_profile.ui_language, &ui_lang_ctx)?;
    learner_profile_content.push_str(&ui_lang_str);

    if !request.linguistic_background.is_empty() {
        learner_profile_content.push_str("\n\n");
        learner_profile_content.push_str(&cfg.learner_profile.linguistic_background_intro);
        learner_profile_content.push('\n');

        for lang in &request.linguistic_background {
            let mut ctx = global_ctx.clone();
            ctx.insert("iso", lang.iso_639_3.clone());
            ctx.insert("level", lang.level.clone());
            let entry = interpolate(&cfg.learner_profile.linguistic_background_entry, &ctx)?;
            learner_profile_content.push_str(&entry);
            learner_profile_content.push('\n');
        }
    }

    blocks.push(wrap_tag("learner_profile", &learner_profile_content));

    // Skill context section
    let mut skill_context_content = String::new();
    let skill_path_str = interpolate(&cfg.skill_context.skill_tree_path, &global_ctx)?;
    skill_context_content.push_str(&skill_path_str);

    if request.pedagogical_context.is_some() {
        skill_context_content.push('\n');
        let ped_focus_str = interpolate(&cfg.skill_context.pedagogical_focus, &global_ctx)?;
        skill_context_content.push_str(&ped_focus_str);
    }

    blocks.push(wrap_tag("skill_context", &skill_context_content));

    // User context section (if provided)
    if !context_description.is_empty() {
        let user_context_str = interpolate(&cfg.user_context, &global_ctx)?;
        blocks.push(wrap_tag("user_context", &user_context_str));
    }

    // Morpheme segmentation directives (agglutinative languages only)
    if let Some(morph_directives) = language.extra_extraction_directives() {
        blocks.push(wrap_tag("morpheme_segmentation", &morph_directives));
    }

    // Output instruction section
    blocks.push(wrap_tag("output", &cfg.output_instruction));

    Ok(blocks.join("\n\n"))
}
