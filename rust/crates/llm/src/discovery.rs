//! Live model discovery for the LLM gateway.
//!
//! Each provider exposes a "list models" endpoint. Discovery calls it with the
//! caller's API key, parses the lineup into [`DiscoveredModel`]s, and returns
//! them so the UI can offer an always-current model list instead of a curated
//! static one. The curated [`crate::registry`] list stays the fallback when a
//! provider is unreachable, keyless, or returns nothing.
//!
//! This module is intentionally transport-shaped and side-effect-light: the
//! caller owns the `reqwest::Client`, SSRF/host validation, caching, and
//! credential decryption. Parsing is split into pure functions so the bulk of
//! the behavior is testable without a network.

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use crate::registry::ProviderTransport;

/// A single model offered by a provider, as discovered live.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DiscoveredModel {
    pub model: String,
    pub display_name: String,
}

impl DiscoveredModel {
    fn new(model: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self { model: model.into(), display_name: display_name.into() }
    }
}

/// Why a discovery attempt did not yield a live list. Callers map this to a
/// graceful fallback (curated registry models), never a hard user error.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("model discovery request failed: {0}")]
    Request(String),
    #[error("model discovery endpoint returned HTTP {0}")]
    Status(u16),
    #[error("model discovery response could not be parsed: {0}")]
    Decode(String),
}

/// Default discovery timeout. Discovery is interactive (an operator is waiting
/// on the Add-service form), so fail fast and fall back to curated models.
pub const DEFAULT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);

/// The discovery base URL to use when the provider config carries no override.
/// Mirrors each adapter's own default so discovery and inference agree on the
/// endpoint. `OpenAiCompatible` has no universal default — the caller must
/// supply the vendor base URL (same contract as the factory).
pub fn default_discovery_base(transport: ProviderTransport) -> Option<&'static str> {
    match transport {
        ProviderTransport::Anthropic => Some("https://api.anthropic.com"),
        ProviderTransport::OpenAi => Some("https://api.openai.com"),
        ProviderTransport::Gemini => Some("https://generativelanguage.googleapis.com"),
        ProviderTransport::Ollama => None,
        ProviderTransport::OpenAiCompatible => None,
    }
}

/// Build the "list models" URL for a transport + base URL.
///
/// OpenAI-style bases may be a service root (`…/openai`) or already versioned
/// (`…/v1`, `…/api/paas/v4`); we append `/models` only when the version segment
/// is already present, matching the chat-completions URL logic so a pasted
/// vendor base never doubles its version segment.
pub fn models_url(transport: ProviderTransport, base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match transport {
        ProviderTransport::Anthropic => format!("{trimmed}/v1/models"),
        ProviderTransport::Gemini => format!("{trimmed}/v1beta/models"),
        ProviderTransport::Ollama => format!("{trimmed}/api/tags"),
        ProviderTransport::OpenAi | ProviderTransport::OpenAiCompatible => {
            if is_versioned(trimmed) {
                format!("{trimmed}/models")
            } else {
                format!("{trimmed}/v1/models")
            }
        }
    }
}

/// Whether a base URL's last path segment is a version marker like `v1` or `v4`.
fn is_versioned(trimmed: &str) -> bool {
    trimmed.rsplit('/').next().is_some_and(|segment| {
        segment.len() >= 2 && segment.starts_with('v') && segment[1..].chars().all(|c| c.is_ascii_digit())
    })
}

/// Discover models for a provider over HTTP.
///
/// `base_url` must already be resolved (config override or
/// [`default_discovery_base`]) and host-validated by the caller. `api_key` is
/// required for every keyed transport; Ollama ignores it.
pub async fn discover_models(
    client: &Client,
    transport: ProviderTransport,
    base_url: &str,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<Vec<DiscoveredModel>, DiscoveryError> {
    let url = models_url(transport, base_url);
    let mut request = client.get(&url).timeout(timeout);

    request = match transport {
        ProviderTransport::Anthropic => {
            request.header("x-api-key", api_key.unwrap_or_default()).header("anthropic-version", "2023-06-01")
        }
        ProviderTransport::OpenAi | ProviderTransport::OpenAiCompatible => {
            request.header("authorization", format!("Bearer {}", api_key.unwrap_or_default()))
        }
        // Gemini authenticates with a query-string key, not a header.
        ProviderTransport::Gemini => request.query(&[("key", api_key.unwrap_or_default())]),
        ProviderTransport::Ollama => request,
    };

    let response = request.send().await.map_err(|err| DiscoveryError::Request(err.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DiscoveryError::Status(status.as_u16()));
    }

    let body = response.text().await.map_err(|err| DiscoveryError::Request(err.to_string()))?;
    let json: Value = serde_json::from_str(&body).map_err(|err| DiscoveryError::Decode(err.to_string()))?;

    let models = match transport {
        ProviderTransport::Anthropic => parse_anthropic_models(&json),
        ProviderTransport::OpenAi | ProviderTransport::OpenAiCompatible => parse_openai_models(&json),
        ProviderTransport::Gemini => parse_gemini_models(&json),
        ProviderTransport::Ollama => parse_ollama_models(&json),
    };
    Ok(models)
}

/// OpenAI `/v1/models`: `{ "data": [ { "id": "gpt-4o" }, … ] }`.
pub fn parse_openai_models(json: &Value) -> Vec<DiscoveredModel> {
    json.get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .filter(|id| !id.is_empty())
                .map(|id| DiscoveredModel::new(id, id))
                .collect()
        })
        .unwrap_or_default()
}

