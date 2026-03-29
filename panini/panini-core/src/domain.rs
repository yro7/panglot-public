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
