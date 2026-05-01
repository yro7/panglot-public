use serde::{Deserialize, Serialize};

// Re-export extraction-related types from panini-core
pub use panini_core::domain::{ExtractedFeature, MultiwordExpression};

/// The metadata of a card. It contains the card id, the skill id
/// and the list of features extracted from the context text by the `FeatureExtractor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "M: for<'de2> Deserialize<'de2>, F: for<'de2> Deserialize<'de2>"))]
pub struct CardMetadata<M, F = ()> {
    pub card_id: String,
    /// ISO 639-3 language code (e.g. "pol", "cmn"). Used to filter cards by language during lexicon scan.
    #[serde(default)]
    pub language: String,
    /// The id of the tree node that triggered this generation. Snapshot taken
    /// at gen time — survives YAML edits / node renames as opaque string.
    #[serde(alias = "skill_id")]
    pub source_node_id: String,
    /// Display name of the source node at gen time. Used by the study UI; not
    /// authoritative — re-resolve from the current tree if you need fresh data.
    #[serde(alias = "skill_name")]
    pub source_node_name: String,
    pub pedagogical_explanation: String,
    /// Features extracted from the target word(s) — what the card is testing.
    pub target_features: Vec<ExtractedFeature<M>>,
    /// Features extracted from the surrounding context words.
    pub context_features: Vec<ExtractedFeature<M>>,
    /// Multi-word expressions (idioms, collocations) found in the sentence.
    pub multiword_expressions: Vec<MultiwordExpression>,
    pub ipa: Option<String>,
    pub audio_file: Option<String>,
    /// Morpheme segmentation — present only for agglutinative languages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub morpheme_segmentation: Option<Vec<crate::morpheme::WordSegmentation<F>>>,
}
