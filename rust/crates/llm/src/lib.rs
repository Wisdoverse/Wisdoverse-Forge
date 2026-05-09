//! Wisdoverse Forge LLM — multi-provider LLM gateway.
//!
//! Abstracts communication with Anthropic, OpenAI, and Ollama APIs
//! behind a unified [`LlmProvider`] trait. The [`LlmGateway`] routes
//! requests to the appropriate provider by name.

pub mod anthropic;
pub mod gateway;
pub mod gemini;
pub mod openai;
pub mod provider;
pub mod sse_framer;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

#[cfg(any(test, feature = "test-support"))]
pub use testing::MockProvider;

pub use anthropic::AnthropicProvider;
pub use gateway::LlmGateway;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;
pub use provider::{
    ChatMessage, ChatRequest, ChatResponse, LlmError, LlmProvider, LlmStream, StreamDelta, Usage, model_context_limit,
};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LlmProviderBuildConfig {
    pub provider_key: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

/// Stateless factory that constructs per-request `LlmProvider` instances using
/// a caller-supplied API key. The factory itself holds no secrets —
/// `PromptService` resolves the user's credential via `UserLlmConfigRepository`
/// and passes it in, then drops the provider instance after the stream ends.
pub struct LlmProviderFactory {
    ollama_base_url: Option<String>,
    #[cfg(any(test, feature = "test-support"))]
    mock: Option<(String, String)>,
}

impl LlmProviderFactory {
    pub fn new(ollama_base_url: Option<String>) -> Self {
        Self {
            ollama_base_url,
            #[cfg(any(test, feature = "test-support"))]
            mock: None,
        }
    }

    /// Test-only: yield a single `MockProvider(reply)` for any key equal to
    /// `name`. For any `provider_key != name`, `build` falls through to the
    /// real match arms, which lets tests exercise a mock+real provider mix.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_mock(name: &str, reply: &str) -> Self {
        Self { ollama_base_url: None, mock: Some((name.to_string(), reply.to_string())) }
    }

    /// Build a provider instance for `provider_key` using the caller's
    /// decrypted API key. Ollama ignores the key argument since it's local /
    /// keyless.
    pub fn build(&self, provider_key: &str, api_key: String) -> Result<Arc<dyn LlmProvider>, LlmError> {
        self.build_with_config(LlmProviderBuildConfig {
            provider_key: provider_key.to_string(),
            api_key,
            base_url: None,
        })
    }

    pub fn build_with_config(&self, config: LlmProviderBuildConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some((name, reply)) = &self.mock
            && config.provider_key == *name
        {
            return Ok(Arc::new(MockProvider::new(name, reply)));
        }
        match config.provider_key.as_str() {
            "anthropic" => Ok(Arc::new(anthropic::AnthropicProvider::new(config.api_key, config.base_url))),
            "openai" => Ok(Arc::new(openai::OpenAiProvider::new(config.api_key, config.base_url))),
            "google" => Ok(Arc::new(match config.base_url {
                Some(base_url) => gemini::GeminiProvider::with_base_url(config.api_key, base_url),
                None => gemini::GeminiProvider::new(config.api_key),
            })),
            "ollama" => {
                let url = config.base_url.or_else(|| self.ollama_base_url.clone()).ok_or_else(|| {
                    LlmError::NotConfigured("OLLAMA_BASE_URL env var required for ollama provider".into())
                })?;
                Ok(Arc::new(openai::OpenAiProvider::ollama(url)))
            }
            provider if is_openai_compatible_provider(provider) => {
                let base_url = config
                    .base_url
                    .or_else(|| default_openai_compatible_base_url(provider).map(str::to_string))
                    .ok_or_else(|| {
                        LlmError::NotConfigured(format!(
                            "provider '{provider}' requires a base_url compatible with OpenAI Chat Completions"
                        ))
                    })?;
                Ok(Arc::new(openai::OpenAiProvider::compatible(provider, config.api_key, base_url)))
            }
            other => Err(LlmError::NotConfigured(format!("provider '{other}' not supported"))),
        }
    }
}

fn is_openai_compatible_provider(provider: &str) -> bool {
    matches!(provider, "groq" | "deepseek" | "xai" | "openrouter" | "together" | "fireworks")
}

