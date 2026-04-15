use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Time period for usage aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimePeriod {
    LastHour,
    Last24Hours,
    Last7Days,
    Last30Days,
}

impl TimePeriod {
    /// Duration in milliseconds.
    pub const fn as_millis(&self) -> i64 {
        match self {
            Self::LastHour => 3_600_000,
            Self::Last24Hours => 86_400_000,
            Self::Last7Days => 604_800_000,
            Self::Last30Days => 2_592_000_000,
        }
    }
}

/// Aggregated LLM usage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmUsageSummary {
    pub call_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

/// Aggregated post-processing usage (TTS + IPA).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostProcessUsageSummary {
    pub tts_calls: i64,
    pub ipa_calls: i64,
    pub tts_chars: i64,
    pub ipa_chars: i64,
}

/// Combined usage report for a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub period: TimePeriod,
    pub llm: LlmUsageSummary,
    pub post_process: PostProcessUsageSummary,
}

/// A single row in a breakdown (by language, model, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageBreakdownRow {
    pub key: String,
    pub call_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

/// Trait for querying usage analytics. Implementations are storage-specific.
#[async_trait]
pub trait UsageAnalyticsProvider: Send + Sync {
    /// Get aggregated usage (LLM + post-processing) for a user over a time period.
    async fn get_usage_report(
        &self,
        user_id: &str,
        period: TimePeriod,
    ) -> Result<UsageReport>;

    /// Get LLM usage broken down by language.
    async fn get_llm_breakdown_by_language(
        &self,
        user_id: &str,
        period: TimePeriod,
    ) -> Result<Vec<UsageBreakdownRow>>;

    /// Get LLM usage broken down by model.
    async fn get_llm_breakdown_by_model(
        &self,
        user_id: &str,
        period: TimePeriod,
    ) -> Result<Vec<UsageBreakdownRow>>;
}
