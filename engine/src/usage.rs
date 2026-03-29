use std::fmt;
use tokio::sync::mpsc;

// ── LLM Usage ──

/// A single LLM usage event to be persisted for billing/monitoring.
#[derive(Debug, Clone)]
pub struct LlmProviderUsageEvent {
    pub user_id: String,
    pub request_id: String,
    pub endpoint: String,
    pub call_type: String,
    pub provider: String,
    pub model: String,
    pub language: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
    pub is_error: bool,
    pub error_message: Option<String>,
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
    Llm(LlmProviderUsageEvent),
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

    /// Record an LLM usage event.
    pub fn record_llm_call(&self, event: LlmProviderUsageEvent) {
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
