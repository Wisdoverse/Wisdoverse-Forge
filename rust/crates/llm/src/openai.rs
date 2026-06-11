//! OpenAI-compatible provider implementation (also works for Ollama).

use agentforge_core::RuntimeCapability;
use futures::stream::{BoxStream, StreamExt};
use reqwest::Client;

use crate::provider::{ChatRequest, ChatResponse, LlmError, LlmProvider, LlmStream, StreamDelta, Usage};

/// OpenAI Chat Completions API provider.
///
/// Also works with any OpenAI-compatible API (e.g., Ollama, vLLM, LiteLLM).
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    provider_name: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider.
    ///
    /// If `base_url` is `None`, defaults to `https://api.openai.com`.
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".to_string()),
            provider_name: "openai".to_string(),
        }
    }

    /// Create an Ollama-compatible provider (no API key needed).
    pub fn ollama(base_url: String) -> Self {
        Self { client: Client::new(), api_key: String::new(), base_url, provider_name: "ollama".to_string() }
    }

    /// Create a provider for an OpenAI-compatible API.
    ///
    /// `base_url` accepts both shapes vendors document: a service root before
    /// `/v1/chat/completions` (Groq uses `https://api.groq.com/openai`) or a
    /// fully versioned base the OpenAI SDK style appends `/chat/completions`
    /// to (MiniMax uses `https://api.minimaxi.com/v1`, Zhipu uses
    /// `https://open.bigmodel.cn/api/paas/v4`). See [`chat_completions_url`].
    pub fn compatible(provider_name: impl Into<String>, api_key: String, base_url: String) -> Self {
        Self { client: Client::new(), api_key, base_url, provider_name: provider_name.into() }
    }
}

/// Join the Chat Completions path onto a configured base URL.
///
/// Appends `/v1/chat/completions` to service roots, but only
/// `/chat/completions` when the base already ends in a version segment such as
/// `/v1` or `/api/paas/v4` — the shape OpenAI-SDK-style vendor docs publish.
/// Without this, a pasted vendor base URL would double the version segment
/// (`…/v1/v1/chat/completions`).
fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let already_versioned = trimmed.rsplit('/').next().is_some_and(|segment| {
        segment.len() >= 2 && segment.starts_with('v') && segment[1..].chars().all(|c| c.is_ascii_digit())
    });
    if already_versioned { format!("{trimmed}/chat/completions") } else { format!("{trimmed}/v1/chat/completions") }
}

