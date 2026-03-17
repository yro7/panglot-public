use serde::{Deserialize, Serialize};
use anyhow::Result;
use lc_core::domain::{ExtractedFeature, MultiwordExpression};
use lc_core::traits::Language;

use crate::generator::GenerationRequest;
use crate::llm_client::{ChatMessage, LlmClient, LlmRequest, Role};
use crate::llm_utils::clean_llm_json;
use crate::prompts::{FeatureExtractorContext, PromptConfig};
use crate::skill_tree::SkillNode;

/// Error returned when feature extraction parsing fails, carrying the raw LLM output.
#[derive(Debug)]
pub struct ExtractionParseError {
    pub raw_response: String,
    pub error_message: String,
}

impl std::fmt::Display for ExtractionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_message)
    }
}

impl std::error::Error for ExtractionParseError {}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(bound = "M: schemars::JsonSchema")]
pub struct FeatureExtractionResponse<M> {
    /// PEDAGOGICAL EXPLANATION FORMAT:
    /// Write the ENTIRE explanation in the learner's interface language.
    /// The field must be an HTML string (no markdown). Structure it as follows:
    ///
    /// 1. **Translations**: Start with literal and natural translations (if they differ).
    ///    Use: <p><b>Translations:</b><br><i>Lit:</i> ...<br><i>Nat:</i> ...</p>
    ///
    /// 2. **Analysis**: A bullet list analyzing key grammatical components of the sentence.
    ///    - Focus on the grammar concepts relevant to the skill being tested.
    ///    - Highlight verbs in <span style='color:#e74c3c'><b>red</b></span>, nouns/subjects in <span style='color:#3498db'><b>blue</b></span>, grammar rules/cases in <span style='color:#27ae60'><b>green</b></span>.
    ///    - Do NOT analyze every single trivial word. Merge concepts where natural.
    ///    Use: <p><b>Analysis:</b></p><ul><li>...</li></ul>
    ///
    /// 3. **Grammar Recap**: A summary box of the specific declensions, conjugations, or rules used.
    ///    Use: <div style='background-color:#3a3a3a;color:#e0e0e0;padding:10px;border-radius:5px;margin-top:10px;border-left:4px solid #3498db'><b>Grammar Recap:</b><br>...</div>
    ///
    /// IMPORTANT: No introductory or concluding chatter. No "Here is..." or "Great example!". Just the structured analysis.
    pub pedagogical_explanation: String,
    /// Morphological features of the TARGET word(s) — what the card tests.
    pub target_features: Vec<ExtractedFeature<M>>,
    /// Morphological features of the surrounding CONTEXT words.
    pub context_features: Vec<ExtractedFeature<M>>,
    /// Multi-word expressions (idioms, collocations, phrasal expressions) found in the sentence.
    /// Extract these when a group of words forms a single semantic unit.
    #[serde(default)]
    pub multiword_expressions: Vec<MultiwordExpression>,
}

/// Previous failed attempt context for LLM self-correction retry.
pub struct PreviousAttempt {
    pub raw_response: String,
    pub error: String,
}

/// Call 2: extracts morphological features from a generated card's JSON.
///
/// Accepts the resolved `node` and `node_path` directly so this function
/// is agnostic to whether the tree was customized or not.
pub async fn extract_features_via_llm<L: Language + Send + Sync>(
    language: &L,
    node: &SkillNode,
    node_path: &str,
    llm_client: &dyn LlmClient,
    req: &GenerationRequest<L>,
    card_json: &str,
    targets: &[String],
    temperature: f32,
    max_tokens: u32,
    previous_attempt: Option<&PreviousAttempt>,
    prompt_config: &PromptConfig,
) -> Result<FeatureExtractionResponse<L::Morphology>>
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
{
    let system_prompt = FeatureExtractorContext::builder()
        .language(language)
        .skill_node(node)
        .node_path(node_path)
        .request(req)
        .prompt_config(prompt_config)
        .build()
        .generate_prompt()?;

    let schema = serde_json::to_value(
        &schemars::schema_for!(FeatureExtractionResponse<L::Morphology>)
    ).unwrap();

    let mut messages = vec![
        ChatMessage {
            role: Role::System,
            content: system_prompt,
        },
        ChatMessage {
            role: Role::User,
            content: format!(
                "Extract features from this card:\n{}\n\nTARGET WORDS: {:?}",
                card_json, targets
            ),
        },
    ];

    if let Some(prev) = previous_attempt {
        messages.push(ChatMessage {
            role: Role::Assistant,
            content: prev.raw_response.clone(),
        });
        messages.push(ChatMessage {
            role: Role::User,
            content: format!(
                "Your output is not conform to what I'm expecting. Please look at the error and correct yourself: {}",
                prev.error
            ),
        });
    }

    let request = LlmRequest {
        messages,
        temperature,
        max_tokens: Some(max_tokens),
        response_schema: Some(schema),
    };

    let response = llm_client.chat_completion(&request).await?;
    let cleaned = clean_llm_json(&response);
    let normalized = crate::llm_utils::normalize_pos_tags(cleaned);

    let response_parsed: FeatureExtractionResponse<L::Morphology> = match serde_json::from_str(&normalized) {
        Ok(parsed) => parsed,
        Err(e) => {
            let err_msg = e.to_string();
            eprintln!("Failed to parse feature extraction response:\nRAW:\n{}\nERROR:\n{}", normalized, err_msg);
            return Err(ExtractionParseError {
                raw_response: normalized,
                error_message: err_msg,
            }.into());
        }
    };
    Ok(response_parsed)
}
