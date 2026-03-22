use std::fmt;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::llm_client::{CallType, LlmClient, LlmRequest, LlmResponse, RequestContext};

// ── LLM Usage ──

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

// ── Post-Processing Usage ──

/// The type of post-processing operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostProcessType {
    Tts,
    Ipa,
}

impl fmt::Display for PostProcessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tts => write!(f, "tts"),
            Self::Ipa => write!(f, "ipa"),
        }
    }
}

/// A single post-processing usage event (TTS or IPA).
#[derive(Debug, Clone)]
pub struct PostProcessEvent {
    pub user_id: String,
    pub request_id: String,
    pub language: Option<String>,
    pub process_type: PostProcessType,
    pub input_chars: u32,
    pub latency_ms: u64,
    pub success: bool,
}

// ── Unified Pipeline Event ──

/// Unified event covering both LLM and post-processing operations.
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Llm(UsageEvent),
    PostProcess(PostProcessEvent),
}

// ── Recorder ──

/// Fire-and-forget sender for pipeline events.
#[derive(Clone)]
pub struct UsageRecorder {
    tx: mpsc::UnboundedSender<PipelineEvent>,
}

impl UsageRecorder {
    pub fn new(tx: mpsc::UnboundedSender<PipelineEvent>) -> Self {
        Self { tx }
    }

    /// Record an LLM usage event (backward-compatible wrapper).
    pub fn record(&self, event: UsageEvent) {
        let _ = self.tx.send(PipelineEvent::Llm(event));
    }

    /// Record a post-processing usage event.
    pub fn record_post_process(&self, event: PostProcessEvent) {
        let _ = self.tx.send(PipelineEvent::PostProcess(event));
    }
}

/// Returns both the recorder (sender) and receiver for wiring.
pub fn usage_channel() -> (UsageRecorder, mpsc::UnboundedReceiver<PipelineEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (UsageRecorder::new(tx), rx)
}

// ── Instrumented LLM Client ──

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
        if let Some(ctx) = &request.request_context {
            self.recorder.record(UsageEvent {
                ctx: ctx.clone(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                call_type: request.call_type,
                tokens_in: response.usage.tokens_in,
                tokens_out: response.usage.tokens_out,
                latency_ms: response.latency_ms,
            });
        }
        Ok(response)
    }
}
