use serde::{Deserialize, Serialize};
use lc_core::traits::{CardModel, Language, LinguisticDefinition};
use lc_core::user::UserSettings;
use lc_core::storage::StorageProvider;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::card_models::CardModelId;
use crate::llm_backend::LlmBackend;
use crate::prompts::GeneratorContext;
use crate::skill_tree::SkillNode;

use crate::analyzer::{DynLexiconTracker, LexiconTracker, LibraryAnalyzer};

use super::core::Pipeline;
use super::types::{DynGeneratedCard, DynPromptPreview, to_panini_language_levels};

/// Language-agnostic pipeline interface. The API layer works exclusively through this trait.
/// The tree is injected at each call — the pipeline is stateless w.r.t. the skill tree.
#[async_trait::async_trait]
pub trait DynPipeline: Send + Sync {
    fn language_name(&self) -> &str;
    fn iso_code_str(&self) -> &str;

    /// Returns the base skill tree for this language (cached after first call).
    fn base_tree(&self) -> SkillNode;

    async fn generate_cards_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_backend::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<Vec<DynGeneratedCard>>;

    /// Builds the deck name from the tree path: `Language::TreePath::CardModel`.
    fn build_deck_name_dyn(&self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId) -> Result<String>;

    fn preview_prompt_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        difficulty: u8, user_profile: UserSettings,
        lexicon_options: Option<crate::generator::LexiconOption>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<DynPromptPreview>;

    /// Factory: builds a type-erased lexicon tracker from a storage provider.
    async fn load_lexicon(&self, provider: &(dyn StorageProvider + Sync)) -> Result<Arc<dyn DynLexiconTracker>>;

    /// Hot-swap the LLM backend (e.g. after changing provider/model at runtime).
    fn swap_llm_client(&self, new_backend: LlmBackend);

    fn available_models(&self) -> Vec<CardModelId>;
}

