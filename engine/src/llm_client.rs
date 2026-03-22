use serde::{Deserialize, Serialize};
use anyhow::Result;
use async_trait::async_trait;
use std::fmt;
use std::str::FromStr;

// ── Provider Enum ──

/// Known LLM providers with built-in URL & API-key conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Google,
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Custom,
}

impl LlmProvider {
    /// Default base URL for this provider's chat-completions endpoint.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::Google    => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::OpenAi    => "https://api.openai.com/v1",
            Self::Custom    => "",
        }
    }

    /// Environment variable name that holds the API key for this provider.
    pub fn default_api_key_env(&self) -> &'static str {
        match self {
            Self::Google    => "GOOGLE_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi    => "OPENAI_API_KEY",
            Self::Custom    => "LLM_API_KEY",
        }
    }

    /// Whether this provider uses the OpenAI-compatible `/chat/completions` format.
    fn is_openai_compatible(&self) -> bool {
        matches!(self, Self::Google | Self::OpenAi | Self::Custom)
    }
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Google    => write!(f, "google"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::OpenAi    => write!(f, "openai"),
            Self::Custom    => write!(f, "custom"),
        }
    }
}

impl FromStr for LlmProvider {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "google" | "gemini"    => Ok(Self::Google),
            "anthropic" | "claude" => Ok(Self::Anthropic),
            "openai" | "gpt"      => Ok(Self::OpenAi),
            "custom"              => Ok(Self::Custom),
            other => Err(anyhow::anyhow!(
                "Unknown LLM provider '{}'. Expected: google, anthropic, openai, custom", other
            )),
        }
    }
}

// ── Messages ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

// ── Token Usage & LLM Response ──

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cached_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub usage: TokenUsage,
    pub latency_ms: u64,
}

// ── Request Context (for billing decorator) ──

#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    pub user_id: String,
    pub request_id: String,
    pub endpoint: String,
    pub language: Option<String>,
}

/// Which pipeline step produced this LLM call.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub enum CallType {
    Generation,
    Extraction,
}

impl Default for CallType {
    fn default() -> Self { Self::Generation }
}

impl fmt::Display for CallType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generation => write!(f, "Generation"),
            Self::Extraction => write!(f, "Extraction"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub response_schema: Option<serde_json::Value>,
    pub request_context: Option<RequestContext>,
    pub call_type: CallType,
}

/// Trait for LLM providers — enables mock injection in tests.
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat_completion(&self, request: &LlmRequest) -> Result<LlmResponse>;
}

// ── Provider-aware HTTP Client ──

