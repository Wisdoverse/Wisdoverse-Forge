//! Google Gemini provider (streaming via `:streamGenerateContent?alt=sse`).

use agentforge_core::RuntimeCapability;
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use reqwest::Client;

use crate::provider::{ChatMessage, ChatRequest, ChatResponse, LlmError, LlmProvider, LlmStream, StreamDelta, Usage};

// Path-safe encoding: escape everything except unreserved chars, keep `-._~` literal.
const MODEL_PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'#')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b':');

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self { client: Client::new(), api_key, base_url: "https://generativelanguage.googleapis.com".into() }
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self { client: Client::new(), api_key, base_url }
    }
}

/// Split a Gemini `ChatRequest` into `systemInstruction` (from `role: "system"`
/// messages) and the remaining user/assistant turns mapped to Gemini roles.
fn split_system_instruction(messages: &[ChatMessage]) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system_text: Option<String> = None;
    let mut contents = Vec::with_capacity(messages.len());
    for m in messages {
        if m.role == "system" {
            system_text = Some(m.content.clone());
            continue;
        }
        let role = if m.role == "assistant" { "model" } else { "user" };
        contents.push(serde_json::json!({
            "role": role,
            "parts": [{ "text": m.content }],
        }));
    }
    (system_text, contents)
}

#[async_trait]
impl LlmStream for GeminiProvider {
    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError> {
        let encoded_model = utf8_percent_encode(&request.model, MODEL_PATH).to_string();
        let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse", self.base_url, encoded_model);

        let (system_text, contents) = split_system_instruction(&request.messages);
        let mut body = serde_json::json!({ "contents": contents });
        if let Some(sys) = system_text {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{ "text": sys }],
            });
        }

        let resp = self.client.post(url).header("x-goog-api-key", &self.api_key).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(LlmError::Api {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(Box::pin(parse_gemini_sse(resp.bytes_stream())))
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "google" // aligned with user_llm_configs.provider key
    }

    fn capability_profile(&self) -> RuntimeCapability {
        RuntimeCapability::api_provider_or_default(self.name(), 1_000_000)
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        // Non-streaming fallback (aggregate the stream).
        let mut s = self.stream(request.clone()).await?;
        let mut content = String::new();
        let mut usage = None;
        while let Some(d) = s.next().await {
            match d? {
                StreamDelta::Text(t) => content.push_str(&t),
                StreamDelta::Usage { input_tokens, output_tokens } => {
                    usage = Some(Usage { input_tokens, output_tokens })
                }
                StreamDelta::Done { .. } => break,
            }
        }
        Ok(ChatResponse { content, model: request.model, usage })
    }
}

