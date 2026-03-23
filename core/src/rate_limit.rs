use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::usage_analytics::TimePeriod;

/// The kind of resource being limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    /// Total LLM tokens (in + out) per time window.
    LlmTokens,
    /// Number of LLM calls per time window.
    LlmCalls,
    /// TTS characters per time window.
    TtsChars,
}

/// A single configured limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRule {
    pub kind: LimitKind,
    pub period: TimePeriod,
    pub max_value: i64,
}

/// Returned when a limit is exceeded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitExceeded {
    pub kind: LimitKind,
    pub period: TimePeriod,
    pub current_usage: i64,
    pub max_allowed: i64,
}

impl std::fmt::Display for RateLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rate limit exceeded for {:?} ({:?}): {}/{}",
            self.kind, self.period, self.current_usage, self.max_allowed
        )
    }
}

impl std::error::Error for RateLimitExceeded {}

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstract rate limiter. Implementations are storage-specific.
///
/// The return type of `check_limits` uses a nested `Result`:
/// - Outer `Result`: infrastructure errors (e.g. DB failure)
/// - Inner `Result`: business logic (limit exceeded or not)
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check all configured limits for a user.
    /// Returns `Ok(Ok(()))` if all limits pass, or `Ok(Err(...))` with the first exceeded limit.
    async fn check_limits(&self, user_id: &str) -> Result<std::result::Result<(), RateLimitExceeded>>;

    /// Get current usage for a specific limit kind and period.
    async fn get_current_usage(
        &self,
        user_id: &str,
        kind: LimitKind,
        period: TimePeriod,
    ) -> Result<i64>;
}
