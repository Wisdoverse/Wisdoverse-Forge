//! LLM Gateway — routes requests to the appropriate provider.

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::BoxStream;

use crate::provider::{ChatRequest, ChatResponse, LlmError, LlmProvider, StreamDelta};

/// Central gateway that routes LLM requests to registered providers.
pub struct LlmGateway {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    default_provider: Option<String>,
}

impl LlmGateway {
    /// Create a new empty gateway.
    pub fn new() -> Self {
        Self { providers: HashMap::new(), default_provider: None }
    }

    /// Register a provider. The first registered provider becomes the default.
    pub fn register(&mut self, name: &str, provider: Arc<dyn LlmProvider>) {
        if self.default_provider.is_none() {
            self.default_provider = Some(name.to_string());
        }
        self.providers.insert(name.to_string(), provider);
    }

    /// Override the default provider.
    pub fn set_default(&mut self, name: &str) {
        self.default_provider = Some(name.to_string());
    }

    /// Route a chat request to the specified or default provider.
    pub async fn chat(&self, provider_name: Option<&str>, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let name = provider_name
            .or(self.default_provider.as_deref())
            .ok_or_else(|| LlmError::NotConfigured("no provider configured".into()))?;

        let provider = self
            .providers
            .get(name)
            .ok_or_else(|| LlmError::NotConfigured(format!("provider '{name}' not registered")))?;

        tracing::debug!(provider = name, model = %request.model, "Routing LLM request");
        provider.chat(request).await
    }

    /// Route a streaming chat request to the specified or default provider.
    pub async fn stream(
        &self,
        provider_name: Option<&str>,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamDelta, LlmError>>, LlmError> {
        let name = provider_name
            .or(self.default_provider.as_deref())
            .ok_or_else(|| LlmError::NotConfigured("no provider configured".into()))?;
        let provider = self
            .providers
            .get(name)
            .ok_or_else(|| LlmError::NotConfigured(format!("provider '{name}' not registered")))?;
        tracing::debug!(provider = name, model = %request.model, "Routing LLM stream request");
        provider.stream(request).await
    }

    /// List all registered provider names.
    pub fn available_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for LlmGateway {
    fn default() -> Self {
        Self::new()
    }
}
