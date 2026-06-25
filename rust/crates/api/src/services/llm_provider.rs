//! LLM provider configuration service.
//!
//! Owns user-provider orchestration over the `user_llm_configs` repository:
//! validation, secret encryption/decryption, default selection, and connection
//! test result persistence.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use agentforge_core::{AppResult, TenantScope, crypto};
use agentforge_llm::{
    ChatMessage, ChatRequest, DEFAULT_DISCOVERY_TIMEOUT, DiscoveredModel, LlmProviderBuildConfig, LlmProviderFactory,
    ProviderTransport, default_discovery_base, discover_models, normalize_provider_key, provider_spec,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::credential::{
    DiscoveredModelView, DiscoveredModelsResult, LlmProviderConfigResponse, LlmProviderPolicy, LlmProviderTestResult,
    ModelDiscoverySource, curated_models,
};
pub(crate) use crate::domain::credential::{
    discovered_models_response, llm_provider_delete_response, llm_provider_list_response, llm_provider_response,
    llm_provider_test_response, supported_providers_response,
};
use crate::domain::resource::{is_outbound_https_host_allowed, provider_base_url_allowed};
use crate::repositories::user::llm_config::{
    InsertLlmProviderConfig, LlmProviderConfigRow, UpdateLlmProviderConfig, UserLlmConfigRepository,
};

/// How long a discovered model list stays fresh. Provider catalogs change on the
/// order of weeks, and the list is provider-global (not tenant-specific), so a
/// process-wide TTL cache keyed by `provider|base_url` keeps the interactive
/// Add-service form fast without hammering provider APIs.
const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A cached model list with the instant it was fetched.
type DiscoveryCacheEntry = (Instant, Vec<DiscoveredModel>);

/// Process-global discovery cache. Keyed by `provider_key|base_url` — never by
/// API key, since the model catalog at an endpoint is the same regardless of
/// which tenant's key fetched it (the list is public metadata, not a secret).
static DISCOVERY_CACHE: LazyLock<Mutex<HashMap<String, DiscoveryCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn discovery_cache_get(key: &str) -> Option<Vec<DiscoveredModel>> {
    let cache = DISCOVERY_CACHE.lock().ok()?;
    let (stored_at, models) = cache.get(key)?;
    if stored_at.elapsed() < DISCOVERY_CACHE_TTL { Some(models.clone()) } else { None }
}

fn discovery_cache_put(key: String, models: Vec<DiscoveredModel>) {
    if let Ok(mut cache) = DISCOVERY_CACHE.lock() {
        cache.insert(key, (Instant::now(), models));
    }
}

pub(crate) struct LlmProviderService {
    repo: UserLlmConfigRepository,
    encryption_key: Option<[u8; 32]>,
    llm_factory: Arc<LlmProviderFactory>,
}

impl LlmProviderService {
    pub(crate) fn from_pool(
        pool: PgPool,
        encryption_key: Option<[u8; 32]>,
        llm_factory: Arc<LlmProviderFactory>,
    ) -> Self {
        Self::new(UserLlmConfigRepository::new(pool), encryption_key, llm_factory)
    }

    pub(crate) fn new(
        repo: UserLlmConfigRepository,
        encryption_key: Option<[u8; 32]>,
        llm_factory: Arc<LlmProviderFactory>,
    ) -> Self {
        Self { repo, encryption_key, llm_factory }
    }

    pub(crate) async fn list_providers(&self, scope: &TenantScope) -> AppResult<Vec<LlmProviderConfigResponse>> {
        Ok(self
            .repo
            .list_configs(scope)
            .await?
            .into_iter()
            .enumerate()
            .map(|(idx, row)| provider_response_from_row(row, i32::try_from(idx + 1).unwrap_or(i32::MAX)))
            .collect())
    }

    pub(crate) async fn get_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<LlmProviderConfigResponse> {
        let row = self.repo.get_config(scope, id).await?;
        Ok(provider_response_from_row(row, 1))
    }

    pub(crate) async fn create_provider(
        &self,
        scope: &TenantScope,
        provider: String,
        model: String,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> AppResult<LlmProviderConfigResponse> {
        let draft = LlmProviderPolicy::create_draft(provider, model, display_name, api_key, base_url)?;
        let (encrypted_api_key, api_key_prefix) = self.encrypted_api_key_and_prefix(draft.api_key.as_deref())?;

        if self.repo.provider_model_exists(scope, &draft.provider, &draft.model).await? {
            return Err(LlmProviderPolicy::provider_model_conflict().into());
        }

        let is_default = self.repo.should_insert_as_default(scope, &draft.provider).await?;
        let row = self
            .repo
            .insert_config(
                scope,
                InsertLlmProviderConfig {
                    provider: draft.provider,
                    model: draft.model,
                    display_name: draft.display_name,
                    base_url: draft.base_url,
                    api_key_prefix,
                    encrypted_api_key,
                    is_default,
                },
            )
            .await?;
        Ok(provider_response_from_row(row, 1))
    }

    pub(crate) async fn update_provider(
        &self,
        scope: &TenantScope,
        id: Uuid,
        model: Option<String>,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
        is_enabled: Option<bool>,
    ) -> AppResult<LlmProviderConfigResponse> {
        let current = self.repo.get_config(scope, id).await?;
        let draft = LlmProviderPolicy::update_draft(
            &current.provider,
            current.model,
            current.display_name,
            current.base_url,
            current.is_enabled,
            model,
            display_name,
            api_key,
            base_url,
            is_enabled,
        )?;
        let encrypted =
            if let Some(api_key) = draft.api_key.as_deref() { Some(self.encrypt_api_key(api_key)?) } else { None };
        let row = self
            .repo
            .update_config(
                scope,
                id,
                UpdateLlmProviderConfig {
                    model: draft.model,
                    display_name: draft.display_name,
                    base_url: draft.base_url,
                    is_enabled: draft.is_enabled,
                    encrypted_api_key: encrypted.as_ref().map(|(encrypted, _)| encrypted.clone()),
                    api_key_prefix: encrypted.map(|(_, prefix)| prefix),
                },
            )
            .await?;
        Ok(provider_response_from_row(row, 1))
    }

    pub(crate) async fn delete_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete_config(scope, id).await?;
        Ok(())
    }

    pub(crate) async fn set_default_provider(
        &self,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<LlmProviderConfigResponse> {
        let current = self.repo.get_config(scope, id).await?;
        let row = self.repo.set_default_config(scope, id, &current.provider).await?;
        Ok(provider_response_from_row(row, 1))
    }

    pub(crate) async fn test_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<LlmProviderTestResult> {
        let provider = self.repo.get_test_config(scope, id).await?;
        if !provider.is_enabled.unwrap_or(true) {
            self.repo.record_test_result(scope, id, "failed", Some("disabled"), Some("Provider is disabled.")).await?;
            return Ok(LlmProviderTestResult::disabled());
        }

        let model = LlmProviderPolicy::required_test_model(provider.model.as_deref())?;

        let api_key = if provider.provider == "ollama" {
            String::new()
        } else {
            let key = self.encryption_key.ok_or_else(LlmProviderPolicy::missing_test_api_key)?;
            crypto::decrypt_base64(&key, &provider.encrypted_api_key)
                .map_err(LlmProviderPolicy::decrypt_api_key_failed)?
        };

        // SSRF guard: refuse to probe an operator-supplied base URL pointing at a
        // private/loopback/metadata host (mirrors the discovery + inference guards;
        // Ollama is exempt). Record it as a failed test rather than a 500.
        if !provider_base_url_allowed(&provider.provider, provider.base_url.as_deref()) {
            let LlmProviderTestResult::Error(test_error) = LlmProviderTestResult::blocked_base_url() else {
                unreachable!("blocked_base_url is an Error variant");
            };
            self.repo
                .record_test_result(scope, id, "failed", Some(test_error.code()), Some(test_error.message()))
                .await?;
            return Ok(LlmProviderTestResult::Error(test_error));
        }

        let provider_instance = match self.llm_factory.build_with_config(LlmProviderBuildConfig {
            provider_key: provider.provider.clone(),
            api_key,
            base_url: provider.base_url.clone(),
        }) {
            Ok(instance) => instance,
            Err(error) => {
                let LlmProviderTestResult::Error(test_error) = LlmProviderTestResult::from_llm_error(&error) else {
                    unreachable!("LLM errors map to failed provider tests");
                };
                self.repo
                    .record_test_result(scope, id, "failed", Some(test_error.code()), Some(test_error.message()))
                    .await?;
                return Ok(LlmProviderTestResult::Error(test_error));
            }
        };

        let request = ChatRequest {
            model: model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Reply with a short connection check acknowledgement.".to_string(),
            }],
            max_tokens: Some(16),
            temperature: Some(0.0),
        };

        match tokio::time::timeout(Duration::from_secs(30), provider_instance.chat(request)).await {
            Ok(Ok(response)) => {
                self.repo.record_test_result(scope, id, "passed", None, None).await?;
                Ok(LlmProviderTestResult::success(
                    provider.id,
                    provider.provider,
                    model,
                    &response.content,
                    response.usage,
                ))
            }
            Ok(Err(error)) => {
                let LlmProviderTestResult::Error(test_error) = LlmProviderTestResult::from_llm_error(&error) else {
                    unreachable!("LLM errors map to failed provider tests");
                };
                self.repo
                    .record_test_result(scope, id, "failed", Some(test_error.code()), Some(test_error.message()))
                    .await?;
                Ok(LlmProviderTestResult::Error(test_error))
            }
            Err(_) => {
                self.repo
                    .record_test_result(
                        scope,
                        id,
                        "failed",
                        Some("timeout"),
                        Some("Provider connection test timed out."),
                    )
                    .await?;
                Ok(LlmProviderTestResult::timeout())
            }
        }
    }

    /// Discover models for a saved provider using its stored credential.
    /// Falls back to the curated list on any failure — discovery is an
    /// enrichment, never a hard error.
    pub(crate) async fn discover_models_for_config(
        &self,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<DiscoveredModelsResult> {
        let provider = self.repo.get_test_config(scope, id).await?;
        let transport = provider_spec(&provider.provider).map(|spec| spec.transport);

        let api_key = match transport {
            Some(ProviderTransport::Ollama) => None,
            _ => match self.encryption_key {
                Some(key) => crypto::decrypt_base64(&key, &provider.encrypted_api_key).ok(),
                None => None,
            },
        };

        Ok(self.resolve_and_discover(&provider.provider, provider.base_url, api_key).await)
    }

    /// Discover models for a not-yet-saved provider from form input (provider +
    /// optional base URL + optional key). Used by the Add-service form so an
    /// operator sees the live model list before committing. The raw key is used
    /// for this one outbound call and never persisted.
    pub(crate) async fn discover_models_preview(
        &self,
        provider: &str,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> DiscoveredModelsResult {
        self.resolve_and_discover(provider, base_url, api_key).await
    }

    /// Core discovery resolver shared by the saved-config and preview paths.
    /// Resolves transport + base URL, applies the SSRF host guard (except local
    /// Ollama), checks the TTL cache, calls the provider, and falls back to the
    /// curated registry list whenever a live list can't be produced.
    async fn resolve_and_discover(
        &self,
        provider: &str,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> DiscoveredModelsResult {
        let provider_key = normalize_provider_key(provider);
        let curated = DiscoveredModelsResult {
            provider: provider_key.clone(),
            source: ModelDiscoverySource::Curated,
            models: curated_models(&provider_key),
        };

        let Some(spec) = provider_spec(&provider_key) else {
            return curated;
        };
        let transport = spec.transport;
        let is_ollama = transport == ProviderTransport::Ollama;

        // Resolve the discovery base URL: config override, then the spec
        // default, then the transport's well-known default endpoint.
        let Some(base) = base_url
            .filter(|value| !value.trim().is_empty())
            .or_else(|| spec.default_base_url.map(str::to_string))
            .or_else(|| default_discovery_base(transport).map(str::to_string))
        else {
            return curated;
        };

        // SSRF guard for remote endpoints. Ollama is a local, keyless,
        // operator-configured runtime, so it is intentionally exempt.
        //
        // NOTE (F022): this is a literal-host guard only. It parses the host via
        // the `url` crate and rejects private/loopback/metadata/link-local IPs
        // (incl. IPv6 ULA, IPv4-mapped, and inet_aton-encoded forms), but it does
        // NOT resolve DNS — a public hostname whose A/AAAA record points at a
        // private IP (DNS rebinding) still passes here. There is no network-layer
        // egress backstop on this outbound discovery client (unlike the project-
        // clone path, which runs in a restricted-egress container). Treat this as
        // best-effort defense-in-depth; a connect-time resolve-and-recheck (or an
        // operator allowlist) is the residual hardening tracked in F022.
        if !is_ollama && !is_outbound_https_host_allowed(&base) {
            return curated;
        }

        // Keyed transports need a credential to list models.
        let api_key = api_key.filter(|key| !key.trim().is_empty());
        if !is_ollama && api_key.is_none() {
            return curated;
        }

        let cache_key = format!("{provider_key}|{base}");
        if let Some(models) = discovery_cache_get(&cache_key) {
            return live_result(&provider_key, models);
        }

        let client = reqwest::Client::new();
        match discover_models(&client, transport, &base, api_key.as_deref(), DEFAULT_DISCOVERY_TIMEOUT).await {
            Ok(models) if !models.is_empty() => {
                discovery_cache_put(cache_key, models.clone());
                live_result(&provider_key, models)
            }
            // Empty list or any error: keep the curated fallback.
            _ => curated,
        }
    }

    fn encrypted_api_key_and_prefix(&self, api_key: Option<&str>) -> AppResult<(String, Option<String>)> {
        if let Some(api_key) = api_key {
            let (encrypted_api_key, prefix) = self.encrypt_api_key(api_key)?;
            Ok((encrypted_api_key, Some(prefix)))
        } else {
            Ok((String::new(), None))
        }
    }

    fn encrypt_api_key(&self, api_key: &str) -> AppResult<(String, String)> {
        let key = self.encryption_key.ok_or_else(LlmProviderPolicy::missing_storage_key)?;
        let encrypted_api_key =
            crypto::encrypt_base64(&key, api_key).map_err(LlmProviderPolicy::encrypt_api_key_failed)?;
        Ok((encrypted_api_key, LlmProviderPolicy::api_key_prefix(api_key)))
    }
}

/// Wrap a non-empty live model list as a `Live`-sourced discovery result.
fn live_result(provider_key: &str, models: Vec<DiscoveredModel>) -> DiscoveredModelsResult {
    DiscoveredModelsResult {
        provider: provider_key.to_string(),
        source: ModelDiscoverySource::Live,
        models: models
            .into_iter()
            .map(|model| DiscoveredModelView { model: model.model, display_name: model.display_name })
            .collect(),
    }
}

fn provider_response_from_row(row: LlmProviderConfigRow, priority: i32) -> LlmProviderConfigResponse {
    let provider = row.provider;
    let display_name = row.display_name.unwrap_or_else(|| LlmProviderPolicy::display_name(&provider).to_string());

    LlmProviderConfigResponse {
        id: row.id,
        provider,
        display_name,
        model: row.model.unwrap_or_default(),
        base_url: row.base_url,
        api_key_prefix: row.api_key_prefix,
        priority,
        is_enabled: row.is_enabled.unwrap_or(true),
        is_default: row.is_default.unwrap_or(false),
        last_test_status: row.last_test_status,
        last_test_error_code: row.last_test_error_code,
        last_test_error_message: row.last_test_error_message,
        last_tested_at: row.last_tested_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_response_from_row_defaults_display_name_and_flags() {
        let id = Uuid::new_v4();
        let response = provider_response_from_row(
            LlmProviderConfigRow {
                id,
                provider: "openai".to_string(),
                model: Some("gpt-5.5".to_string()),
                display_name: None,
                base_url: None,
                api_key_prefix: Some("sk-12345".to_string()),
                is_enabled: None,
                is_default: None,
                last_test_status: Some("passed".to_string()),
                last_test_error_code: None,
                last_test_error_message: None,
                last_tested_at: Some("2026-05-20T13:00:00Z".to_string()),
            },
            2,
        );

        assert_eq!(response.id, id);
        assert_eq!(response.display_name, "OpenAI");
        assert_eq!(response.model, "gpt-5.5");
        assert!(response.is_enabled);
        assert!(!response.is_default);
        assert_eq!(response.priority, 2);
    }
}
