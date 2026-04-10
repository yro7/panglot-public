use serde::{Deserialize, Serialize};
use lc_core::domain::CardMetadata;
use lc_core::traits::{Language, LinguisticDefinition};

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::generator::GenerationRequest;
use crate::llm_backend::LlmBackend;
use crate::skill_tree::SkillNode;
use crate::usage::{PostProcessEvent, PostProcessType};

use super::core::Pipeline;
use super::types::{GeneratedCard, to_panini_language_levels};

impl<L: Language + Send + Sync> Pipeline<L>
where
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
{
    /// Processes a single parsed card: runs feature extraction and post-processors in parallel,
    /// then assembles the final `GeneratedCard` with its metadata.
    pub(super) async fn process_single_card(
        &self,
        any_card: crate::card_models::AnyCard,
        extra_fields: L::ExtraFields,
        card_json_for_extraction: String,
        skill_node_id: &str,
        skill_name: String,
        extraction_node: Option<SkillNode>,
        extraction_path: String,
        llm_sem: Arc<Semaphore>,
        pp_sem: Arc<Semaphore>,
        req: &GenerationRequest<L>,
    ) -> GeneratedCard<L::Morphology, L::GrammaticalFunction> {
        let card_id_str = uuid::Uuid::new_v4().to_string();
        let extra_value = serde_json::to_value(&extra_fields).unwrap_or(serde_json::Value::Null);

        // --- Parallel: Feature Extraction + Early Post-Processing ---
        let (extracted, early_results) = tokio::join!(
            self.run_feature_extraction(
                &any_card, &card_json_for_extraction,
                extraction_node.as_ref(), &extraction_path,
                &llm_sem, req,
            ),
            self.run_early_post_processors(
                &any_card, &card_id_str, &extra_value,
                &pp_sem, req,
            )
        );

        // Assemble metadata from feature extraction results
        let (pedagogical_explanation, target_features, context_features, multiword_expressions, morpheme_segmentation) = match extracted {
            Ok(resp) => (resp.pedagogical_explanation, resp.target_features, resp.context_features, resp.multiword_expressions, resp.morpheme_segmentation),
            Err(e) => {
                tracing::error!(%e, "Feature extraction FAILED (even after retry) — explanation will be empty");
                (String::new(), Vec::new(), Vec::new(), Vec::new(), None)
            },
        };

        let mut metadata = CardMetadata {
            card_id: card_id_str,
            language: self.language.linguistic_def().iso_code().to_639_3().to_string(),
            skill_id: skill_node_id.to_string(),
            skill_name,
            pedagogical_explanation,
            target_features,
            context_features,
            multiword_expressions,
            ipa: None,
            audio_file: None,
            morpheme_segmentation,
        };

        // Merge early post-processing results
        for r in early_results {
            if r.ipa.is_some() { metadata.ipa = r.ipa; }
            if r.audio_file.is_some() { metadata.audio_file = r.audio_file; }
        }

        // --- Late Post-Processing (sequential, after feature extraction) ---
        for processor in &self.late_processors {
            if let Err(e) = processor.process(&self.language, &any_card, &extra_value, &mut metadata).await {
                tracing::error!(%e, "Late post-processing error");
            }
        }

        GeneratedCard { model: any_card, metadata }
    }

    /// Runs feature extraction via Panini (LLM call 2) with exponential backoff retries.
    pub(super) async fn run_feature_extraction(
        &self,
        any_card: &crate::card_models::AnyCard,
        card_json: &str,
        extraction_node: Option<&SkillNode>,
        extraction_path: &str,
        llm_sem: &Arc<Semaphore>,
        req: &GenerationRequest<L>,
    ) -> Result<panini_core::morpheme::FeatureExtractionResponse<L::Morphology, L::GrammaticalFunction>> {
        let targets = any_card.targets().to_vec();
        let Some(ext_node) = extraction_node else {
            return Err(anyhow::anyhow!("Node not found for extraction"));
        };

        let build_extraction_request = || panini_engine::prompts::ExtractionRequest {
            content: card_json.to_string(),
            targets: targets.clone(),
            pedagogical_context: ext_node.node_instructions.clone(),
            skill_path: Some(extraction_path.to_string()),
            learner_ui_language: req.user_profile.ui_language.clone(),
            linguistic_background: to_panini_language_levels(&req.user_profile.linguistic_background),
            user_prompt: req.user_prompt.clone(),
        };

        macro_rules! call_panini {
            ($m:expr, $request:expr, $prev:expr) => {
                panini_engine::extract_features_via_llm(
                    self.language.linguistic_def(),
                    $m, $request,
                    panini_engine::ExtractionOptions {
                        temperature: self.extractor_temperature,
                        max_tokens: self.extractor_max_tokens,
                        previous_attempt: $prev,
                        extractor_prompts: &self.prompt_config.extractor,
                    },
                ).await
            }
        }

        let mut prev_attempt: Option<panini_engine::PreviousAttempt> = None;
        let mut backoff = backoff::ExponentialBackoffBuilder::new()
            .with_initial_interval(std::time::Duration::from_secs(1))
            .with_multiplier(2.0)
            .with_max_elapsed_time(Some(std::time::Duration::from_secs(30)))
            .build();

        loop {
            let _permit = llm_sem.acquire().await
                .map_err(|_| anyhow::anyhow!("LLM semaphore closed"))?;
            let backend = self.llm_backend.read()
                .map_err(|e| anyhow::anyhow!("LLM backend lock poisoned: {}", e))?.clone();
            let extraction_request = build_extraction_request();

            let r = tokio::time::timeout(self.llm_call_timeout, async {
                match &backend {
                    LlmBackend::OpenAi(m) => call_panini!(m, &extraction_request, prev_attempt.as_ref()),
                    LlmBackend::Anthropic(m) => call_panini!(m, &extraction_request, prev_attempt.as_ref()),
                    LlmBackend::Google(m) => call_panini!(m, &extraction_request, prev_attempt.as_ref()),
                }
            })
            .await
            .map_err(|_| panini_engine::ExtractionError::Parse(
                panini_engine::ExtractionParseError {
                    raw_response: String::new(),
                    error_message: format!("Feature extraction timed out after {:?}", self.llm_call_timeout),
                }
            ))
            .and_then(|inner| inner); // Flatten nested Result
            drop(_permit);

            match r {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    // Convert ExtractionError to anyhow::Error for downcast + retry logic
                    let e_anyhow: anyhow::Error = e.into();

                    // Determine next backoff interval; None means max elapsed time exceeded
                    let Some(wait) = backoff::backoff::Backoff::next_backoff(&mut backoff) else {
                        return Err(e_anyhow);
                    };

                    // Build correction context for the next attempt
                    if let Some(ext_err) = e_anyhow.downcast_ref::<panini_engine::ExtractionError>() {
                        if let panini_engine::ExtractionError::Parse(pe) = ext_err {
                            prev_attempt = Some(panini_engine::PreviousAttempt {
                                raw_response: pe.raw_response.clone(),
                                error: pe.error_message.clone(),
                            });
                        }
                    }

                    tracing::warn!(?wait, %e_anyhow, "Feature extraction attempt failed, retrying");
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }

    /// Runs all early post-processors (IPA, TTS) for a single card.
    pub(super) async fn run_early_post_processors(
        &self,
        any_card: &crate::card_models::AnyCard,
        card_id: &str,
        extra_value: &serde_json::Value,
        pp_sem: &Arc<Semaphore>,
        req: &GenerationRequest<L>,
    ) -> Vec<crate::post_process::EarlyPostProcessResult> {
        let _pp_permit = match pp_sem.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
                tracing::error!("Post-process semaphore closed");
                return Vec::new();
            }
        };
        let speakable = any_card.speakable_text();
        let input_chars = speakable.as_ref().map(|t| t.len() as u32).unwrap_or(0);
        let lang_code = self.language.linguistic_def().iso_code().to_639_3().to_string();

        let mut results = Vec::new();
        for processor in &self.early_processors {
            let pp_start = std::time::Instant::now();
            match processor.process(&self.language, card_id, any_card, extra_value).await {
                Ok(r) => {
                    if let (Some(recorder), Some(ctx)) = (&self.usage_recorder, &req.request_context) {
                        let latency_ms = pp_start.elapsed().as_millis() as u64;
                        if r.ipa.is_some() {
                            recorder.record_post_process(PostProcessEvent {
                                user_id: ctx.user_id.clone(),
                                request_id: ctx.request_id.clone(),
                                language: Some(lang_code.clone()),
                                process_type: PostProcessType::Ipa,
                                input_chars,
                                latency_ms,
                                success: true,
                            });
                        }
                        if r.audio_file.is_some() {
                            recorder.record_post_process(PostProcessEvent {
                                user_id: ctx.user_id.clone(),
                                request_id: ctx.request_id.clone(),
                                language: Some(lang_code.clone()),
                                process_type: PostProcessType::Tts,
                                input_chars,
                                latency_ms,
                                success: true,
                            });
                        }
                    }
                    results.push(r);
                }
                Err(e) => tracing::error!(%e, "Early post-processing error"),
            }
        }
        results
    }
}
