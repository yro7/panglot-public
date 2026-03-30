use lc_core::traits::Language;

use crate::llm_backend::LlmBackend;
use crate::post_process::{EarlyPostProcessor, LatePostProcessor};
use crate::prompts::PromptConfig;
use crate::usage::UsageRecorder;
use crate::validation::CardValidator;

use super::core::Pipeline;
use super::types::PipelineConfig;

/// Builder for constructing a `Pipeline`.
pub struct PipelineBuilder<L: Language + Send + Sync> {
    language: L,
    llm_backend: LlmBackend,
    config: PipelineConfig,
    prompt_config: PromptConfig,
    early_processors: Vec<Box<dyn EarlyPostProcessor<L>>>,
    late_processors: Vec<Box<dyn LatePostProcessor<L>>>,
    validators: Vec<Box<dyn CardValidator<L>>>,
    usage_recorder: Option<UsageRecorder>,
}

impl<L: Language + Send + Sync> PipelineBuilder<L> {
    pub fn new(language: L, llm_backend: LlmBackend, config: PipelineConfig, prompt_config: PromptConfig) -> Self {
        Self {
            language,
            llm_backend,
            config,
            prompt_config,
            early_processors: Vec::new(),
            late_processors: Vec::new(),
            validators: Vec::new(),
            usage_recorder: None,
        }
    }

    pub fn early_processor(mut self, processor: Box<dyn EarlyPostProcessor<L>>) -> Self {
        self.early_processors.push(processor);
        self
    }

    pub fn late_processor(mut self, processor: Box<dyn LatePostProcessor<L>>) -> Self {
        self.late_processors.push(processor);
        self
    }

    pub fn validator(mut self, validator: Box<dyn CardValidator<L>>) -> Self {
        self.validators.push(validator);
        self
    }

    pub fn usage_recorder(mut self, recorder: UsageRecorder) -> Self {
        self.usage_recorder = Some(recorder);
        self
    }

    pub fn build(self) -> Pipeline<L> {
        Pipeline {
            language: self.language,
            llm_backend: std::sync::RwLock::new(self.llm_backend),
            early_processors: self.early_processors,
            late_processors: self.late_processors,
            validators: self.validators,
            generator_temperature: self.config.generator_temperature,
            generator_max_tokens: self.config.generator_max_tokens,
            extractor_temperature: self.config.extractor_temperature,
            extractor_max_tokens: self.config.extractor_max_tokens,
            llm_call_timeout: self.config.llm_call_timeout,
            prompt_config: self.prompt_config,
            usage_recorder: self.usage_recorder,
            cached_base_tree: std::sync::OnceLock::new(),
        }
    }
}
