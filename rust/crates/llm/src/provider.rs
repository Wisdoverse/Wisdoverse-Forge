//! Core LLM provider trait and shared types.

use agentforge_core::RuntimeCapability;
use async_trait::async_trait;
use futures::stream::BoxStream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Max time to establish a TCP+TLS connection to a provider before failing.
pub(crate) const PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Max idle gap BETWEEN bytes on a provider response. This is a per-read
/// timeout, NOT a total-request deadline, so a long legitimate streaming
/// response is never cut — but a stalled or black-hole upstream cannot hang the
/// request (and its tokio task + DB/connection-pool resources) indefinitely.
/// Without it, `Client::new()` has no timeouts at all, so a single bad upstream
/// can exhaust the async runtime under load (F024).
pub(crate) const PROVIDER_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Build an outbound HTTP client with the given connect + read-idle timeouts.
/// Crate-internal so tests can exercise the deadline behavior with short values
/// against a stalled server.
pub(crate) fn client_with_timeouts(connect: Duration, read_idle: Duration) -> Client {
    Client::builder()
        .connect_timeout(connect)
        .read_timeout(read_idle)
        .build()
        // A builder failure means the TLS backend could not initialize; fall back
        // to a default client rather than panicking in a provider constructor.
        .unwrap_or_else(|_| Client::new())
}

/// The shared, timeout-bounded HTTP client every provider uses for both
/// non-streaming and streaming requests.
pub(crate) fn timed_client() -> Client {
    client_with_timeouts(PROVIDER_CONNECT_TIMEOUT, PROVIDER_READ_IDLE_TIMEOUT)
}

/// Content of a chat message: plain text, or an ordered list of blocks
/// (text + images) for multimodal input. Serializes untagged, so `Text` is a
/// bare JSON string (identical to the pre-multimodal wire format) and `Blocks`
/// is a JSON array — but providers render `Blocks` explicitly to their own
/// image shapes rather than relying on this neutral representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// One block of multimodal message content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    /// Base64-encoded image with its IANA media type (e.g. "image/png").
    Image {
        media_type: String,
        data: String,
    },
}

impl From<String> for MessageContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for MessageContent {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl MessageContent {
    /// Best-effort plain-text view: the text for `Text`, or the newline-joined
    /// text blocks for `Blocks` (images skipped). Used where a provider field
    /// must be a string, e.g. an Anthropic/Gemini system instruction.
    pub fn to_text_lossy(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    ContentBlock::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Image blocks in order, if any.
    pub fn images(&self) -> impl Iterator<Item = (&str, &str)> {
        let blocks: &[ContentBlock] = match self {
            Self::Blocks(blocks) => blocks,
            Self::Text(_) => &[],
        };
        blocks.iter().filter_map(|block| match block {
            ContentBlock::Image { media_type, data } => Some((media_type.as_str(), data.as_str())),
            ContentBlock::Text { .. } => None,
        })
    }
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
}

/// Request payload sent to any LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// Unified response from any LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Errors that can occur when interacting with LLM providers.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// One chunk of a streaming provider response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamDelta {
    /// Partial assistant text.
    Text(String),
    /// Final usage (tokens in/out) — emitted right before `Done`.
    Usage { input_tokens: u32, output_tokens: u32 },
    /// Stream ended cleanly.
    Done { finish_reason: String },
}

/// Streaming variant of `chat()`. Returns a stream of deltas.
///
/// The stream completes with a terminal `StreamDelta::Done { finish_reason }`
/// on success, or yields an `LlmError` on failure.
#[async_trait]
pub trait LlmStream: Send + Sync {
    async fn stream(&self, request: ChatRequest)
    -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError>;
}

/// Per-model context-window limits used to size the prompt history budget.
///
/// F029 design note: an unknown model falls back to 4_096 ON PURPOSE. This is a
/// per-MODEL table; a provider's `capability_profile().max_context_tokens` is a
/// per-PROVIDER ceiling, so it cannot stand in for a specific model's window
/// here (different models on the same provider differ). The fallback is
/// deliberately the SMALL side because the failure modes are asymmetric: an
/// under-estimate makes `PromptService` truncate a little extra history (safe,
/// the request still fits), while an over-estimate would let the request exceed
/// the real window and be rejected by the provider with a 400. So we never guess
/// a generous limit for an unrecognized model; add the model here when its real
/// window is known.
const MODEL_LIMITS: &[(&str, usize)] = &[
    ("claude-sonnet-4-6", 200_000),
    ("claude-opus-4-7", 200_000),
    ("gpt-4o", 128_000),
    ("gpt-4-turbo", 128_000),
    ("gemini-2.0-pro", 1_000_000),
    ("llama3.2", 8_192),
];

/// Context-window limit for `model`, or a conservative 4_096 fallback for an
/// unrecognized model (see `MODEL_LIMITS` for why the fallback is small).
pub fn model_context_limit(model: &str) -> usize {
    MODEL_LIMITS.iter().find(|(m, _)| *m == model).map(|(_, l)| *l).unwrap_or(4_096)
}

/// Trait implemented by each LLM provider backend.
#[async_trait]
pub trait LlmProvider: LlmStream + Send + Sync {
    /// Provider name (e.g., "anthropic", "openai", "ollama").
    fn name(&self) -> &str;

