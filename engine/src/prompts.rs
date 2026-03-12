use lc_core::traits::{Language, MorphologyInfo};
use crate::generator::GenerationRequest;
use crate::skill_tree::SkillNode;
use serde::{Deserialize};
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
    #[error("Invalid placeholder in template '{template}': '{placeholder}' not found in context")]
    MissingPlaceholder { template: String, placeholder: String },
    #[error("Placeholder '{placeholder}' in template is not available in context")]
    PlaceholderNotAvailable { placeholder: String },
}

// ----- Prompt Config Structs -----

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratorPrompts {
    pub system_role: String,
    pub target_language: String,
    pub language_directives: String,
    pub skill_context: String,
    pub generation_params: GenerationParams,
    pub user_prompt: String,
    pub injected_vocabulary: String,
    pub excluded_vocabulary: String,
    pub output_single: String,
    pub output_multiple: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerationParams {
    pub num_cards: String,
    pub difficulty: String,
    pub ui_language: String,
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct UserMessages {
    pub generator_user: String,
    pub generator_retry_user: String,
    pub extractor_user: String,
    pub extractor_retry_user: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommonPrompts {
    pub no_instructions_fallback: String,
    pub conjugation_instructions: String,
    pub output_json_schema_rules: String,
    pub feature_extraction_separation: String,
}

#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub generator: GeneratorPrompts,
    pub extractor: ExtractorPrompts,
    pub user_messages: UserMessages,
    pub common: CommonPrompts,
}

impl PromptConfig {
    pub fn load(prompts_dir: &str) -> Result<Self, PromptBuilderError> {
        let generator = load_yaml::<GeneratorPrompts>(&format!("{}/generator.yaml", prompts_dir))?;
        let extractor = load_yaml::<ExtractorPrompts>(&format!("{}/extractor.yaml", prompts_dir))?;
        let user_messages = load_yaml::<UserMessages>(&format!("{}/user.yaml", prompts_dir))?;
        let common = load_yaml::<CommonPrompts>(&format!("{}/common.yaml", prompts_dir))?;

        let config = PromptConfig {
            generator,
            extractor,
            user_messages,
            common,
        };

        // Validate all templates
        config.validate_all()?;

        Ok(config)
    }

    fn validate_all(&self) -> Result<(), PromptBuilderError> {
        // Collect all template strings (names and content)
        let templates = vec![
            ("generator.system_role", &self.generator.system_role),
            ("generator.target_language", &self.generator.target_language),
            ("generator.language_directives", &self.generator.language_directives),
            ("generator.skill_context", &self.generator.skill_context),
            ("generator.generation_params.num_cards", &self.generator.generation_params.num_cards),
            ("generator.generation_params.difficulty", &self.generator.generation_params.difficulty),
            ("generator.generation_params.ui_language", &self.generator.generation_params.ui_language),
            ("generator.user_prompt", &self.generator.user_prompt),
            ("generator.injected_vocabulary", &self.generator.injected_vocabulary),
            ("generator.excluded_vocabulary", &self.generator.excluded_vocabulary),
            ("generator.output_single", &self.generator.output_single),
            ("generator.output_multiple", &self.generator.output_multiple),
            ("extractor.system_role", &self.extractor.system_role),
            ("extractor.target_language", &self.extractor.target_language),
            ("extractor.extraction_directives", &self.extractor.extraction_directives),
            ("extractor.learner_profile.ui_language", &self.extractor.learner_profile.ui_language),
            ("extractor.learner_profile.linguistic_background_intro", &self.extractor.learner_profile.linguistic_background_intro),
            ("extractor.learner_profile.linguistic_background_entry", &self.extractor.learner_profile.linguistic_background_entry),
            ("extractor.skill_context.skill_tree_path", &self.extractor.skill_context.skill_tree_path),
            ("extractor.skill_context.pedagogical_focus", &self.extractor.skill_context.pedagogical_focus),
            ("extractor.user_context", &self.extractor.user_context),
            ("extractor.output_instruction", &self.extractor.output_instruction),
            ("user_messages.generator_user", &self.user_messages.generator_user),
            ("user_messages.generator_retry_user", &self.user_messages.generator_retry_user),
            ("user_messages.extractor_user", &self.user_messages.extractor_user),
            ("user_messages.extractor_retry_user", &self.user_messages.extractor_retry_user),
        ];

        let placeholder_re = Regex::new(r"\{(\w+)\}").unwrap();

        for (_name, template) in templates {
            for cap in placeholder_re.captures_iter(template) {
                let placeholder = cap[1].to_string();
                // List of known placeholders that will be filled at runtime
                let known_placeholders = vec![
                    "language", "directives", "instructions", "node_path",
                    "num_cards", "difficulty", "iso", "name",
                    "prompt", "user_prompt", "list", "targets", "card_json", "feedback",
                    "error", "count", "path", "level", "context_description",
                ];

                if !known_placeholders.contains(&placeholder.as_str()) {
                    return Err(PromptBuilderError::PlaceholderNotAvailable {
                        placeholder,
                    });
                }
            }
        }

        Ok(())
    }
}

// Helper to load YAML files
fn load_yaml<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T, PromptBuilderError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| PromptBuilderError::ConfigLoadError(format!("Failed to read {}: {}", path, e)))?;

    serde_yaml::from_str(&content)
        .map_err(|e| PromptBuilderError::ConfigLoadError(format!("Failed to parse {}: {}", path, e)))
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

// ----- Generator Prompt Context -----

#[derive(typed_builder::TypedBuilder)]
pub struct GeneratorContext<'a, L: Language> {
    pub language: &'a L,
    pub skill_node: &'a SkillNode,
    pub node_path: &'a str,
    pub request: &'a GenerationRequest<L>,
    pub prompt_config: &'a PromptConfig,
}

