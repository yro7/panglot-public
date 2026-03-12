use serde::{Deserialize, Serialize};


/// A morphological feature extracted from a sentence.
/// Wraps the surface form (word as it appears) with its language-specific morphological analysis.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(bound = "M: schemars::JsonSchema")]
pub struct ExtractedFeature<M> {
    /// The word as it appears in the sentence (surface form).
    pub word: String,
    /// Language-specific morphological analysis (lemma, case, gender, etc.).
    pub morphology: M,
}

/// An idiomatic, multi-word expression extracted from a sentence. 
/// To be used if the meaning of the expression cannot be guessed purely from translation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MultiwordExpression {
    /// The base expression, put in a generic form. Examples :  \"robić z igły widły\" instead of \"Robisz z igły widły.\". Or \"faire la tête instead\" instead of \"tu fais la la tête !\".",
    pub text: String,
    /// The meaning or translation of the expression as a whole.
    pub meaning: String,
}

/// The metadata of a card. It contains the card id, the skill id
/// and the list of features extracted from the context text by the FeatureExtractor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardMetadata<M> {
    pub card_id: String,
    /// ISO 639-3 language code (e.g. "pol", "cmn"). Used to filter cards by language during lexicon scan.
    #[serde(default)]
    pub language: String,
    pub skill_id: String,
    pub skill_name: String,
    pub pedagogical_explanation: String,
    /// Features extracted from the target word(s) — what the card is testing.
    pub target_features: Vec<ExtractedFeature<M>>,
    /// Features extracted from the surrounding context words.
    pub context_features: Vec<ExtractedFeature<M>>,
    /// Multi-word expressions (idioms, collocations) found in the sentence.
    pub multiword_expressions: Vec<MultiwordExpression>,
    pub ipa: Option<String>,
    pub audio_file: Option<String>,
}