/// Test-only constructor that sets a custom base URL directly.
#[cfg(test)]
impl OpenAiProvider {
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self { client: Client::new(), api_key, base_url, provider_name: "openai".to_string() }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait::async_trait]
impl LlmStream for OpenAiProvider {
    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError> {
        let mut req = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "stream": true,
            // `stream_options.include_usage` makes real OpenAI send usage in a
            // dedicated frame after the finish_reason frame, before [DONE].
            // Strict OpenAI-compat servers may ignore or reject this field;
            // currently acceptable for Ollama which ignores unknown fields.
            "stream_options": { "include_usage": true },
        });
        if let Some(mt) = request.max_tokens {
            req["max_tokens"] = mt.into();
        }
        if let Some(t) = request.temperature {
            req["temperature"] = t.into();
        }

        let mut rb = self
            .client
            .post(chat_completions_url(&self.base_url))
            .header("content-type", "application/json")
            .json(&req);
        if !self.api_key.is_empty() {
            rb = rb.header("authorization", format!("Bearer {}", self.api_key));
        }
        let resp = rb.send().await?;
        if !resp.status().is_success() {
            return Err(LlmError::Api {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        Ok(Box::pin(parse_openai_sse(resp.bytes_stream())))
    }
}

/// Parse OpenAI SSE bytes into a delta stream.
///
/// Uses the shared [`crate::sse_framer::sse_data_payloads`] framer so that
/// multi-byte UTF-8 codepoints split across chunk boundaries are preserved
/// correctly.
///
/// Real OpenAI with `stream_options: { include_usage: true }` sends three
/// distinct terminal frames:
///   1. `{"choices":[{"delta":{},"finish_reason":"stop"}]}`  — captures finish_reason
///   2. `{"choices":[],"usage":{"prompt_tokens":N,"completion_tokens":M}}`  — usage
///   3. `data: [DONE]`  — triggers terminal emission of Usage then Done
///
/// We thread `finish_reason`, `input_tokens`, and `output_tokens` as state
/// across frames so that [DONE] can fire both deltas together. If the stream
/// closes without `[DONE]`, a terminal `Done` is emitted with whatever
/// `finish_reason` was captured (or `"interrupted"` if none was seen).
fn parse_openai_sse(
    bytes: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl futures::Stream<Item = Result<StreamDelta, LlmError>> + Send + 'static {
    use crate::sse_framer::sse_data_payloads;

    async_stream::stream! {
        let mut payloads = Box::pin(sse_data_payloads(bytes));
        let mut finish_reason: Option<String> = None;
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;
        let mut done_emitted = false;

        while let Some(payload_result) = payloads.next().await {
            let payload = match payload_result {
                Err(e) => { yield Err(e); return; }
                Ok(p) => p,
            };
            let payload = payload.trim();
            if payload == "[DONE]" {
                // Terminal: emit accumulated usage then done.
                let reason = finish_reason.take().unwrap_or_else(|| "stop".into());
                yield Ok(StreamDelta::Usage { input_tokens, output_tokens });
                yield Ok(StreamDelta::Done { finish_reason: reason });
                done_emitted = true;
                continue;
            }
            if payload.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Text delta frame.
            if let Some(content) = v.pointer("/choices/0/delta/content").and_then(|x| x.as_str())
                && !content.is_empty()
            {
                yield Ok(StreamDelta::Text(content.to_string()));
            }
            // finish_reason frame — capture, do not emit yet.
            if let Some(finish) = v.pointer("/choices/0/finish_reason").and_then(|x| x.as_str()) {
                finish_reason = Some(finish.to_string());
            }
            // Usage frame — capture, do not emit yet.
            if let Some(usage) = v.get("usage").and_then(|u| u.as_object()) {
                input_tokens = usage.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                output_tokens = usage.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            }
        }

        // Terminal fallback: stream closed without [DONE].
        if !done_emitted {
            let reason = finish_reason.unwrap_or_else(|| "interrupted".into());
            yield Ok(StreamDelta::Usage { input_tokens, output_tokens });
            yield Ok(StreamDelta::Done { finish_reason: reason });
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn capability_profile(&self) -> RuntimeCapability {
        let max_context_tokens = if self.provider_name == "ollama" { 8_192 } else { 128_000 };
        RuntimeCapability::api_provider_or_default(self.name(), max_context_tokens)
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let mut req = self
            .client
            .post(chat_completions_url(&self.base_url))
            .header("content-type", "application/json")
            .json(&body);

        if !self.api_key.is_empty() {
            req = req.header("authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status != 200 {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message: text });
        }

        let json: serde_json::Value = resp.json().await?;

        let content = json["choices"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|c| c["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            tracing::warn!(response = %json, "OpenAI response missing expected content structure");
            return Err(LlmError::Parse("response missing content".into()));
        }

        let usage = json["usage"].as_object().map(|u| Usage {
            input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            output_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        Ok(ChatResponse { content, model: json["model"].as_str().unwrap_or(&request.model).to_string(), usage })
    }
}

#[cfg(test)]
mod url_tests {
    use super::chat_completions_url;

    #[test]
    fn appends_v1_segment_to_service_roots() {
        assert_eq!(
            chat_completions_url("https://api.groq.com/openai"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        assert_eq!(chat_completions_url("https://api.deepseek.com"), "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(chat_completions_url("http://litellm:4000"), "http://litellm:4000/v1/chat/completions");
        assert_eq!(chat_completions_url("http://localhost:11434"), "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn does_not_double_version_segment_on_versioned_bases() {
        assert_eq!(chat_completions_url("https://api.minimaxi.com/v1"), "https://api.minimaxi.com/v1/chat/completions");
        assert_eq!(
            chat_completions_url("https://open.bigmodel.cn/api/paas/v4"),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
        assert_eq!(chat_completions_url("https://api.moonshot.cn/v1/"), "https://api.moonshot.cn/v1/chat/completions");
    }

    #[test]
    fn non_version_trailing_segments_still_get_v1() {
        assert_eq!(chat_completions_url("https://host.example/vllm"), "https://host.example/vllm/v1/chat/completions");
        assert_eq!(
            chat_completions_url("https://host.example/v1beta"),
            "https://host.example/v1beta/v1/chat/completions"
        );
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::provider::ChatMessage;
    use futures::StreamExt;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Real OpenAI dual-frame shape: finish_reason and usage arrive in separate
    // frames; [DONE] is the terminator that triggers the terminal emission.
    const SSE_BODY: &str = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n\
data: [DONE]\n\n";

    #[tokio::test]
    async fn openai_stream_concatenates_and_terminates() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(SSE_BODY).insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = OpenAiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gpt-4o".into(),
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
        assert_eq!(done, Some("stop".into()));
        assert_eq!(usage, Some((10, 5)));
    }

    #[tokio::test]
    async fn emits_done_from_captured_finish_reason_on_truncated_stream() {
        // Stream ends with finish_reason and usage frames but NO [DONE] terminator.
        // The finish_reason captured from the frame should be used (not "interrupted").
        const TRUNCATED: &str = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n";

        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(TRUNCATED)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&srv)
            .await;
        let p = OpenAiProvider::with_base_url("test".into(), srv.uri());
        let req = ChatRequest {
            model: "gpt-4o".into(),
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
        assert_eq!(text, "Hel");
        // finish_reason "stop" was captured from the frame — use it, not "interrupted".
        assert_eq!(done, Some("stop".into()));
        assert_eq!(usage, Some((10, 5)));
    }
}
