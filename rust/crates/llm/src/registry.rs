//! Provider registry for the Rust-native LLM gateway.
//!
//! This is the single source of truth for provider keys, display metadata, and
//! transport shape. UI/API layers can expose the metadata, while the factory
//! uses the same registry to decide which adapter to construct.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransport {
    Anthropic,
    OpenAi,
    Gemini,
    Ollama,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderModel {
    pub model: &'static str,
    pub display_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSpec {
    pub key: &'static str,
    pub display_name: &'static str,
    pub transport: ProviderTransport,
    pub default_base_url: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub models: &'static [ProviderModel],
    pub requires_api_key: bool,
    pub allow_custom_models: bool,
}

const ANTHROPIC_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "claude-sonnet-4-20250514", display_name: "Claude Sonnet 4" },
    ProviderModel { model: "claude-sonnet-4-6", display_name: "Claude Sonnet 4.6" },
];

const OPENAI_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "gpt-5.5", display_name: "GPT-5.5" },
    ProviderModel { model: "gpt-5.4", display_name: "GPT-5.4" },
    ProviderModel { model: "gpt-4o", display_name: "GPT-4o" },
    ProviderModel { model: "gpt-4o-mini", display_name: "GPT-4o Mini" },
];

const GOOGLE_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "gemini-2.5-pro", display_name: "Gemini 2.5 Pro" },
    ProviderModel { model: "gemini-2.0-pro", display_name: "Gemini 2.0 Pro" },
];

const OLLAMA_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "llama3", display_name: "Llama 3" },
    ProviderModel { model: "llama3.2", display_name: "Llama 3.2" },
];

const GROQ_MODELS: &[ProviderModel] =
    &[ProviderModel { model: "llama-3.3-70b-versatile", display_name: "Llama 3.3 70B" }];

const DEEPSEEK_MODELS: &[ProviderModel] = &[ProviderModel { model: "deepseek-chat", display_name: "DeepSeek Chat" }];

const XAI_MODELS: &[ProviderModel] = &[ProviderModel { model: "grok-3-mini", display_name: "Grok 3 Mini" }];

const OPENROUTER_MODELS: &[ProviderModel] =
    &[ProviderModel { model: "openai/gpt-4o-mini", display_name: "OpenAI GPT-4o Mini" }];

const TOGETHER_MODELS: &[ProviderModel] = &[ProviderModel { model: "openai/gpt-oss-20b", display_name: "GPT OSS 20B" }];

const FIREWORKS_MODELS: &[ProviderModel] =
    &[ProviderModel { model: "accounts/fireworks/models/qwen3-30b-a3b", display_name: "Qwen3 30B A3B" }];

const LITELLM_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "gpt-4o-mini", display_name: "Gateway alias: gpt-4o-mini" },
    ProviderModel { model: "claude-sonnet-4-20250514", display_name: "Gateway alias: Claude Sonnet 4" },
];

pub const PROVIDER_SPECS: &[ProviderSpec] = &[
    ProviderSpec {
        key: "anthropic",
        display_name: "Anthropic",
        transport: ProviderTransport::Anthropic,
        default_base_url: None,
        default_model: Some("claude-sonnet-4-20250514"),
        models: ANTHROPIC_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "openai",
        display_name: "OpenAI",
        transport: ProviderTransport::OpenAi,
        default_base_url: None,
        default_model: Some("gpt-5.5"),
        models: OPENAI_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "google",
        display_name: "Google",
        transport: ProviderTransport::Gemini,
        default_base_url: None,
        default_model: Some("gemini-2.5-pro"),
        models: GOOGLE_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "ollama",
        display_name: "Ollama",
        transport: ProviderTransport::Ollama,
        default_base_url: None,
        default_model: Some("llama3"),
        models: OLLAMA_MODELS,
        requires_api_key: false,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "groq",
        display_name: "Groq",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.groq.com/openai"),
        default_model: Some("llama-3.3-70b-versatile"),
        models: GROQ_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "deepseek",
        display_name: "DeepSeek",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.deepseek.com"),
        default_model: Some("deepseek-chat"),
        models: DEEPSEEK_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "xai",
        display_name: "xAI",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.x.ai"),
        default_model: Some("grok-3-mini"),
        models: XAI_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "openrouter",
        display_name: "OpenRouter",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://openrouter.ai/api"),
        default_model: Some("openai/gpt-4o-mini"),
        models: OPENROUTER_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "together",
        display_name: "Together AI",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.together.xyz"),
        default_model: Some("openai/gpt-oss-20b"),
        models: TOGETHER_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "fireworks",
        display_name: "Fireworks AI",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.fireworks.ai/inference"),
        default_model: Some("accounts/fireworks/models/qwen3-30b-a3b"),
        models: FIREWORKS_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "litellm",
        display_name: "LiteLLM Gateway",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("http://litellm:4000"),
        default_model: Some("gpt-4o-mini"),
        models: LITELLM_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "openai_compatible",
        display_name: "OpenAI-Compatible",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: None,
        default_model: None,
        models: &[],
        requires_api_key: true,
        allow_custom_models: true,
    },
];

pub fn normalize_provider_key(provider: &str) -> String {
    let key = provider.trim().to_ascii_lowercase().replace('-', "_");
    match key.as_str() {
        "lite_llm" => "litellm".to_string(),
        "custom" | "custom_openai" | "openai_compatible" => "openai_compatible".to_string(),
        _ => key,
    }
}

pub fn supported_provider_specs() -> &'static [ProviderSpec] {
    PROVIDER_SPECS
}

pub fn provider_spec(provider: &str) -> Option<&'static ProviderSpec> {
    let key = normalize_provider_key(provider);
    PROVIDER_SPECS.iter().find(|spec| spec.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_litellm_and_generic_openai_compatible() {
        let keys: Vec<_> = supported_provider_specs().iter().map(|spec| spec.key).collect();

        assert!(keys.contains(&"litellm"));
        assert!(keys.contains(&"openai_compatible"));
    }

    #[test]
    fn aliases_normalize_to_stable_provider_keys() {
        assert_eq!(normalize_provider_key("Lite_LLM"), "litellm");
        assert_eq!(normalize_provider_key("openai-compatible"), "openai_compatible");
        assert_eq!(normalize_provider_key("custom"), "openai_compatible");
    }

    #[test]
    fn litellm_is_openai_compatible_with_default_proxy_url() {
        let spec = provider_spec("litellm").expect("litellm provider spec");

        assert_eq!(spec.transport, ProviderTransport::OpenAiCompatible);
        assert_eq!(spec.default_base_url, Some("http://litellm:4000"));
        assert!(spec.allow_custom_models);
    }
}