impl<'a, L: Language> GeneratorContext<'a, L> {
    pub fn generate_prompt(self) -> Result<String, PromptBuilderError> {
        let cfg = &self.prompt_config.generator;

        let instructions = self.skill_node.node_instructions.as_deref()
            .unwrap_or(&self.prompt_config.common.no_instructions_fallback);

        // === BUILD GLOBAL PLACEHOLDER CONTEXT ===
        // All placeholders available to all sections
        let num_cards_val = self.request.num_cards.to_string();
        let difficulty_val = self.request.difficulty.to_string();
        let count_val = self.request.num_cards.to_string();

        let ui_lang_name = self.request.user_profile.ui_language.clone();
        let ui_lang_iso_code = IsoLang::from_name(&ui_lang_name)
            .map(|lang| lang.to_639_3().to_string())
            .unwrap_or_else(|| "eng".to_string());

        let directives = self.language.generation_directives().unwrap_or("");

        // Build vocabulary lists (upfront for global access)
        let injected_list = if !self.request.injected_vocabulary.is_empty() {
            self.request.injected_vocabulary
                .iter()
                .map(|e| format!("{} ({})", e.word, e.morphology.pos_label()))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };

        let excluded_list = if !self.request.excluded_vocabulary.is_empty() {
            self.request.excluded_vocabulary
                .iter()
                .map(|e| format!("{} ({})", e.word, e.morphology.pos_label()))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };

        let prompt_str = self.request.user_prompt.as_deref().unwrap_or("");

        // Global context with ALL placeholders available to all sections
        let mut global_ctx = HashMap::new();
        global_ctx.insert("language", self.language.name());
        global_ctx.insert("directives", directives);
        global_ctx.insert("node_path", self.node_path);
        global_ctx.insert("instructions", instructions);
        global_ctx.insert("num_cards", num_cards_val.as_str());
        global_ctx.insert("difficulty", difficulty_val.as_str());
        global_ctx.insert("iso", ui_lang_iso_code.as_str());
        global_ctx.insert("name", &ui_lang_name);
        global_ctx.insert("prompt", prompt_str);
        global_ctx.insert("count", count_val.as_str());

        let mut blocks = Vec::new();

        // System role - interpolate with global context
        let system_role = interpolate(&cfg.system_role, &global_ctx)?;
        blocks.push(system_role);

        // Target language section
        let language_context = interpolate(&cfg.target_language, &global_ctx)?;
        blocks.push(wrap_tag("target_language", &language_context));

        // Language directives section
        if !directives.is_empty() {
            let language_directives = interpolate(&cfg.language_directives, &global_ctx)?;
            blocks.push(wrap_tag("language_directives", &language_directives));
        }

        // Skill context section
        let skill_context = interpolate(&cfg.skill_context, &global_ctx)?;
        blocks.push(wrap_tag("skill_context", &skill_context));

        // Generation parameters section
        let mut gen_params = String::new();

        let num_cards_str = interpolate(&cfg.generation_params.num_cards, &global_ctx)?;
        gen_params.push_str(&num_cards_str);
        gen_params.push('\n');

        let difficulty_str = interpolate(&cfg.generation_params.difficulty, &global_ctx)?;
        gen_params.push_str(&difficulty_str);
        gen_params.push('\n');

        let ui_lang_str = interpolate(&cfg.generation_params.ui_language, &global_ctx)?;
        gen_params.push_str(&ui_lang_str);

        if !prompt_str.is_empty() {
            gen_params.push('\n');
            let user_prompt_str = interpolate(&cfg.user_prompt, &global_ctx)?;
            gen_params.push_str(&user_prompt_str);
        }

        blocks.push(wrap_tag("generation_params", &gen_params));

        // Injected vocabulary section
        if !self.request.injected_vocabulary.is_empty() {
            let mut ctx = global_ctx.clone();
            ctx.insert("list", injected_list.as_str());
            let injected = interpolate(&cfg.injected_vocabulary, &ctx)?;
            blocks.push(wrap_tag("injected_vocabulary", &injected));
        }

        // Excluded vocabulary section
        if !self.request.excluded_vocabulary.is_empty() {
            let mut ctx = global_ctx.clone();
            ctx.insert("list", excluded_list.as_str());
            let excluded = interpolate(&cfg.excluded_vocabulary, &ctx)?;
            blocks.push(wrap_tag("excluded_vocabulary", &excluded));
        }

        // Conjugation instructions (if applicable)
        if self.request.card_model_id == crate::card_models::CardModelId::Conjugation {
            blocks.push(wrap_tag("conjugation_instructions", &self.prompt_config.common.conjugation_instructions));
        }

        // Output instruction section
        let output_instr = if self.request.num_cards == 1 {
            self.prompt_config.generator.output_single.clone()
        } else {
            interpolate(&self.prompt_config.generator.output_multiple, &global_ctx)?
        };
        blocks.push(wrap_tag("output", &output_instr));

        Ok(blocks.join("\n\n"))
    }
}