/// Anthropic `/v1/models`: `{ "data": [ { "id": …, "display_name": … }, … ] }`.
pub fn parse_anthropic_models(json: &Value) -> Vec<DiscoveredModel> {
    json.get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?;
                    if id.is_empty() {
                        return None;
                    }
                    let display = item.get("display_name").and_then(Value::as_str).filter(|s| !s.is_empty());
                    Some(DiscoveredModel::new(id, display.unwrap_or(id)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Gemini `/v1beta/models`: `{ "models": [ { "name": "models/gemini-2.5-pro",
/// "displayName": …, "supportedGenerationMethods": [ … ] }, … ] }`. Keeps only
/// models that support `generateContent` and strips the `models/` name prefix.
pub fn parse_gemini_models(json: &Value) -> Vec<DiscoveredModel> {
    json.get("models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name").and_then(Value::as_str)?;
                    let supports_generate = item
                        .get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .map(|methods| methods.iter().any(|m| m.as_str() == Some("generateContent")))
                        // Absent field: don't exclude — be permissive.
                        .unwrap_or(true);
                    if !supports_generate {
                        return None;
                    }
                    let model = name.strip_prefix("models/").unwrap_or(name);
                    if model.is_empty() {
                        return None;
                    }
                    let display = item.get("displayName").and_then(Value::as_str).filter(|s| !s.is_empty());
                    Some(DiscoveredModel::new(model, display.unwrap_or(model)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Ollama `/api/tags`: `{ "models": [ { "name": "llama3:latest" }, … ] }`.
pub fn parse_ollama_models(json: &Value) -> Vec<DiscoveredModel> {
    json.get("models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .filter(|name| !name.is_empty())
                .map(|name| DiscoveredModel::new(name, name))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn models_url_appends_version_for_service_root() {
        assert_eq!(models_url(ProviderTransport::OpenAi, "https://api.openai.com"), "https://api.openai.com/v1/models");
        assert_eq!(
            models_url(ProviderTransport::OpenAiCompatible, "https://api.groq.com/openai"),
            "https://api.groq.com/openai/v1/models"
        );
    }

    #[test]
    fn models_url_does_not_double_existing_version() {
        assert_eq!(
            models_url(ProviderTransport::OpenAiCompatible, "https://api.minimaxi.com/v1"),
            "https://api.minimaxi.com/v1/models"
        );
        assert_eq!(
            models_url(ProviderTransport::OpenAiCompatible, "https://open.bigmodel.cn/api/paas/v4"),
            "https://open.bigmodel.cn/api/paas/v4/models"
        );
    }

    #[test]
    fn models_url_per_transport_paths() {
        assert_eq!(
            models_url(ProviderTransport::Anthropic, "https://api.anthropic.com"),
            "https://api.anthropic.com/v1/models"
        );
        assert_eq!(
            models_url(ProviderTransport::Gemini, "https://generativelanguage.googleapis.com"),
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        assert_eq!(models_url(ProviderTransport::Ollama, "http://localhost:11434"), "http://localhost:11434/api/tags");
    }

    #[test]
    fn parse_openai_skips_empty_and_missing_ids() {
        let json = json!({ "data": [ { "id": "gpt-4o" }, { "id": "" }, { "object": "model" } ] });
        assert_eq!(parse_openai_models(&json), vec![DiscoveredModel::new("gpt-4o", "gpt-4o")]);
    }

    #[test]
    fn parse_anthropic_prefers_display_name() {
        let json = json!({ "data": [
            { "id": "claude-sonnet-4-6", "display_name": "Claude Sonnet 4.6" },
            { "id": "claude-haiku-4-5" }
        ] });
        assert_eq!(
            parse_anthropic_models(&json),
            vec![
                DiscoveredModel::new("claude-sonnet-4-6", "Claude Sonnet 4.6"),
                DiscoveredModel::new("claude-haiku-4-5", "claude-haiku-4-5"),
            ]
        );
    }

    #[test]
    fn parse_gemini_strips_prefix_and_filters_generate_content() {
        let json = json!({ "models": [
            { "name": "models/gemini-2.5-pro", "displayName": "Gemini 2.5 Pro", "supportedGenerationMethods": ["generateContent"] },
            { "name": "models/embedding-001", "displayName": "Embedding", "supportedGenerationMethods": ["embedContent"] }
        ] });
        assert_eq!(parse_gemini_models(&json), vec![DiscoveredModel::new("gemini-2.5-pro", "Gemini 2.5 Pro")]);
    }

    #[test]
    fn parse_ollama_uses_tag_name() {
        let json = json!({ "models": [ { "name": "llama3:latest" }, { "name": "qwen2.5-coder:7b" } ] });
        assert_eq!(
            parse_ollama_models(&json),
            vec![
                DiscoveredModel::new("llama3:latest", "llama3:latest"),
                DiscoveredModel::new("qwen2.5-coder:7b", "qwen2.5-coder:7b"),
            ]
        );
    }

    #[test]
    fn parse_handles_malformed_shapes_without_panicking() {
        let garbage = json!({ "unexpected": true });
        assert!(parse_openai_models(&garbage).is_empty());
        assert!(parse_anthropic_models(&json!([])).is_empty());
        assert!(parse_gemini_models(&json!("nope")).is_empty());
        assert!(parse_ollama_models(&Value::Null).is_empty());
    }

    #[tokio::test]
    async fn discover_openai_sends_bearer_and_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [ { "id": "gpt-5.5" }, { "id": "gpt-4o-mini" } ]
            })))
            .mount(&server)
            .await;

        let models = discover_models(
            &Client::new(),
            ProviderTransport::OpenAi,
            &server.uri(),
            Some("sk-test"),
            DEFAULT_DISCOVERY_TIMEOUT,
        )
        .await
        .expect("discovery succeeds");

        assert_eq!(
            models,
            vec![DiscoveredModel::new("gpt-5.5", "gpt-5.5"), DiscoveredModel::new("gpt-4o-mini", "gpt-4o-mini")]
        );
    }

    #[tokio::test]
    async fn discover_anthropic_sends_versioned_key_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("x-api-key", "sk-ant"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [ { "id": "claude-opus-4-8", "display_name": "Claude Opus 4.8" } ]
            })))
            .mount(&server)
            .await;

        let models = discover_models(
            &Client::new(),
            ProviderTransport::Anthropic,
            &server.uri(),
            Some("sk-ant"),
            DEFAULT_DISCOVERY_TIMEOUT,
        )
        .await
        .expect("discovery succeeds");

        assert_eq!(models, vec![DiscoveredModel::new("claude-opus-4-8", "Claude Opus 4.8")]);
    }

    #[tokio::test]
    async fn discover_gemini_uses_query_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1beta/models"))
            .and(query_param("key", "goog-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [ { "name": "models/gemini-2.5-pro", "displayName": "Gemini 2.5 Pro", "supportedGenerationMethods": ["generateContent"] } ]
            })))
            .mount(&server)
            .await;

        let models = discover_models(
            &Client::new(),
            ProviderTransport::Gemini,
            &server.uri(),
            Some("goog-test"),
            DEFAULT_DISCOVERY_TIMEOUT,
        )
        .await
        .expect("discovery succeeds");

        assert_eq!(models, vec![DiscoveredModel::new("gemini-2.5-pro", "Gemini 2.5 Pro")]);
    }

    #[tokio::test]
    async fn discover_ollama_needs_no_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [ { "name": "llama3:latest" } ]
            })))
            .mount(&server)
            .await;

        let models =
            discover_models(&Client::new(), ProviderTransport::Ollama, &server.uri(), None, DEFAULT_DISCOVERY_TIMEOUT)
                .await
                .expect("discovery succeeds");

        assert_eq!(models, vec![DiscoveredModel::new("llama3:latest", "llama3:latest")]);
    }

    #[tokio::test]
    async fn discover_maps_http_error_to_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = discover_models(
            &Client::new(),
            ProviderTransport::OpenAi,
            &server.uri(),
            Some("bad"),
            DEFAULT_DISCOVERY_TIMEOUT,
        )
        .await
        .expect_err("unauthorized surfaces as Status");

        assert!(matches!(err, DiscoveryError::Status(401)));
    }
}