fn default_openai_compatible_base_url(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("https://api.groq.com/openai"),
        "deepseek" => Some("https://api.deepseek.com"),
        "xai" => Some("https://api.x.ai"),
        "openrouter" => Some("https://openrouter.ai/api"),
        "together" => Some("https://api.together.xyz"),
        "fireworks" => Some("https://api.fireworks.ai/inference"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockProvider;

    #[test]
    fn chat_request_serialization() {
        let req = ChatRequest {
            model: "claude-3-opus".to_string(),
            messages: vec![ChatMessage { role: "user".to_string(), content: "Hello".to_string() }],
            max_tokens: Some(1024),
            temperature: Some(0.7),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "claude-3-opus");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["max_tokens"], 1024);
    }

    #[test]
    fn chat_response_serialization() {
        let resp = ChatResponse {
            content: "Hello!".to_string(),
            model: "gpt-4".to_string(),
            usage: Some(Usage { input_tokens: 5, output_tokens: 10 }),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "Hello!");
        assert_eq!(parsed.usage.unwrap().input_tokens, 5);
    }

    #[test]
    fn chat_request_omits_none_fields() {
        let req = ChatRequest { model: "test".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("max_tokens").is_none());
        assert!(json.get("temperature").is_none());
    }

    #[tokio::test]
    async fn gateway_routes_to_default_provider() {
        let mut gw = LlmGateway::new();
        gw.register("mock", Arc::new(MockProvider::new("mock", "mock response")));

        let req =
            ChatRequest { model: "test-model".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let resp = gw.chat(None, req).await.unwrap();
        assert_eq!(resp.content, "mock response");
        assert_eq!(resp.model, "test-model");
    }

    #[tokio::test]
    async fn gateway_routes_to_named_provider() {
        let mut gw = LlmGateway::new();
        gw.register("alpha", Arc::new(MockProvider::new("alpha", "alpha response")));
        gw.register("beta", Arc::new(MockProvider::new("beta", "beta response")));

        let req = ChatRequest { model: "m".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let resp = gw.chat(Some("beta"), req).await.unwrap();
        assert_eq!(resp.content, "beta response");
    }

    #[tokio::test]
    async fn gateway_error_on_unknown_provider() {
        let gw = LlmGateway::new();
        let req = ChatRequest { model: "m".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let err = gw.chat(Some("nonexistent"), req).await.unwrap_err();
        assert!(matches!(err, LlmError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn gateway_error_when_no_providers() {
        let gw = LlmGateway::new();
        let req = ChatRequest { model: "m".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let err = gw.chat(None, req).await.unwrap_err();
        assert!(matches!(err, LlmError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn gateway_set_default_overrides() {
        let mut gw = LlmGateway::new();
        gw.register("first", Arc::new(MockProvider::new("first", "first response")));
        gw.register("second", Arc::new(MockProvider::new("second", "second response")));
        gw.set_default("second");

        let req = ChatRequest { model: "m".to_string(), messages: vec![], max_tokens: None, temperature: None };

        let resp = gw.chat(None, req).await.unwrap();
        assert_eq!(resp.content, "second response");
    }

    #[test]
    fn gateway_available_providers() {
        let mut gw = LlmGateway::new();
        gw.register("a", Arc::new(MockProvider::new("a", "")));
        gw.register("b", Arc::new(MockProvider::new("b", "")));

        let mut providers = gw.available_providers();
        providers.sort();
        assert_eq!(providers, vec!["a", "b"]);
    }

    #[test]
    fn anthropic_provider_construction() {
        let p = AnthropicProvider::new("sk-test".to_string(), None);
        assert_eq!(p.name(), "anthropic");
    }

    #[test]
    fn anthropic_provider_custom_base_url() {
        let p = AnthropicProvider::new("sk-test".to_string(), Some("https://custom.api.com".to_string()));
        assert_eq!(p.name(), "anthropic");
    }

    #[test]
    fn openai_provider_construction() {
        let p = OpenAiProvider::new("sk-test".to_string(), None);
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn openai_provider_custom_base_url() {
        let p = OpenAiProvider::new("sk-test".to_string(), Some("https://custom.openai.com".to_string()));
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn openai_ollama_factory() {
        let p = OpenAiProvider::ollama("http://localhost:11434".to_string());
        assert_eq!(p.name(), "ollama");
    }

    #[tokio::test]
    async fn gateway_stream_dispatches_by_provider_name() {
        use futures::StreamExt;
        let mut gw = LlmGateway::new();
        gw.register("mock", Arc::new(MockProvider::new("mock", "abc")));
        let req = ChatRequest {
            model: "x".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            max_tokens: None,
            temperature: None,
        };
        let mut s = gw.stream(Some("mock"), req).await.unwrap();
        let mut text = String::new();
        while let Some(d) = s.next().await {
            if let StreamDelta::Text(t) = d.unwrap() {
                text.push_str(&t);
            }
        }
        assert_eq!(text, "abc");
    }

    #[test]
    fn factory_builds_anthropic() {
        let f = LlmProviderFactory::new(None);
        let p = f.build("anthropic", "sk-test".into()).unwrap();
        assert_eq!(p.name(), "anthropic");
    }

    #[test]
    fn factory_builds_openai() {
        let f = LlmProviderFactory::new(None);
        let p = f.build("openai", "sk-test".into()).unwrap();
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn provider_default_capability_profile_is_conservative_api_runtime() {
        let f = LlmProviderFactory::new(None);
        let p = f.build("openai", "sk-test".into()).unwrap();
        let profile = p.capability_profile();

        assert_eq!(profile.provider_name.as_deref(), Some("openai"));
        assert_eq!(profile.runtime_kind, agentforge_core::RuntimeKind::Api);
        assert_eq!(profile.cli_tool, None);
        assert!(profile.max_context_tokens > 0);
        assert!(!profile.supports_terminal);
        assert!(!profile.supports_mcp_bridge);
        assert!(!profile.supports_hooks);
    }

    #[test]
    fn provider_capability_profiles_use_provider_context_limits() {
        let f = LlmProviderFactory::new(Some("http://localhost:11434".into()));

        let cases = [
            ("anthropic", "sk-test", 200_000),
            ("openai", "sk-test", 128_000),
            ("google", "goog-test", 1_000_000),
            ("ollama", "", 8_192),
            ("groq", "gsk-test", 128_000),
        ];

        for (provider_key, api_key, expected_context_tokens) in cases {
            let p = f.build(provider_key, api_key.to_string()).unwrap();
            let profile = p.capability_profile();

            assert_eq!(profile.provider_name.as_deref(), Some(provider_key));
            assert_eq!(profile.runtime_kind, agentforge_core::RuntimeKind::Api);
            assert_eq!(profile.max_context_tokens, expected_context_tokens);
            assert_eq!(profile.cli_tool, None);
            assert!(!profile.supports_skills_mount);
            assert!(!profile.supports_terminal);
        }

        let mock = LlmProviderFactory::with_mock("mocky", "hi").build("mocky", String::new()).unwrap();
        let profile = mock.capability_profile();
        assert_eq!(profile.provider_name.as_deref(), Some("mocky"));
        assert_eq!(profile.max_context_tokens, 8_192);
    }

    #[test]
    fn factory_builds_google() {
        let f = LlmProviderFactory::new(None);
        let p = f.build("google", "goog-test".into()).unwrap();
        assert_eq!(p.name(), "google");
    }

    #[test]
    fn factory_builds_ollama_with_base_url() {
        let f = LlmProviderFactory::new(Some("http://localhost:11434".into()));
        let p = f.build("ollama", String::new()).unwrap();
        assert_eq!(p.name(), "ollama");
    }

    #[test]
    fn factory_builds_openai_compatible_provider() {
        let f = LlmProviderFactory::new(None);
        let p = f.build("groq", "gsk-test".into()).unwrap();
        assert_eq!(p.name(), "groq");
    }

    #[test]
    fn factory_builds_openai_compatible_provider_with_custom_base_url() {
        let f = LlmProviderFactory::new(None);
        let p = f
            .build_with_config(LlmProviderBuildConfig {
                provider_key: "openrouter".into(),
                api_key: "sk-or".into(),
                base_url: Some("https://example.test/openrouter".into()),
            })
            .unwrap();
        assert_eq!(p.name(), "openrouter");
    }

    #[test]
    fn factory_ollama_without_base_url_errors() {
        let f = LlmProviderFactory::new(None);
        let err = f.build("ollama", String::new()).err().expect("expected Err");
        assert!(matches!(err, LlmError::NotConfigured(_)));
        assert!(format!("{err}").contains("OLLAMA_BASE_URL"));
    }

    #[test]
    fn factory_unknown_provider_errors() {
        let f = LlmProviderFactory::new(None);
        let err = f.build("mystery", "k".into()).err().expect("expected Err");
        assert!(matches!(err, LlmError::NotConfigured(_)));
        assert!(format!("{err}").contains("'mystery'"));
    }

    #[test]
    fn factory_with_mock_routes_to_mock_provider() {
        let f = LlmProviderFactory::with_mock("mocky", "hi");
        let p = f.build("mocky", String::new()).unwrap();
        assert_eq!(p.name(), "mocky");
    }
}