/// Parse Gemini SSE bytes into a delta stream.
///
/// Uses the shared [`crate::sse_framer::sse_data_payloads`] framer so that
/// multi-byte UTF-8 codepoints split across chunk boundaries are preserved
/// correctly. Emits a terminal `Done { finish_reason: "interrupted" }` when
/// the upstream socket closes without a `finishReason` frame.
fn parse_gemini_sse(
    bytes: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl futures::Stream<Item = Result<StreamDelta, LlmError>> + Send + 'static {
    use crate::sse_framer::sse_data_payloads;

    async_stream::stream! {
        let mut payloads = Box::pin(sse_data_payloads(bytes));
        let mut done_emitted = false;

        while let Some(payload_result) = payloads.next().await {
            let payload = match payload_result {
                Err(e) => { yield Err(e); return; }
                Ok(p) if p.is_empty() => continue,
                Ok(p) => p,
            };
            let v: serde_json::Value = match serde_json::from_str(payload.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(parts) = v.pointer("/candidates/0/content/parts").and_then(|x| x.as_array()) {
                for p in parts {
                    if let Some(t) = p.get("text").and_then(|x| x.as_str())
                        && !t.is_empty()
                    {
                        yield Ok(StreamDelta::Text(t.to_string()));
                    }
                }
            }
            if let Some(finish) = v.pointer("/candidates/0/finishReason").and_then(|x| x.as_str()) {
                let pt =
                    v.pointer("/usageMetadata/promptTokenCount").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let ct =
                    v.pointer("/usageMetadata/candidatesTokenCount").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                yield Ok(StreamDelta::Usage { input_tokens: pt, output_tokens: ct });
                yield Ok(StreamDelta::Done { finish_reason: finish.to_string() });
                done_emitted = true;
            }
        }

        // Terminal fallback: stream closed without a finishReason frame.
        if !done_emitted {
            yield Ok(StreamDelta::Usage { input_tokens: 0, output_tokens: 0 });
            yield Ok(StreamDelta::Done { finish_reason: "interrupted".into() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatMessage;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SSE: &str = "\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]}}]}\n\n\
data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5}}\n\n";

    #[tokio::test]
    async fn gemini_stream_concatenates() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.*:streamGenerateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = GeminiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gemini-2.0-pro".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: None,
            temperature: None,
        };
        let mut s = p.stream(req).await.unwrap();
        let mut text = String::new();
        let mut done = None;
        let mut usage = None;
        while let Some(d) = s.next().await {
            match d.unwrap() {
                StreamDelta::Text(t) => text.push_str(&t),
                StreamDelta::Done { finish_reason } => done = Some(finish_reason),
                StreamDelta::Usage { input_tokens, output_tokens } => usage = Some((input_tokens, output_tokens)),
            }
        }
        assert_eq!(text, "Hello");
        assert_eq!(done, Some("STOP".into()));
        assert_eq!(usage, Some((10, 5)));
    }

    #[tokio::test]
    async fn gemini_stream_lifts_system_role_to_system_instruction() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.*:streamGenerateContent"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({
                "systemInstruction": { "parts": [{ "text": "be terse" }] },
                "contents": [{ "role": "user", "parts": [{ "text": "hi" }] }]
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = GeminiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gemini-2.0-pro".into(),
            messages: vec![
                ChatMessage { role: "system".into(), content: "be terse".into() },
                ChatMessage { role: "user".into(), content: "hi".into() },
            ],
            max_tokens: None,
            temperature: None,
        };
        let mut s = p.stream(req).await.expect("mock body_partial_json did not match — systemInstruction not lifted");
        let mut got_text = false;
        while let Some(d) = s.next().await {
            if matches!(d.unwrap(), StreamDelta::Text(_)) {
                got_text = true;
            }
        }
        assert!(got_text, "stream produced no text deltas");
    }

    #[tokio::test]
    async fn gemini_stream_uses_header_not_query_key() {
        use wiremock::matchers::header;
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.*:streamGenerateContent"))
            .and(header("x-goog-api-key", "test"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = GeminiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gemini-2.0-pro".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: None,
            temperature: None,
        };
        let mut s = p.stream(req).await.expect("mock x-goog-api-key header did not match");
        while s.next().await.is_some() {}
    }

    #[tokio::test]
    async fn emits_interrupted_done_on_truncated_stream() {
        // Two delta frames, no finishReason frame — stream truncated.
        const TRUNCATED: &str = "\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]}}]}\n\n";

        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.*:streamGenerateContent"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(TRUNCATED)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = GeminiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gemini-2.0-pro".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: None,
            temperature: None,
        };
        let mut s = p.stream(req).await.unwrap();
        let mut text = String::new();
        let mut done = None;
        let mut got_usage = false;
        while let Some(d) = s.next().await {
            match d.unwrap() {
                StreamDelta::Text(t) => text.push_str(&t),
                StreamDelta::Done { finish_reason } => done = Some(finish_reason),
                StreamDelta::Usage { .. } => got_usage = true,
            }
        }
        assert_eq!(text, "Hello");
        assert_eq!(done, Some("interrupted".into()));
        assert!(got_usage, "expected a Usage delta before Done");
    }
}
