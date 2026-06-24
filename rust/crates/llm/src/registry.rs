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
    /// Alternate-region endpoint for providers whose `default_base_url` is a
    /// China-region host. Surfaced to the UI as a "Global endpoint" hint so an
    /// operator can switch regions by pasting one URL; the factory never uses
    /// it implicitly.
    pub global_base_url: Option<&'static str>,
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

const ZHIPU_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "glm-5.1", display_name: "GLM-5.1" },
    ProviderModel { model: "glm-4.7", display_name: "GLM-4.7" },
    ProviderModel { model: "glm-4.5-air", display_name: "GLM-4.5 Air" },
];

const MINIMAX_MODELS: &[ProviderModel] = &[ProviderModel { model: "MiniMax-M3", display_name: "MiniMax M3" }];

const MOONSHOT_MODELS: &[ProviderModel] = &[ProviderModel { model: "kimi-k2.5", display_name: "Kimi K2.5" }];

const DASHSCOPE_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "qwen3.5-plus", display_name: "Qwen3.5 Plus" },
    ProviderModel { model: "qwen3-max", display_name: "Qwen3 Max" },
    ProviderModel { model: "qwen3-coder-plus", display_name: "Qwen3 Coder Plus" },
];

const HUNYUAN_MODELS: &[ProviderModel] = &[
    ProviderModel { model: "hunyuan-turbo-latest", display_name: "Hunyuan Turbo" },
    ProviderModel { model: "hunyuan-t1-latest", display_name: "Hunyuan T1" },
];

