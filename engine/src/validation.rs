use async_trait::async_trait;
use lc_core::traits::Language;

use crate::card_models::AnyCard;

/// Validates a parsed card before it enters the processing pipeline.
/// Returns Ok(()) if valid, Err(feedback) with a message to inject into the retry prompt.
#[async_trait]
pub trait CardValidator<L: Language + Send + Sync>: Send + Sync
where L::Morphology: Send + Sync
{
    async fn validate(
        &self,
        language: &L,
        model: &AnyCard,
        extra_fields: &serde_json::Value,
    ) -> Result<(), String>;
}
