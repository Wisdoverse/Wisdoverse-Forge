//! Anthropic Claude provider implementation.

use agentforge_core::RuntimeCapability;
use futures::stream::{BoxStream, StreamExt};
use reqwest::Client;

use crate::provider::{ChatMessage, ChatRequest, ChatResponse, LlmError, LlmProvider, LlmStream, StreamDelta, Usage};

/// Anthropic Messages API provider.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// If `base_url` is `None`, defaults to `https://api.anthropic.com`.
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        }
    }
}

/// Test-only constructor that sets a custom base URL directly.
#[cfg(test)]
impl AnthropicProvider {
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self { client: Client::new(), api_key, base_url }
    }
}

/// Split a `ChatRequest`'s messages into an optional top-level system prompt
/// (Anthropic's expected shape) and the remaining user/assistant turns.
fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system_text: Option<String> = None;
    let mut rest = Vec::with_capacity(messages.len());
    for m in messages {
        if m.role == "system" {
            system_text = Some(m.content.clone());
        } else {
            rest.push(serde_json::json!({ "role": m.role, "content": m.content }));
        }
    }
    (system_text, rest)
}

#[async_trait::async_trait]
impl LlmStream for AnthropicProvider {
    async fn stream(
        &self,
        mut request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError> {
        request.max_tokens.get_or_insert(4_096);
        let (system_text, messages) = split_system(&request.messages);
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "stream": true,
        });
        if let Some(sys) = system_text {
            body["system"] = serde_json::Value::String(sys);
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message: msg });
        }

        let byte_stream = resp.bytes_stream();
        let sse = parse_anthropic_sse(byte_stream);
        Ok(Box::pin(sse))
    }
}