// ----- Feature Extractor Prompt Context -----

#[derive(typed_builder::TypedBuilder)]
pub struct FeatureExtractorContext<'a, L: Language> {
    pub language: &'a L,
    pub skill_node: &'a SkillNode,
    pub node_path: &'a str,
    pub request: &'a GenerationRequest<L>,
    pub prompt_config: &'a PromptConfig,
}

impl<'a, L: Language> FeatureExtractorContext<'a, L> {
    pub fn generate_prompt(self) -> Result<String, PromptBuilderError> {
        let cfg = &self.prompt_config.extractor;

        // === BUILD GLOBAL PLACEHOLDER CONTEXT ===
        let ui_lang_name = self.request.user_profile.ui_language.clone();
        let ui_lang_iso_code = IsoLang::from_name(&ui_lang_name)
            .map(|lang| lang.to_639_3().to_string())
            .unwrap_or_else(|| "eng".to_string());

        let context_description = self.request.user_prompt.as_deref().unwrap_or("");

        // Global context with ALL placeholders available to all sections
        let mut global_ctx = HashMap::new();
        global_ctx.insert("language", self.language.name());
        global_ctx.insert("directives", self.language.extraction_directives());
        global_ctx.insert("path", self.node_path);
        global_ctx.insert("instructions", self.skill_node.node_instructions.as_deref().unwrap_or(""));
        global_ctx.insert("iso", ui_lang_iso_code.as_str());
        global_ctx.insert("name", &ui_lang_name);
        global_ctx.insert("context_description", context_description);

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

        // For learner_profile.ui_language, {language} refers to UI language, not target language
        let mut ui_lang_ctx = global_ctx.clone();
        ui_lang_ctx.insert("language", &ui_lang_name);
        let ui_lang_str = interpolate(&cfg.learner_profile.ui_language, &ui_lang_ctx)?;
        learner_profile_content.push_str(&ui_lang_str);

        if !self.request.user_profile.linguistic_background.is_empty() {
            learner_profile_content.push_str("\n\n");
            learner_profile_content.push_str(&cfg.learner_profile.linguistic_background_intro);
            learner_profile_content.push('\n');

            for lang in &self.request.user_profile.linguistic_background {
                let level_str = format!("{:?}", lang.level);
                let mut ctx = global_ctx.clone();
                ctx.insert("iso", lang.iso_639_3.as_str());
                ctx.insert("level", level_str.as_str());
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

        if let Some(_) = &self.skill_node.node_instructions {
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

        // Output instruction section
        blocks.push(wrap_tag("output", &cfg.output_instruction));

        Ok(blocks.join("\n\n"))
    }
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::domain::ExtractedFeature;
    use langs::Polish;
    use crate::card_models::CardModelId;
    use crate::skill_tree::{SkillNodeConfig, SkillTree};
    use lc_core::user::UserProfile;

    fn sample_tree() -> SkillTree<Polish> {
        let config = SkillNodeConfig {
            id: "root".to_string(),
            name: "Polski".to_string(),
            node_instructions: None,
            children: vec![SkillNodeConfig {
                id: "accusative".to_string(),
                name: "Biernik".to_string(),
                node_instructions: Some("Focus on the accusative case.".to_string()),
                children: vec![],
            }],
        };
        SkillTree::new(Polish, config)
    }

    fn load_test_config() -> PromptConfig {
        let prompts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("prompts");
        PromptConfig::load(prompts_path.to_str().unwrap())
            .expect("Failed to load prompts config")
    }

    #[test]
    fn test_load_prompt_config() {
        let config = load_test_config();
        assert!(!config.generator.system_role.is_empty());
        assert!(!config.extractor.system_role.is_empty());
        assert!(!config.user_messages.generator_user.is_empty());
        assert!(!config.common.no_instructions_fallback.is_empty());
    }

    #[test]
    fn test_wrap_tag() {
        let result = wrap_tag("section", "content");
        assert_eq!(result, "<section>\ncontent\n</section>");
    }

    #[test]
    fn test_interpolate_single_placeholder() {
        let mut ctx = HashMap::new();
        ctx.insert("name", "Alice");

        let result = interpolate("Hello {name}", &ctx).unwrap();
        assert_eq!(result, "Hello Alice");
    }

    #[test]
    fn test_interpolate_multiple_placeholders() {
        let mut ctx = HashMap::new();
        ctx.insert("greeting", "Hi");
        ctx.insert("name", "Bob");

        let result = interpolate("{greeting} {name}!", &ctx).unwrap();
        assert_eq!(result, "Hi Bob!");
    }

    #[test]
    fn test_interpolate_missing_placeholder_fails() {
        let ctx: HashMap<&str, &str> = HashMap::new();
        let result = interpolate("Hello {name}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_prompt_config_validation() {
        let prompts_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("prompts");
        let result = PromptConfig::load(prompts_path.to_str().unwrap());
        assert!(result.is_ok(), "Prompt config should load and validate successfully");
    }

    #[test]
    fn test_generator_prompt_builder() {
        let tree = sample_tree();
        let acc_id = "accusative";
        let config = load_test_config();

        let req = GenerationRequest::<Polish> {
            card_model_id: CardModelId::ClozeTest,
            num_cards: 3,
            difficulty: 5,
            user_profile: UserProfile::new("English".to_string()),
            user_prompt: Some("Use funny sentences.".to_string()),
            transliteration: None,
            injected_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
            excluded_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
        };

        let node = tree.find_node(acc_id).unwrap();
        let path = tree.get_node_path(acc_id).unwrap();

        let prompt = GeneratorContext::builder()
            .language(&tree.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&config)
            .build()
            .generate_prompt()
            .unwrap();

        assert!(prompt.contains("language learning exercises generator"));
        assert!(prompt.contains("Polish"));
        assert!(prompt.contains("Biernik"));
        assert!(prompt.contains("Focus on the accusative case."));
        assert!(prompt.contains("Number of distinct cards to generate: 3"));
        assert!(prompt.contains("English"));
        assert!(prompt.contains("eng"), "ISO 639-3 code should be dynamically resolved from 'English'");
        assert!(prompt.contains("Use funny sentences."));
        assert!(prompt.contains("EXACTLY 3 distinct objects"));
        assert!(prompt.contains("<target_language>"));
        assert!(prompt.contains("</target_language>"));
        assert!(prompt.contains("<skill_context>"));
        assert!(prompt.contains("</skill_context>"));
    }

    #[test]
    fn test_feature_extractor_prompt_builder() {
        let tree = sample_tree();
        let acc_id = "accusative";
        let config = load_test_config();

        let req = GenerationRequest::<Polish> {
            card_model_id: CardModelId::ClozeTest,
            num_cards: 1,
            difficulty: 5,
            user_profile: UserProfile::new("French".to_string()),
            user_prompt: None,
            transliteration: None,
            injected_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
            excluded_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
        };

        let node = tree.find_node(acc_id).unwrap();
        let path = tree.get_node_path(acc_id).unwrap();

        let prompt = FeatureExtractorContext::builder()
            .language(&tree.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&config)
            .build()
            .generate_prompt()
            .unwrap();

        assert!(prompt.contains("expert computational linguist"));
        assert!(prompt.contains("Polish"));
        assert!(prompt.contains("<extraction_directives>"));
        assert!(prompt.contains("</extraction_directives>"));
        assert!(prompt.contains("French"));
        assert!(prompt.contains("fra"), "ISO 639-3 code should be dynamically resolved from 'French'");
        assert!(prompt.contains("<learner_profile>"));
        assert!(prompt.contains("</learner_profile>"));
        assert!(prompt.contains("<skill_context>"));
        assert!(prompt.contains("</skill_context>"));
    }

    #[test]
    fn test_generator_single_card_output() {
        let tree = sample_tree();
        let acc_id = "accusative";
        let config = load_test_config();

        let req = GenerationRequest::<Polish> {
            card_model_id: CardModelId::ClozeTest,
            num_cards: 1,
            difficulty: 5,
            user_profile: UserProfile::new("English".to_string()),
            user_prompt: None,
            transliteration: None,
            injected_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
            excluded_vocabulary: Vec::<ExtractedFeature<langs::PolishMorphology>>::new(),
        };

        let node = tree.find_node(acc_id).unwrap();
        let path = tree.get_node_path(acc_id).unwrap();

        let prompt = GeneratorContext::builder()
            .language(&tree.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&config)
            .build()
            .generate_prompt()
            .unwrap();

        assert!(prompt.contains("EXACTLY ONE JSON object"));
        assert!(!prompt.contains("distinct objects"));
    }

    #[test]
    fn dump_feature_extractor_schema() {
        let schema_value = serde_json::to_value(
            &schemars::schema_for!(crate::feature_extractor::FeatureExtractionResponse<langs::PolishMorphology>)
        ).unwrap();
        let schema = serde_json::to_string_pretty(&schema_value).unwrap();
        eprintln!("FEATURE EXTRACTOR SCHEMA\n{}", schema);
    }
}
