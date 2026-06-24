//! Test-support helpers. Gated behind `#[cfg(any(test, feature = "test-support"))]`
//! so consumers of this crate opt in via `features = ["test-support"]` in their
//! `[dev-dependencies]` block.

use agentforge_core::RuntimeCapability;
use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::provider::{ChatRequest, ChatResponse, LlmError, LlmProvider, LlmStream, StreamDelta, Usage};

/// Mock provider used by gateway tests and cross-crate consumers that want to
/// route a deterministic reply back through the real `LlmGateway` / factory.
pub struct MockProvider {
    name: String,
    response_content: String,
}

impl MockProvider {
    pub fn new(name: &str, content: &str) -> Self {
        Self { name: name.to_string(), response_content: content.to_string() }
    }
}

#[async_trait]
impl LlmStream for MockProvider {
    async fn stream(
        &self,
        _request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError> {
        let response = self.response_content.clone();
        let s = futures::stream::iter(vec![
            Ok(StreamDelta::Text(response)),
            Ok(StreamDelta::Usage { input_tokens: 0, output_tokens: 0 }),
            Ok(StreamDelta::Done { finish_reason: "stop".into() }),
        ]);
        Ok(Box::pin(s))
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capability_profile(&self) -> RuntimeCapability {
        RuntimeCapability::api_provider_or_default(self.name(), 8_192)
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: self.response_content.clone(),
            model: request.model,
            usage: Some(Usage { input_tokens: 10, output_tokens: 20 }),
        })
    }
}
