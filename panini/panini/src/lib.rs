pub use panini_core;
pub use panini_engine;

// Re-export key types at top level for ergonomics
pub use panini_core::{LinguisticDefinition, MorphologyInfo};
pub use panini_core::component::ComponentRequires;
pub use panini_core::component::{AnalysisComponent, ComponentContext, ExtractionResult, ExtractionResultError};
pub use panini_core::components::{
    MorphemeSegmentation, MorphologyAnalysis, MultiwordExpressions, PedagogicalExplanation,
};
pub use panini_engine::{extract_with_components, ExtractionRequest};
pub use panini_engine::extractor::{ExtractionError, ExtractionOptions};
pub use panini_macro::PaniniResult;

/// Internal re-exports used by `#[derive(PaniniResult)]` generated code.
///
/// This module is NOT part of the public API. Paths inside it may change
/// without notice. Consumer code should never reference `__macro_support`
/// directly.
#[doc(hidden)]
pub mod __macro_support {
    pub use panini_core;
    pub use panini_engine;
    pub use rig;
}

#[cfg(feature = "langs")]
pub use panini_langs;
