pub use panini_core;
pub use panini_engine;

// Re-export key types at top level for ergonomics
pub use panini_core::{LinguisticDefinition, MorphologyInfo};
pub use panini_core::morpheme::FeatureExtractionResponse;
pub use panini_engine::{extract_features_via_llm, ExtractionRequest};
// LlmClient has been removed — use rig::completion::CompletionModel directly.
// Add rig-core as a direct dependency in your crate and pass any CompletionModel
// to extract_features_via_llm.

#[cfg(feature = "langs")]
pub use panini_langs;
