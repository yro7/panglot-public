pub use panini_core;
pub use panini_engine;

// Re-export key types at top level for ergonomics
pub use panini_core::{LinguisticDefinition, MorphologyInfo};
pub use panini_core::morpheme::FeatureExtractionResponse;
pub use panini_core::component::{AnalysisComponent, ComponentContext, ExtractionResult, ExtractionResultError};
pub use panini_core::components::{
    MorphemeSegmentation, MorphologyAnalysis, MultiwordExpressions, PedagogicalExplanation,
};
pub use panini_engine::{extract_features_via_llm, extract_with_components, ExtractionRequest};

#[cfg(feature = "langs")]
pub use panini_langs;