#[async_trait::async_trait]
impl<L> DynPipeline for Pipeline<L>
where
    L: Language + Send + Sync + 'static,
    L::Morphology: std::fmt::Debug
        + Clone
        + PartialEq
        + std::hash::Hash
        + Eq
        + Serialize
        + for<'de> Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    L::GrammaticalFunction: std::fmt::Debug
        + Clone
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + schemars::JsonSchema
        + Send
        + Sync,
    L::ExtraFields: schemars::JsonSchema + Send + Sync,
{
    fn language_name(&self) -> &str {
        self.language.linguistic_def().name()
    }

    fn iso_code_str(&self) -> &str {
        self.language.linguistic_def().iso_code().to_639_3()
    }

    fn base_tree(&self) -> SkillNode {
        self.cached_base_tree.get_or_init(|| {
            let config = self.language.default_tree_config();
            crate::skill_tree::build_node(config.root)
        }).clone()
    }

    async fn generate_cards_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        num_cards: u32, difficulty: u8, user_profile: UserSettings,
        user_prompt: Option<String>,
        lexicon_options: Option<crate::generator::LexiconOption>,
        request_context: Option<crate::llm_backend::RequestContext>,
        llm_sem: Arc<Semaphore>, pp_sem: Arc<Semaphore>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<Vec<DynGeneratedCard>> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, num_cards, difficulty, user_profile, user_prompt, lexicon_options, request_context, concrete);
        let cards = self.generate_cards_batch(tree_root, &req, node_id, llm_sem, pp_sem).await?;
        Ok(cards.into_iter().map(|c| {
            let metadata_json = serde_json::to_string(&c.metadata).unwrap_or_default();
            let explanation = lc_core::sanitize::escape_html(&c.metadata.pedagogical_explanation)
                .replace('\n', "<br>");
            DynGeneratedCard {
                card_id: c.metadata.card_id,
                template_name: c.model.template_name().to_string(),
                front_html: c.model.front_html(),
                back_html: c.model.back_html(),
                fields: c.model.to_fields(),
                explanation,
                skill_name: c.metadata.skill_name,
                ipa: c.metadata.ipa.unwrap_or_default(),
                audio_path: c.metadata.audio_file,
                metadata_json,
            }
        }).collect())
    }

    fn build_deck_name_dyn(&self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId) -> Result<String> {
        let tree_path = crate::skill_tree::get_node_path(tree_root, node_id)
            .ok_or_else(|| anyhow::anyhow!("Node path not found for '{}'", node_id))?;
        let deck_path = tree_path.replace(" > ", "::");
        Ok(format!("{}::{}", deck_path, card_model_id))
    }

    fn preview_prompt_dyn(
        &self, tree_root: &SkillNode, node_id: &str, card_model_id: CardModelId,
        difficulty: u8, user_profile: UserSettings,
        lexicon_options: Option<crate::generator::LexiconOption>,
        lexicon: Option<Arc<dyn DynLexiconTracker>>,
    ) -> Result<DynPromptPreview> {
        let concrete = lexicon.as_ref().and_then(|l| l.as_any().downcast_ref::<LexiconTracker<L::Morphology>>());
        let req = self.build_generation_request(card_model_id, 1, difficulty, user_profile, None, lexicon_options, None, concrete);

        let node = crate::skill_tree::find_node(tree_root, node_id)
            .ok_or_else(|| anyhow::anyhow!("Node '{}' not found", node_id))?;
        let path = crate::skill_tree::get_node_path(tree_root, node_id).unwrap_or_default();

        let system_prompt_call_1 = GeneratorContext::builder()
            .language(&self.language)
            .skill_node(node)
            .node_path(&path)
            .request(&req)
            .prompt_config(&self.prompt_config)
            .build()
            .generate_prompt()?;

        // Build component list (mirrors worker.rs / panini-langs registry composition).
        // `MorphemeSegmentation` self-filters via `is_compatible` at compose time, so
        // including it unconditionally is safe across all languages.
        use panini_core::component::AnalysisComponent;
        use panini_core::components::{
            MorphemeSegmentation, MorphologyAnalysis, MultiwordExpressions, PedagogicalExplanation,
        };
        let pedagogical = PedagogicalExplanation;
        let morphology = MorphologyAnalysis;
        let multiword = MultiwordExpressions;
        let morpheme_seg = MorphemeSegmentation;
        let lang_def = self.language.linguistic_def();
        let all_components: Vec<&dyn AnalysisComponent<L::LinguisticDef>> = vec![
            &pedagogical,
            &morphology,
            &multiword,
            &morpheme_seg,
        ];
        let selected: Vec<&dyn AnalysisComponent<L::LinguisticDef>> = all_components
            .into_iter()
            .filter(|c| c.is_compatible(lang_def))
            .collect();

        let system_prompt_call_2 = {
            let extraction_req = panini_engine::prompts::ExtractionRequest {
                content: String::new(),
                targets: Vec::new(),
                pedagogical_context: node.node_instructions.clone(),
                skill_path: Some(path.clone()),
                learner_ui_language: req.user_profile.ui_language.clone(),
                linguistic_background: to_panini_language_levels(&req.user_profile.linguistic_background),
                user_prompt: req.user_prompt.clone(),
            };
            panini_engine::composer::compose_prompt(
                lang_def,
                &extraction_req,
                &self.prompt_config.extractor,
                &selected,
            )?
        };

        let schema_call_1 = crate::card_models::AnyCard::schema_json_value::<L>(card_model_id);
        let schema_call_2 = panini_engine::composer::compose_schema(lang_def, &selected);

        Ok(DynPromptPreview {
            system_prompt_call_1,
            system_prompt_call_2,
            schema_call_1,
            schema_call_2,
        })
    }

    async fn load_lexicon(&self, provider: &(dyn StorageProvider + Sync)) -> Result<Arc<dyn DynLexiconTracker>> {
        let analyzer = LibraryAnalyzer;
        let lang = self.language.linguistic_def().iso_code().to_639_3();
        let tracker = analyzer.extract_tracker_async::<L::Morphology>(provider, Some(lang)).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(Arc::new(tracker))
    }

    fn swap_llm_client(&self, new_backend: LlmBackend) {
        *self.llm_backend.write().unwrap() = new_backend;
    }

    fn available_models(&self) -> Vec<CardModelId> {
        CardModelId::available_models(&self.language)
    }
}