/// Parse Anthropic SSE bytes into a delta stream.
///
/// Uses the shared [`crate::sse_framer::sse_data_payloads`] framer so that
/// multi-byte UTF-8 codepoints split across chunk boundaries are preserved
/// correctly. Emits a terminal `Done { finish_reason: "interrupted" }` when
/// the upstream socket closes without a `message_delta` frame.
fn parse_anthropic_sse(
    bytes: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl futures::Stream<Item = Result<StreamDelta, LlmError>> + Send + 'static {
    use crate::sse_framer::sse_data_payloads;

    async_stream::stream! {
        let mut payloads = Box::pin(sse_data_payloads(bytes));
        let mut input_tokens: u32 = 0;
        let mut done_emitted = false;

        while let Some(payload_result) = payloads.next().await {
            let payload = match payload_result {
                Err(e) => { yield Err(e); return; }
                Ok(p) if p.is_empty() => continue,
                Ok(p) => p,
            };
            let v: serde_json::Value = match serde_json::from_str(&payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("message_start") => {
                    if let Some(t) = v.pointer("/message/usage/input_tokens").and_then(|x| x.as_u64()) {
                        input_tokens = t as u32;
                    }
                }
                Some("content_block_delta") => {
                    if let Some(t) = v.pointer("/delta/text").and_then(|x| x.as_str()) {
                        yield Ok(StreamDelta::Text(t.to_string()));
                    }
                }
                Some("message_delta") => {
                    let finish = v
                        .pointer("/delta/stop_reason")
                        .and_then(|x| x.as_str())
                        .unwrap_or("stop")
                        .to_string();
                    let output_tokens =
                        v.pointer("/usage/output_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    yield Ok(StreamDelta::Usage { input_tokens, output_tokens });
                    yield Ok(StreamDelta::Done { finish_reason: finish });
                    done_emitted = true;
                }
                _ => {}
            }
        }

        // Terminal fallback: upstream closed without `message_delta`.
        if !done_emitted {
            yield Ok(StreamDelta::Usage { input_tokens, output_tokens: 0 });
            yield Ok(StreamDelta::Done { finish_reason: "interrupted".into() });
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn capability_profile(&self) -> RuntimeCapability {
        RuntimeCapability::api_provider_or_default(self.name(), 200_000)
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let (system_text, messages) = split_system(&request.messages);

        let mut body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": messages,
        });

        if let Some(sys) = system_text {
            body["system"] = serde_json::Value::String(sys);
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message: text });
        }

        let json: serde_json::Value = resp.json().await?;

        let content = json["content"]
            .as_array()
            .and_then(|blocks| blocks.first())
            .and_then(|block| block["text"].as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            tracing::warn!(response = %json, "Anthropic response missing expected content structure");
            return Err(LlmError::Parse("response missing content".into()));
        }

        let usage = json["usage"].as_object().map(|u| Usage {
            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(ChatResponse { content, model: json["model"].as_str().unwrap_or(&request.model).to_string(), usage })
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use futures::StreamExt;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SSE_BODY: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n\
event: content_block_delta
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n\
event: content_block_delta
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n\
event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
event: message_stop
data: {\"type\":\"message_stop\"}\n\n";

    #[tokio::test]
    async fn stream_extracts_system_message_to_top_level_field() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({
                "system": "be terse",
                "messages": [{"role": "user", "content": "hi"}]
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE_BODY).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;

        let provider = AnthropicProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "claude-sonnet-4-6".into(),
            messages: vec![
                ChatMessage { role: "system".into(), content: "be terse".into() },
                ChatMessage { role: "user".into(), content: "hi".into() },
            ],
            max_tokens: Some(100),
            temperature: None,
        };
        // If `system` was NOT lifted to the top-level field, the body_partial_json
        // matcher above would fail, wiremock would return 404, and stream() would
        // return LlmError::Api. Collecting at least one Text delta proves both that
        // the mock matched AND that the SSE parser wired through.
        let mut s = provider.stream(req).await.expect("mock did not match — system likely absent from body");
        let mut got_text = false;
        while let Some(delta) = s.next().await {
            if matches!(delta.unwrap(), StreamDelta::Text(_)) {
                got_text = true;
            }
        }
        assert!(got_text, "stream produced no text deltas");
    }

    #[tokio::test]
    async fn stream_concatenates_deltas_and_emits_done() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE_BODY).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;

        let provider = AnthropicProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "claude-sonnet-4-6".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: Some(100),
            temperature: None,
        };
        let mut s = provider.stream(req).await.unwrap();

        let mut text = String::new();
        let mut done = None;
        let mut usage = None;
        while let Some(delta) = s.next().await {
            match delta.unwrap() {
                StreamDelta::Text(t) => text.push_str(&t),
                StreamDelta::Usage { input_tokens, output_tokens } => usage = Some((input_tokens, output_tokens)),
                StreamDelta::Done { finish_reason } => done = Some(finish_reason),
            }
        }
        assert_eq!(text, "Hello");
        assert_eq!(done, Some("end_turn".into()));
        assert_eq!(usage, Some((10, 5)));
    }

    #[tokio::test]
    async fn emits_interrupted_done_on_truncated_stream() {
        // SSE body ends after one delta, without a message_delta frame.
        const TRUNCATED: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":7,\"output_tokens\":0}}}\n\n\
event: content_block_delta
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n";

        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(TRUNCATED)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;

        let provider = AnthropicProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "claude-sonnet-4-6".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: Some(100),
            temperature: None,
        };
        let mut s = provider.stream(req).await.unwrap();

        let mut text = String::new();
        let mut done = None;
        let mut usage = None;
        while let Some(delta) = s.next().await {
            match delta.unwrap() {
                StreamDelta::Text(t) => text.push_str(&t),
                StreamDelta::Usage { input_tokens, output_tokens } => usage = Some((input_tokens, output_tokens)),
                StreamDelta::Done { finish_reason } => done = Some(finish_reason),
            }
        }
        assert_eq!(text, "Hel");
        assert_eq!(done, Some("interrupted".into()));
        // input_tokens was captured from message_start; output_tokens is 0 (truncated).
        assert_eq!(usage, Some((7, 0)));
    }
}
