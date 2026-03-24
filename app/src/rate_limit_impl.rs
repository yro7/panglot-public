use async_trait::async_trait;
use sqlx::SqlitePool;

use lc_core::rate_limit::{LimitKind, RateLimitExceeded, RateLimitRule, RateLimiter, Result};
use lc_core::usage_analytics::TimePeriod;

use crate::config::RateLimitConfig;

pub struct SqliteRateLimiter {
    pool: SqlitePool,
    free_rules: Vec<RateLimitRule>,
    premium_rules: Vec<RateLimitRule>,
}

impl SqliteRateLimiter {
    pub fn from_config(pool: SqlitePool, config: &RateLimitConfig) -> Self {
        let build_rules = |tier: &crate::config::TierLimitConfig| {
            let mut rules = Vec::new();
            if tier.daily_token_limit > 0 {
                rules.push(RateLimitRule {
                    kind: LimitKind::LlmTokens,
                    period: TimePeriod::Last24Hours,
                    max_value: tier.daily_token_limit,
                });
            }
            if tier.hourly_call_limit > 0 {
                rules.push(RateLimitRule {
                    kind: LimitKind::LlmCalls,
                    period: TimePeriod::LastHour,
                    max_value: tier.hourly_call_limit,
                });
            }
            if tier.daily_tts_char_limit > 0 {
                rules.push(RateLimitRule {
                    kind: LimitKind::TtsChars,
                    period: TimePeriod::Last24Hours,
                    max_value: tier.daily_tts_char_limit,
                });
            }
            rules
        };

        Self {
            pool,
            free_rules: build_rules(&config.free),
            premium_rules: build_rules(&config.premium),
        }
    }
}

#[async_trait]
impl RateLimiter for SqliteRateLimiter {
    async fn check_limits(&self, user_id: &str, is_premium: bool) -> Result<std::result::Result<(), RateLimitExceeded>> {
        let active_rules = if is_premium { &self.premium_rules } else { &self.free_rules };
        for rule in active_rules {
            let usage = self.get_current_usage(user_id, rule.kind, rule.period).await?;
            if usage >= rule.max_value {
                return Ok(Err(RateLimitExceeded {
                    kind: rule.kind,
                    period: rule.period,
                    current_usage: usage,
                    max_allowed: rule.max_value,
                }));
            }
        }
        Ok(Ok(()))
    }

    async fn get_current_usage(
        &self,
        user_id: &str,
        kind: LimitKind,
        period: TimePeriod,
    ) -> Result<i64> {
        let cutoff = crate::state::now_ms() - period.as_millis();
        let value: (i64,) = match kind {
            LimitKind::LlmTokens => {
                sqlx::query_as(
                    "SELECT COALESCE(SUM(tokens_in + tokens_out), 0) \
                     FROM usage_logs WHERE user_id = ? AND created_at >= ? AND event_type = 'llm'",
                )
                .bind(user_id)
                .bind(cutoff)
                .fetch_one(&self.pool)
                .await?
            }
            LimitKind::LlmCalls => {
                sqlx::query_as(
                    "SELECT COUNT(*) \
                     FROM usage_logs WHERE user_id = ? AND created_at >= ? AND event_type = 'llm'",
                )
                .bind(user_id)
                .bind(cutoff)
                .fetch_one(&self.pool)
                .await?
            }
            LimitKind::TtsChars => {
                sqlx::query_as(
                    "SELECT COALESCE(SUM(input_chars), 0) \
                     FROM usage_logs WHERE user_id = ? AND created_at >= ? AND event_type = 'tts'",
                )
                .bind(user_id)
                .bind(cutoff)
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(value.0)
    }
}
