use serde::{Deserialize, Serialize};
use lc_core::traits::Language;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::generator::GenerationRequest;
use crate::llm_utils::clean_llm_json;
use crate::skill_tree::SkillNode;

use super::core::Pipeline;
use super::types::GeneratedCard;

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
    /// Generates a batch of cards using the dual-call LLM strategy.
    pub async fn generate_cards_batch(
        &self,
        tree_root: &SkillNode,
        req: &GenerationRequest<L>,
        skill_node_id: &str,
        llm_semaphore: Arc<Semaphore>,
        post_process_semaphore: Arc<Semaphore>,
    ) -> Result<Vec<GeneratedCard<L::Morphology, L::GrammaticalFunction>>> {
        if req.num_cards == 0 {
            return Ok(Vec::new());
        }

        // --- Call 1: Content Generation (Batched) ---
        let system_content = self.build_generator_system_prompt(tree_root, req, skill_node_id)?;

        let user_content = format!(
            "Difficulty level: {}/10.{}\n\nRespond with valid JSON only, no markdown.",
            req.difficulty,
            req.user_prompt
                .as_deref()
                .map(|p| format!(" {}", p))
                .unwrap_or_default(),
        );

        let item_schema = crate::card_models::AnyCard::schema_json_value::<L>(req.card_model_id);
        // LLM structured outputs (Anthropic tool_use and OpenAI JSON schema) strictly require
        // the top-level schema to be an object, not an array. We wrap it in a `cards` property.
        let array_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "cards": {
                    "type": "array",
                    "items": item_schema,
                }
            },
            "required": ["cards"],
        });

        let raw_json = {
            let _permit = llm_semaphore.acquire().await
                .map_err(|_| anyhow::anyhow!("LLM semaphore closed"))?;
            let backend = self.llm_backend.read()
                .map_err(|e| anyhow::anyhow!("LLM backend lock poisoned: {}", e))?.clone();
            let rig_schema: schemars::Schema = serde_json::from_value(array_schema)?;

            tokio::time::timeout(self.llm_call_timeout, backend.execute_generation(
                &system_content, &user_content, rig_schema,
                self.generator_temperature, self.generator_max_tokens,
                "Generation",
                self.usage_recorder.as_ref(), req.request_context.as_ref(),
            )).await
                .map_err(|_| anyhow::anyhow!("LLM generation call timed out after {:?}", self.llm_call_timeout))??
        };
        tracing::debug!(raw_len = raw_json.len(), "Raw LLM content output");

        let cleaned_json = clean_llm_json(&raw_json);

        tracing::debug!(cleaned_len = cleaned_json.len(), "Cleaned LLM content output");

        // --- Parse the wrapper object and extract the array of cards ---
        let mut parsed_root: serde_json::Value = serde_json::from_str(cleaned_json)?;
        let json_array: Vec<serde_json::Value> = if let Some(arr) = parsed_root.get_mut("cards").and_then(|v| v.as_array_mut()) {
            std::mem::take(arr)
        } else if cleaned_json.starts_with('[') {
             // Fallback just in case the LLM returned the array directly ignoring the schema wrapper
             serde_json::from_str(cleaned_json)?
        } else if req.num_cards == 1 {
             vec![parsed_root]
        } else {
             return Err(anyhow::anyhow!("LLM returned an object without 'cards' array"));
        };

        // Pre-resolve node info for use inside the async closures
        let skill_name = crate::skill_tree::find_node(tree_root, skill_node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let node_for_extraction = crate::skill_tree::find_node(tree_root, skill_node_id).cloned();
        let node_path_for_extraction = crate::skill_tree::get_node_path(tree_root, skill_node_id)
            .unwrap_or_default();

        let mut fetch_tasks = Vec::new();

        for value in json_array.into_iter() {
             let card_json_str = serde_json::to_string(&value)?;

             // --- Parse + Validate (with 1 retry) ---
             let parsed = match self.parse_and_validate(req, &card_json_str).await {
                 Ok(result) => result,
                 Err(feedback) => {
                     tracing::warn!(%feedback, "Card validation failed, retrying");
                     match self.retry_single_card(tree_root, req, skill_node_id, &feedback, &llm_semaphore).await {
                         Ok(result) => result,
                         Err(e) => {
                             tracing::error!(%e, "Card abandoned after retry");
                             continue;
                         }
                     }
                 }
             };

             let (any_card, extra_fields, card_json_for_extraction) = parsed;
             let llm_sem = llm_semaphore.clone();
             let pp_sem = post_process_semaphore.clone();

             fetch_tasks.push(self.process_single_card(
                 any_card, extra_fields, card_json_for_extraction,
                 skill_node_id, skill_name.clone(),
                 node_for_extraction.clone(), node_path_for_extraction.clone(),
                 llm_sem, pp_sem, req,
             ));
        }

        let generated_cards = futures::future::join_all(fetch_tasks).await;

        Ok(generated_cards)
    }
}
