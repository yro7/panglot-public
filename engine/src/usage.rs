use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::llm_client::{CallType, LlmClient, LlmRequest, LlmResponse, RequestContext};

/// A single LLM usage event to be persisted for billing/monitoring.
#[derive(Debug, Clone)]
pub struct UsageEvent {
    pub ctx: RequestContext,
    pub provider: String,
    pub model: String,
    pub call_type: CallType,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
}

/// Fire-and-forget sender for usage events.
#[derive(Clone)]
pub struct UsageRecorder {
    tx: mpsc::UnboundedSender<UsageEvent>,
}

impl UsageRecorder {
    pub fn new(tx: mpsc::UnboundedSender<UsageEvent>) -> Self {
        Self { tx }
    }

    pub fn record(&self, event: UsageEvent) {
        let _ = self.tx.send(event); // fire-and-forget
    }
}

/// Returns both the recorder (sender) and receiver for wiring.
pub fn usage_channel() -> (UsageRecorder, mpsc::UnboundedReceiver<UsageEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (UsageRecorder::new(tx), rx)
}

/// Decorator: wraps any LlmClient, records usage after each successful call.
pub struct InstrumentedLlmClient<C: LlmClient> {
    inner: C,
    recorder: UsageRecorder,
    provider: String,
    model: String,
}

impl<C: LlmClient> InstrumentedLlmClient<C> {
    pub fn new(inner: C, recorder: UsageRecorder, provider: String, model: String) -> Self {
        Self { inner, recorder, provider, model }
    }
}

#[async_trait]
impl<C: LlmClient> LlmClient for InstrumentedLlmClient<C> {
    async fn chat_completion(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let response = self.inner.chat_completion(request).await?;
        self.recorder.record(UsageEvent {
            ctx: request.request_context.clone().unwrap_or_default(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            call_type: request.call_type,
            tokens_in: response.usage.tokens_in,
            tokens_out: response.usage.tokens_out,
            latency_ms: response.latency_ms,
        });
        Ok(response)
    }
}