const XIAOMI_MODELS: &[ProviderModel] = &[ProviderModel { model: "mimo-v2.5-pro", display_name: "MiMo V2.5 Pro" }];

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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
        default_model: Some("deepseek-chat"),
        models: DEEPSEEK_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Zhipu GLM pay-as-you-go API — docs.z.ai (global) / open.bigmodel.cn (CN).
    ProviderSpec {
        key: "zhipu",
        display_name: "Zhipu GLM",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://open.bigmodel.cn/api/paas/v4"),
        global_base_url: Some("https://api.z.ai/api/paas/v4"),
        default_model: Some("glm-4.7"),
        models: ZHIPU_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Zhipu GLM Coding Plan Anthropic-compatible endpoint — docs.z.ai.
    ProviderSpec {
        key: "zhipu_coding",
        display_name: "Zhipu GLM Coding Plan",
        transport: ProviderTransport::Anthropic,
        default_base_url: Some("https://open.bigmodel.cn/api/anthropic"),
        global_base_url: Some("https://api.z.ai/api/anthropic"),
        default_model: Some("glm-4.7"),
        models: ZHIPU_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // MiniMax pay-as-you-go API — platform.minimax.io.
    ProviderSpec {
        key: "minimax",
        display_name: "MiniMax",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.minimaxi.com/v1"),
        global_base_url: Some("https://api.minimax.io/v1"),
        default_model: Some("MiniMax-M3"),
        models: MINIMAX_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // MiniMax Coding Plan Anthropic-compatible endpoint — platform.minimax.io.
    ProviderSpec {
        key: "minimax_coding",
        display_name: "MiniMax Coding Plan",
        transport: ProviderTransport::Anthropic,
        default_base_url: Some("https://api.minimaxi.com/anthropic"),
        global_base_url: Some("https://api.minimax.io/anthropic"),
        default_model: Some("MiniMax-M3"),
        models: MINIMAX_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Moonshot Kimi pay-as-you-go API — platform.moonshot.cn / platform.kimi.ai.
    ProviderSpec {
        key: "moonshot",
        display_name: "Moonshot Kimi",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.moonshot.cn/v1"),
        global_base_url: Some("https://api.moonshot.ai/v1"),
        default_model: Some("kimi-k2.5"),
        models: MOONSHOT_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Moonshot Kimi Coding Plan Anthropic-compatible endpoint — platform.kimi.ai.
    ProviderSpec {
        key: "moonshot_coding",
        display_name: "Moonshot Kimi Coding Plan",
        transport: ProviderTransport::Anthropic,
        default_base_url: Some("https://api.moonshot.cn/anthropic"),
        global_base_url: Some("https://api.moonshot.ai/anthropic"),
        default_model: Some("kimi-k2.5"),
        models: MOONSHOT_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Alibaba Qwen DashScope OpenAI-compatible API — alibabacloud.com Model Studio.
    ProviderSpec {
        key: "dashscope",
        display_name: "Alibaba Qwen (DashScope)",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        global_base_url: Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
        default_model: Some("qwen3-coder-plus"),
        models: DASHSCOPE_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Alibaba Qwen Coding Plan Anthropic-compatible endpoint — alibabacloud.com Model Studio.
    ProviderSpec {
        key: "dashscope_coding",
        display_name: "Alibaba Qwen Coding Plan",
        transport: ProviderTransport::Anthropic,
        default_base_url: Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
        global_base_url: Some("https://coding-intl.dashscope.aliyuncs.com/apps/anthropic"),
        default_model: Some("qwen3-coder-plus"),
        models: DASHSCOPE_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Tencent Hunyuan OpenAI-compatible API (CN only, no Anthropic-compatible
    // coding endpoint) — cloud.tencent.com Hunyuan docs.
    ProviderSpec {
        key: "hunyuan",
        display_name: "Tencent Hunyuan",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.hunyuan.cloud.tencent.com/v1"),
        global_base_url: None,
        default_model: Some("hunyuan-turbo-latest"),
        models: HUNYUAN_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Xiaomi MiMo OpenAI-compatible API (one host serves all regions) —
    // platform.xiaomimimo.com.
    ProviderSpec {
        key: "xiaomi",
        display_name: "Xiaomi MiMo",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.xiaomimimo.com/v1"),
        global_base_url: None,
        default_model: Some("mimo-v2.5-pro"),
        models: XIAOMI_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    // Xiaomi MiMo Coding Plan Anthropic-compatible endpoint (Token Plan
    // subscribers get a dedicated host from their console) — platform.xiaomimimo.com.
    ProviderSpec {
        key: "xiaomi_coding",
        display_name: "Xiaomi MiMo Coding Plan",
        transport: ProviderTransport::Anthropic,
        default_base_url: Some("https://api.xiaomimimo.com/anthropic"),
        global_base_url: None,
        default_model: Some("mimo-v2.5-pro"),
        models: XIAOMI_MODELS,
        requires_api_key: true,
        allow_custom_models: true,
    },
    ProviderSpec {
        key: "xai",
        display_name: "xAI",
        transport: ProviderTransport::OpenAiCompatible,
        default_base_url: Some("https://api.x.ai"),
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        global_base_url: None,
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
        // Mainstream China-region vendor aliases: brand and platform names map
        // to the registry key; `_coding`-suffixed forms map to the matching
        // Coding Plan key. Hunyuan has no coding endpoint, so no
        // `tencent_coding` alias exists.
        "glm" | "bigmodel" | "z_ai" | "zai" => "zhipu".to_string(),
        "glm_coding" | "bigmodel_coding" | "z_ai_coding" | "zai_coding" => "zhipu_coding".to_string(),
        "kimi" => "moonshot".to_string(),
        "kimi_coding" => "moonshot_coding".to_string(),
        "qwen" | "alibaba" | "aliyun" => "dashscope".to_string(),
        "qwen_coding" | "alibaba_coding" | "aliyun_coding" => "dashscope_coding".to_string(),
        "mimo" => "xiaomi".to_string(),
        "mimo_coding" => "xiaomi_coding".to_string(),
        "tencent" => "hunyuan".to_string(),
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

    const CN_PROVIDER_KEYS: &[&str] = &[
        "zhipu",
        "zhipu_coding",
        "minimax",
        "minimax_coding",
        "moonshot",
        "moonshot_coding",
        "dashscope",
        "dashscope_coding",
        "hunyuan",
        "xiaomi",
        "xiaomi_coding",
    ];

    #[test]
    fn registry_resolves_all_mainstream_cn_providers() {
        for key in CN_PROVIDER_KEYS {
            let spec = provider_spec(key).unwrap_or_else(|| panic!("missing provider spec for '{key}'"));
            assert_eq!(spec.key, *key);
        }
    }

    #[test]
    fn cn_provider_specs_have_cn_default_endpoint_model_and_key_policy() {
        for key in CN_PROVIDER_KEYS {
            let spec = provider_spec(key).unwrap_or_else(|| panic!("missing provider spec for '{key}'"));
            assert!(spec.default_base_url.is_some(), "'{key}' must default to its CN endpoint");
            assert!(spec.default_model.is_some(), "'{key}' must seed a default model");
            assert!(!spec.models.is_empty(), "'{key}' must list at least one known model");
            assert!(spec.requires_api_key, "'{key}' must require an API key");
            assert!(spec.allow_custom_models, "'{key}' must allow custom models");
        }
    }

    #[test]
    fn cn_coding_plan_specs_use_anthropic_transport_and_others_openai_compatible() {
        for key in CN_PROVIDER_KEYS {
            let spec = provider_spec(key).unwrap_or_else(|| panic!("missing provider spec for '{key}'"));
            if key.ends_with("_coding") {
                assert_eq!(spec.transport, ProviderTransport::Anthropic, "'{key}' must use the Anthropic transport");
            } else {
                assert_eq!(
                    spec.transport,
                    ProviderTransport::OpenAiCompatible,
                    "'{key}' must use the OpenAI-compatible transport"
                );
            }
        }
    }

    #[test]
    fn cn_default_endpoints_match_vendor_docs() {
        let expected = [
            ("zhipu", "https://open.bigmodel.cn/api/paas/v4", Some("https://api.z.ai/api/paas/v4")),
            ("zhipu_coding", "https://open.bigmodel.cn/api/anthropic", Some("https://api.z.ai/api/anthropic")),
            ("minimax", "https://api.minimaxi.com/v1", Some("https://api.minimax.io/v1")),
            ("minimax_coding", "https://api.minimaxi.com/anthropic", Some("https://api.minimax.io/anthropic")),
            ("moonshot", "https://api.moonshot.cn/v1", Some("https://api.moonshot.ai/v1")),
            ("moonshot_coding", "https://api.moonshot.cn/anthropic", Some("https://api.moonshot.ai/anthropic")),
            (
                "dashscope",
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
            ),
            (
                "dashscope_coding",
                "https://coding.dashscope.aliyuncs.com/apps/anthropic",
                Some("https://coding-intl.dashscope.aliyuncs.com/apps/anthropic"),
            ),
            ("hunyuan", "https://api.hunyuan.cloud.tencent.com/v1", None),
            ("xiaomi", "https://api.xiaomimimo.com/v1", None),
            ("xiaomi_coding", "https://api.xiaomimimo.com/anthropic", None),
        ];

        for (key, cn_url, global_url) in expected {
            let spec = provider_spec(key).unwrap_or_else(|| panic!("missing provider spec for '{key}'"));
            assert_eq!(spec.default_base_url, Some(cn_url), "'{key}' CN default endpoint");
            assert_eq!(spec.global_base_url, global_url, "'{key}' global endpoint hint");
        }
    }

    #[test]
    fn cn_vendor_aliases_normalize_to_registry_keys() {
        assert_eq!(normalize_provider_key("glm"), "zhipu");
        assert_eq!(normalize_provider_key("bigmodel"), "zhipu");
        assert_eq!(normalize_provider_key("z-ai"), "zhipu");
        assert_eq!(normalize_provider_key("ZAI"), "zhipu");
        assert_eq!(normalize_provider_key("kimi"), "moonshot");
        assert_eq!(normalize_provider_key("qwen"), "dashscope");
        assert_eq!(normalize_provider_key("alibaba"), "dashscope");
        assert_eq!(normalize_provider_key("aliyun"), "dashscope");
        assert_eq!(normalize_provider_key("mimo"), "xiaomi");
        assert_eq!(normalize_provider_key("tencent"), "hunyuan");
    }

    #[test]
    fn cn_coding_suffixed_aliases_normalize_to_coding_plan_keys() {
        assert_eq!(normalize_provider_key("glm_coding"), "zhipu_coding");
        assert_eq!(normalize_provider_key("bigmodel_coding"), "zhipu_coding");
        assert_eq!(normalize_provider_key("z-ai-coding"), "zhipu_coding");
        assert_eq!(normalize_provider_key("zai_coding"), "zhipu_coding");
        assert_eq!(normalize_provider_key("kimi_coding"), "moonshot_coding");
        assert_eq!(normalize_provider_key("qwen-coding"), "dashscope_coding");
        assert_eq!(normalize_provider_key("alibaba_coding"), "dashscope_coding");
        assert_eq!(normalize_provider_key("aliyun_coding"), "dashscope_coding");
        assert_eq!(normalize_provider_key("mimo_coding"), "xiaomi_coding");
    }

    #[test]
    fn hunyuan_has_no_coding_plan_entry() {
        assert!(provider_spec("hunyuan_coding").is_none());
        assert!(provider_spec("tencent_coding").is_none());
    }

    #[test]
    fn anthropic_spec_keeps_no_default_base_url() {
        // The Anthropic adapter falls back to api.anthropic.com when the spec
        // and per-config base_url are both absent; the CN coding-plan entries
        // must not change that.
        let spec = provider_spec("anthropic").expect("anthropic provider spec");
        assert_eq!(spec.default_base_url, None);
        assert_eq!(spec.global_base_url, None);
    }
}