    /// Conservative provider-backed API capability profile.
    fn capability_profile(&self) -> RuntimeCapability {
        RuntimeCapability::api_default(self.name())
    }

    /// Send a chat completion request.
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
}

#[cfg(test)]
mod model_limit_tests {
    use super::*;
    #[test]
    fn known_model_returns_limit() {
        assert_eq!(model_context_limit("claude-sonnet-4-6"), 200_000);
    }
    #[test]
    fn unknown_model_fallback_4k() {
        assert_eq!(model_context_limit("mystery-model"), 4_096);
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;
    use std::time::Instant;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// F030 lock-in: the shared client's read-idle timeout must abort a stalled
    /// upstream instead of hanging. If a refactor drops `read_timeout`, this test
    /// hangs past its short deadline and fails.
    #[tokio::test]
    async fn read_idle_timeout_aborts_a_stalled_upstream() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
            .mount(&server)
            .await;

        // A 300ms read-idle deadline must fire well before the 10s stall.
        let client = client_with_timeouts(Duration::from_secs(2), Duration::from_millis(300));
        let start = Instant::now();
        let result = client.get(server.uri()).send().await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "a stalled upstream must produce a timeout error, got {result:?}");
        assert!(elapsed < Duration::from_secs(3), "must abort on the read deadline, took {elapsed:?}");
    }

    /// The production helper must construct successfully (exercises the real path
    /// providers use).
    #[test]
    fn timed_client_builds_with_production_timeouts() {
        let _ = timed_client();
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;

    #[test]
    fn text_content_serializes_as_bare_string() {
        // Backward-compat: text-only messages must keep the pre-multimodal wire
        // shape (a JSON string), not a tagged enum.
        let content = MessageContent::from("hello");
        assert_eq!(serde_json::to_value(&content).unwrap(), serde_json::json!("hello"));
    }

    #[test]
    fn from_str_and_string_produce_text() {
        assert_eq!(MessageContent::from("x"), MessageContent::Text("x".to_string()));
        assert_eq!(MessageContent::from("y".to_string()), MessageContent::Text("y".to_string()));
    }

    #[test]
    fn block_content_serializes_as_typed_array() {
        let content = MessageContent::Blocks(vec![
            ContentBlock::Text { text: "look".to_string() },
            ContentBlock::Image { media_type: "image/png".to_string(), data: "AAAA".to_string() },
        ]);
        let value = serde_json::to_value(&content).unwrap();
        assert_eq!(value[0], serde_json::json!({"type": "text", "text": "look"}));
        assert_eq!(value[1], serde_json::json!({"type": "image", "media_type": "image/png", "data": "AAAA"}));
    }

    #[test]
    fn to_text_lossy_joins_text_blocks_and_skips_images() {
        let content = MessageContent::Blocks(vec![
            ContentBlock::Text { text: "a".to_string() },
            ContentBlock::Image { media_type: "image/png".to_string(), data: "x".to_string() },
            ContentBlock::Text { text: "b".to_string() },
        ]);
        assert_eq!(content.to_text_lossy(), "a\nb");
        assert_eq!(MessageContent::from("solo").to_text_lossy(), "solo");
    }

    #[test]
    fn images_iterates_only_image_blocks() {
        let content = MessageContent::Blocks(vec![
            ContentBlock::Text { text: "t".to_string() },
            ContentBlock::Image { media_type: "image/png".to_string(), data: "D".to_string() },
        ]);
        let images: Vec<_> = content.images().collect();
        assert_eq!(images, vec![("image/png", "D")]);
        assert_eq!(MessageContent::from("t").images().count(), 0);
    }
}