pub struct LlmHttpClient {
    provider: LlmProvider,
    api_key: String,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl LlmHttpClient {
    /// Build a client from a known provider.
    /// Reads the API key from the provider's default env var (e.g. `ANTHROPIC_API_KEY`).
    pub fn from_provider(provider: LlmProvider, model: impl Into<String>) -> Result<Self> {
        dotenvy::dotenv().ok();
        let env_var = provider.default_api_key_env();
        let api_key = std::env::var(env_var)
            .map_err(|_| anyhow::anyhow!("{} not set — add it to your .env", env_var))?;
        Ok(Self {
            provider,
            api_key,
            base_url: provider.default_base_url().to_string(),
            model: model.into(),
            client: reqwest::Client::new(),
        })
    }

    /// Build a fully custom client (arbitrary URL + key).
    pub fn custom(api_key: String, base_url: String, model: String, provider: LlmProvider) -> Self {
        Self { provider, api_key, base_url, model, client: reqwest::Client::new() }
    }

    // ── Anthropic-specific helpers ──

    fn build_anthropic_payload(&self, request: &LlmRequest) -> serde_json::Value {
        // Anthropic separates system from messages
        let system_text: String = request.messages.iter()
            .filter(|m| m.role == Role::System)
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let messages: Vec<serde_json::Value> = request.messages.iter()
            .filter(|m| m.role != Role::System)
            .map(|m| serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => unreachable!(),
                },
                "content": m.content,
            }))
            .collect();

        let mut payload = serde_json::json!({
            "model": self.model,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": messages,
            "temperature": request.temperature,
        });

        if !system_text.is_empty() {
            payload["system"] = serde_json::Value::String(system_text);
        }

        // Structured output via tool_use
        if let Some(schema) = &request.response_schema {
            let clean_schema = Self::sanitize_schema_for_anthropic(schema);
            payload["tools"] = serde_json::json!([{
                "name": "structured_output",
                "description": "Return the result as structured JSON matching this schema.",
                "input_schema": clean_schema,
            }]);
            payload["tool_choice"] = serde_json::json!({
                "type": "tool",
                "name": "structured_output",
            });
        }

        payload
    }

    /// Prepare a JSON Schema for Anthropic's tool `input_schema`.
    /// Anthropic requires `"type": "object"` at the top level and rejects `$schema`/`title`.
    /// schemars v1 may produce a root `$ref` — we inline it if so.
    fn sanitize_schema_for_anthropic(schema: &serde_json::Value) -> serde_json::Value {
        let mut s = schema.clone();

        if let Some(obj) = s.as_object_mut() {
            // Strip meta-keys Anthropic doesn't accept
            obj.remove("$schema");
            obj.remove("title");

            // If root uses $ref (e.g. {"$ref": "#/$defs/MyStruct", "$defs": {...}}),
            // resolve it by merging the referenced definition to the top level.
            if let Some(ref_val) = obj.remove("$ref") {
                if let Some(ref_path) = ref_val.as_str() {
                    // Extract def name from e.g. "#/$defs/MyStruct"
                    let def_name = ref_path
                        .strip_prefix("#/$defs/")
                        .or_else(|| ref_path.strip_prefix("#/definitions/"));

                    if let Some(name) = def_name {
                        // Look up the definition
                        let resolved = obj.get("$defs")
                            .or_else(|| obj.get("definitions"))
                            .and_then(|defs| defs.get(name))
                            .cloned();

                        if let Some(mut def) = resolved {
                            // Merge definition properties into root
                            if let Some(def_obj) = def.as_object_mut() {
                                def_obj.remove("title");
                                for (k, v) in def_obj.iter() {
                                    obj.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Ensure type: "object" is present
            if !obj.contains_key("type") {
                obj.insert("type".to_string(), serde_json::json!("object"));
            }
        }


        s
    }

    fn build_openai_payload(&self, request: &LlmRequest) -> serde_json::Value {
        let mut payload = serde_json::json!({
            "model": self.model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        if let Some(schema) = &request.response_schema {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("response_format".to_string(), serde_json::json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": "structured_output",
                        "schema": schema,
                        "strict": true,
                    }
                }));
            }
        }

        payload
    }

    fn build_request(&self, request: &LlmRequest) -> reqwest::RequestBuilder {
        if self.provider.is_openai_compatible() {
            let payload = self.build_openai_payload(request);
            self.client
                .post(format!("{}/chat/completions", self.base_url))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&payload)
        } else {
            // Anthropic
            let payload = self.build_anthropic_payload(request);
            self.client
                .post(format!("{}/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&payload)
        }
    }

    fn extract_content(&self, json: &serde_json::Value) -> Result<(String, TokenUsage)> {
        if self.provider.is_openai_compatible() {
            // OpenAI format: choices[0].message.content
            let finish_reason = json["choices"][0]["finish_reason"].as_str().unwrap_or("unknown");
            if finish_reason != "stop" {
                tracing::warn!(finish_reason, "LLM non-stop finish reason");
            }
            let usage_json = &json["usage"];
            let token_usage = TokenUsage {
                tokens_in: usage_json["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                tokens_out: usage_json["completion_tokens"].as_u64().unwrap_or(0) as u32,
                cached_tokens: usage_json["prompt_tokens_details"]["cached_tokens"].as_u64().map(|v| v as u32),
            };
            tracing::info!(tokens_in = token_usage.tokens_in, tokens_out = token_usage.tokens_out, finish_reason, "LLM response (OpenAI)");

            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| anyhow::anyhow!("Failed to extract content from OpenAI-compatible response"))?;
            Ok((content, token_usage))
        } else {
            // Anthropic format: content[0].text or content[0].input (tool_use)
            let stop_reason = json["stop_reason"].as_str().unwrap_or("unknown");
            let usage_json = &json["usage"];
            let token_usage = TokenUsage {
                tokens_in: usage_json["input_tokens"].as_u64().unwrap_or(0) as u32,
                tokens_out: usage_json["output_tokens"].as_u64().unwrap_or(0) as u32,
                cached_tokens: usage_json["cache_read_input_tokens"].as_u64().map(|v| v as u32),
            };
            tracing::info!(tokens_in = token_usage.tokens_in, tokens_out = token_usage.tokens_out, stop_reason, "LLM response (Anthropic)");

            // Check for tool_use blocks first (structured output)
            if let Some(content) = json["content"].as_array() {
                for block in content {
                    if block["type"].as_str() == Some("tool_use") {
                        return Ok((serde_json::to_string(&block["input"])?, token_usage));
                    }
                }
                // Fallback: return text blocks
                for block in content {
                    if block["type"].as_str() == Some("text") {
                        if let Some(text) = block["text"].as_str() {
                            return Ok((text.to_string(), token_usage));
                        }
                    }
                }
            }

            Err(anyhow::anyhow!("Failed to extract content from Anthropic response"))
        }
    }
}

#[async_trait]
impl LlmClient for LlmHttpClient {
    async fn chat_completion(&self, request: &LlmRequest) -> Result<LlmResponse> {
        let span = tracing::info_span!("llm_call",
            provider = %self.provider,
            model = %self.model,
            tokens_in = tracing::field::Empty,
            tokens_out = tracing::field::Empty,
        );
        let _enter = span.enter();

        let start = std::time::Instant::now();

        let backoff_config = backoff::ExponentialBackoffBuilder::new()
            .with_initial_interval(std::time::Duration::from_secs(1))
            .with_multiplier(2.0)
            .with_randomization_factor(0.25)
            .with_max_elapsed_time(Some(std::time::Duration::from_secs(30)))
            .build();

        let resp = backoff::future::retry(backoff_config, || async {
            let req = self.build_request(request);
            match req.send().await {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        Ok(r)
                    } else if matches!(status.as_u16(), 429 | 503 | 529) {
                        let _error_text = r.text().await.unwrap_or_default();
                        tracing::warn!(%status, "LLM API returned retryable status");
                        Err(backoff::Error::transient(anyhow::anyhow!("LLM API retryable error ({})", status)))
                    } else {
                        let error_text = r.text().await.unwrap_or_default();
                        Err(backoff::Error::permanent(anyhow::anyhow!("LLM API error ({}): {}", status, error_text)))
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "LLM network error, retrying");
                    Err(backoff::Error::transient(anyhow::anyhow!(e)))
                }
            }
        }).await?;

        let latency_ms = start.elapsed().as_millis() as u64;
        let json: serde_json::Value = resp.json().await?;
        let (content, usage) = self.extract_content(&json)?;
        span.record("tokens_in", usage.tokens_in);
        span.record("tokens_out", usage.tokens_out);
        Ok(LlmResponse { content, usage, latency_ms })
    }
}

// ── Backward-compat alias ──

// ── Mock Client (tests) ──

pub struct MockLlmClient {
    responses: Vec<String>,
    call_index: std::sync::Mutex<usize>,
}

impl MockLlmClient {
    pub fn new(responses: Vec<String>) -> Self {
        Self { responses, call_index: std::sync::Mutex::new(0) }
    }

    pub fn with_fixed_response(response: impl Into<String>) -> Self {
        Self::new(vec![response.into()])
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn chat_completion(&self, _request: &LlmRequest) -> Result<LlmResponse> {
        let mut idx = self.call_index.lock().unwrap();
        let resp = self.responses.get(*idx).or(self.responses.last())
            .cloned().ok_or_else(|| anyhow::anyhow!("MockLlmClient: no responses"))?;
        *idx += 1;
        Ok(LlmResponse { content: resp, usage: TokenUsage::default(), latency_ms: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request() -> LlmRequest {
        LlmRequest {
            messages: vec![], temperature: 0.7, max_tokens: None,
            response_schema: None, request_context: None, call_type: CallType::Generation,
        }
    }

    #[tokio::test]
    async fn mock_fixed_response() {
        let client = MockLlmClient::with_fixed_response(r#"{"sentence": "Test"}"#);
        let resp = client.chat_completion(&test_request()).await.unwrap();
        assert!(resp.content.contains("Test"));
    }

    #[tokio::test]
    async fn mock_sequential_responses() {
        let client = MockLlmClient::new(vec!["first".into(), "second".into()]);
        let req = test_request();
        assert_eq!(client.chat_completion(&req).await.unwrap().content, "first");
        assert_eq!(client.chat_completion(&req).await.unwrap().content, "second");
        assert_eq!(client.chat_completion(&req).await.unwrap().content, "second"); // repeats last
    }

    #[test]
    fn provider_from_str_roundtrip() {
        for name in &["google", "gemini", "anthropic", "claude", "openai", "gpt", "custom"] {
            let provider: LlmProvider = name.parse().unwrap();
            // Display should produce the canonical name
            let canonical = provider.to_string();
            let roundtrip: LlmProvider = canonical.parse().unwrap();
            assert_eq!(provider, roundtrip);
        }
    }

    #[test]
    fn provider_base_urls_non_empty() {
        for provider in &[LlmProvider::Google, LlmProvider::Anthropic, LlmProvider::OpenAi] {
            assert!(!provider.default_base_url().is_empty(), "{:?} has empty base_url", provider);
            assert!(!provider.default_api_key_env().is_empty(), "{:?} has empty env var", provider);
        }
    }

    #[test]
    fn sanitize_schema_produces_valid_anthropic_input() {
        // Use a real schemars schema to test sanitization
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code)]
        struct TestStruct {
            name: String,
            value: i32,
        }
        let raw = serde_json::to_value(&schemars::schema_for!(TestStruct)).unwrap();
        eprintln!("RAW schemars schema:\n{}", serde_json::to_string_pretty(&raw).unwrap());

        let sanitized = LlmHttpClient::sanitize_schema_for_anthropic(&raw);
        eprintln!("SANITIZED schema:\n{}", serde_json::to_string_pretty(&sanitized).unwrap());

        let obj = sanitized.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"),
            "top-level type must be 'object'");
        assert!(!obj.contains_key("$schema"), "must not contain $schema");
        assert!(!obj.contains_key("title"), "must not contain title");
        assert!(!obj.contains_key("$ref"), "must not contain $ref at top level");
        assert!(obj.contains_key("properties"), "must have properties after $ref resolution");
    }
    #[tokio::test]
    #[ignore] // Run manually with: cargo test test_anthropic_api_schemas -- --ignored --nocapture
    async fn test_anthropic_api_schemas() {
        dotenvy::dotenv().ok();
        let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
        let client = reqwest::Client::new();
        
        // 1. Array type properties {"type": ["string", "null"]}
        let array_type_schema = serde_json::json!({
            "type": "object",
            "properties": { "test": { "type": ["string", "null"] } }
        });
        
        // 2. $defs schema (which FeatureExtractionResponse uses)
        let defs_schema = serde_json::json!({
            "type": "object",
            "$defs": { "MyDef": { "type": "string" } },
            "properties": { "test": { "$ref": "#/$defs/MyDef" } }
        });
        
        // 3. Raw FeatureExtractionResponse schema cleaned by our function
        let raw = serde_json::to_value(&schemars::schema_for!(crate::feature_extractor::FeatureExtractionResponse<langs::PolishMorphology>)).unwrap();
        let sanitized = LlmHttpClient::sanitize_schema_for_anthropic(&raw);

        for (name, schema) in [
            ("Array type", array_type_schema), 
            ("$defs schema", defs_schema),
            ("Sanitized FeatureExtractionResponse", sanitized)
        ] {
            let payload = serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "messages": [{"role": "user", "content": "Hi"}],
                "max_tokens": 100,
                "tools": [{
                    "name": "structured_output",
                    "description": "test",
                    "input_schema": schema
                }],
                "tool_choice": {
                    "type": "tool",
                    "name": "structured_output"
                }
            });
            
            let res = client.post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&payload)
                .send()
                .await.unwrap();
                
            let text = res.text().await.unwrap();
            eprintln!("Test: {}\nResult: {}\n", name, text);
        }
    }

    #[test]
    fn dump_build_anthropic_payload() {
        let client = LlmHttpClient::custom("key".into(), "url".into(), "model".into(), LlmProvider::Anthropic);
        
        let schema1 = crate::card_models::AnyCard::schema_json_value::<langs::Polish>(crate::card_models::CardModelId::ClozeTest);
        let req1 = LlmRequest { messages: vec![], temperature: 0.0, max_tokens: None, response_schema: Some(schema1), request_context: None, call_type: CallType::Generation };
        let payload1 = client.build_anthropic_payload(&req1);
        eprintln!("PAYLOAD 1 (CardModel):\n{}", serde_json::to_string_pretty(&payload1.get("tools")).unwrap());

        let raw2 = serde_json::to_value(&schemars::schema_for!(crate::feature_extractor::FeatureExtractionResponse<langs::PolishMorphology>)).unwrap();
        let req2 = LlmRequest { messages: vec![], temperature: 0.0, max_tokens: None, response_schema: Some(raw2), request_context: None, call_type: CallType::Extraction };
        let payload2 = client.build_anthropic_payload(&req2);
        eprintln!("PAYLOAD 2 (FeatureExt):\n{}", serde_json::to_string_pretty(&payload2.get("tools")).unwrap());
    }
}
