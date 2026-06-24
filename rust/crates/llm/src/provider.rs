//! Core LLM provider trait and shared types.

use agentforge_core::RuntimeCapability;
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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

/// Static context-window limits. Unknown models fall back to 4_096 (intentionally
/// conservative — `PromptService::build_history` handles underflow safely).
const MODEL_LIMITS: &[(&str, usize)] = &[
    ("claude-sonnet-4-6", 200_000),
    ("claude-opus-4-7", 200_000),
    ("gpt-4o", 128_000),
    ("gpt-4-turbo", 128_000),
    ("gemini-2.0-pro", 1_000_000),
    ("llama3.2", 8_192),
];

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
