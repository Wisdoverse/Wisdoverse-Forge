//! Credential domain rules.
//!
//! This module owns API key, SSH key, Container CLI, and LLM provider
//! credential policies that are independent of repositories, encryption, HTTP
//! handlers, and filesystem mount materialization.

use agentforge_core::{AppError, AppResult, CliToolKind, ErrorKind};
use agentforge_db::entities::{ApiKey, GitCredential, SshKey};
use agentforge_llm::{LlmError, Usage, normalize_provider_key, provider_spec, supported_provider_specs};
use serde::Serialize;
use url::Url;
use uuid::Uuid;

/// Result of creating an API key — includes the plaintext key (shown once).
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResult {
    pub key: ApiKey,
    /// The plaintext API key — only returned at creation time.
    pub plaintext_key: String,
}

pub(crate) fn api_key_list_response(keys: &[ApiKey]) -> serde_json::Value {
    let api_keys: Vec<_> = keys.iter().map(api_key_payload).collect();
    serde_json::json!({
        "ok": true,
        "data": api_keys.clone(),
        "apiKeys": api_keys,
    })
}

pub(crate) fn api_key_create_response(result: CreateApiKeyResult) -> serde_json::Value {
    let api_key = api_key_payload(&result.key);
    let plaintext_key = result.plaintext_key;
    serde_json::json!({
        "ok": true,
        "data": {
            "key": api_key.clone(),
            "apiKey": api_key.clone(),
            "plaintext_key": plaintext_key.clone(),
            "plaintextKey": plaintext_key.clone(),
        },
        "key": plaintext_key.clone(),
        "plaintextKey": plaintext_key,
        "apiKey": api_key,
    })
}

pub(crate) fn credential_delete_response() -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

pub(crate) struct CredentialRepositoryPolicy;

impl CredentialRepositoryPolicy {
    pub(crate) fn api_key_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("api key {id}")).into()
    }

    pub(crate) fn git_credential_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("git credential {id}")).into()
    }

    pub(crate) fn ssh_key_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("ssh key {id}")).into()
    }

    pub(crate) fn llm_provider_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("llm provider {id}")).into()
    }
}

pub(crate) fn ssh_key_list_response(keys: &[SshKey]) -> serde_json::Value {
    let keys: Vec<_> = keys.iter().map(ssh_key_payload).collect();
    serde_json::json!({
        "ok": true,
        "data": keys.clone(),
        "keys": keys,
    })
}

pub(crate) fn cli_credentials_response<T: Serialize>(connections: T) -> serde_json::Value {
    serde_json::json!({ "ok": true, "connections": connections })
}

pub(crate) fn cli_credential_stored_response(cli_tool: &str) -> serde_json::Value {
    serde_json::json!({ "ok": true, "cli_tool": cli_tool, "status": "stored" })
}

pub(crate) fn cli_credential_deleted_response(cli_tool: &str) -> serde_json::Value {
    serde_json::json!({ "ok": true, "cli_tool": cli_tool, "status": "deleted" })
}

pub(crate) fn container_cli_oauth_file_map_plaintext(files: &serde_json::Value) -> AppResult<String> {
    serde_json::to_string(files).map_err(|err| ContainerCliCredentialPolicy::serialize_files_failed(err).into())
}

pub(crate) fn ssh_key_create_response(key: SshKey) -> serde_json::Value {
    let key = ssh_key_payload(&key);
    serde_json::json!({
        "ok": true,
        "data": key.clone(),
        "key": key,
    })
}

pub(crate) fn git_credential_response(cred: &GitCredential) -> serde_json::Value {
    let credential = git_credential_payload(cred);
    serde_json::json!({
        "ok": true,
        "data": credential.clone(),
        "credential": credential,
    })
}

pub(crate) fn git_credentials_response(creds: &[GitCredential]) -> serde_json::Value {
    let credentials: Vec<_> = creds.iter().map(git_credential_payload).collect();
    serde_json::json!({
        "ok": true,
        "data": credentials.clone(),
        "credentials": credentials,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderModelInfo {
    pub(crate) model: &'static str,
    pub(crate) display_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderInfo {
    pub(crate) provider: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) default_model: Option<&'static str>,
    pub(crate) default_base_url: Option<&'static str>,
    pub(crate) requires_api_key: bool,
    pub(crate) allow_custom_models: bool,
    pub(crate) models: Vec<ProviderModelInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LlmProviderConfigResponse {
    pub(crate) id: Uuid,
    pub(crate) provider: String,
    pub(crate) display_name: String,
    pub(crate) model: String,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key_prefix: Option<String>,
    pub(crate) priority: i32,
    pub(crate) is_enabled: bool,
    pub(crate) is_default: bool,
    pub(crate) last_test_status: Option<String>,
    pub(crate) last_test_error_code: Option<String>,
    pub(crate) last_test_error_message: Option<String>,
    pub(crate) last_tested_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum LlmProviderTestResult {
    Success(LlmProviderTestSuccess),
    Error(LlmProviderTestError),
}

impl LlmProviderTestResult {
    pub(crate) fn success(
        id: Uuid,
        provider: impl Into<String>,
        model: impl Into<String>,
        content: &str,
        usage: Option<Usage>,
    ) -> Self {
        Self::Success(LlmProviderTestSuccess {
            provider: LlmProviderTestProvider { id, provider: provider.into(), model: model.into() },
            response_preview: content.chars().take(120).collect(),
            usage,
        })
    }

    pub(crate) fn disabled() -> Self {
        Self::Error(LlmProviderTestError { code: "disabled", message: "Provider is disabled.", retryable: false })
    }

    pub(crate) fn timeout() -> Self {
        Self::Error(LlmProviderTestError {
            code: "timeout",
            message: "Provider connection test timed out.",
            retryable: true,
        })
    }

    pub(crate) fn from_llm_error(error: &LlmError) -> Self {
        let (code, message, retryable) = llm_provider_test_error_parts(error);
        Self::Error(LlmProviderTestError { code, message, retryable })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LlmProviderTestSuccess {
    provider: LlmProviderTestProvider,
    response_preview: String,
    usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LlmProviderTestProvider {
    id: Uuid,
    provider: String,
    model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct LlmProviderTestError {
    code: &'static str,
    message: &'static str,
    retryable: bool,
}

impl LlmProviderTestError {
    pub(crate) fn code(self) -> &'static str {
        self.code
    }

    pub(crate) fn message(self) -> &'static str {
        self.message
    }
}

pub(crate) fn supported_provider_list() -> Vec<ProviderInfo> {
    supported_provider_specs()
        .iter()
        .map(|spec| ProviderInfo {
            provider: spec.key,
            display_name: spec.display_name,
            default_model: spec.default_model,
            default_base_url: spec.default_base_url,
            requires_api_key: spec.requires_api_key,
            allow_custom_models: spec.allow_custom_models,
            models: spec
                .models
                .iter()
                .map(|model| ProviderModelInfo { model: model.model, display_name: model.display_name })
                .collect(),
        })
        .collect()
}

pub(crate) fn supported_providers_response() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "providers": supported_provider_list(),
    })
}

pub(crate) fn llm_provider_list_response(providers: &[LlmProviderConfigResponse]) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "providers": providers,
    })
}

pub(crate) fn llm_provider_response(provider: &LlmProviderConfigResponse) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "provider": provider,
    })
}

pub(crate) fn llm_provider_delete_response() -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

pub(crate) fn llm_provider_test_response(result: LlmProviderTestResult) -> serde_json::Value {
    match result {
        LlmProviderTestResult::Success(success) => serde_json::json!({
            "ok": true,
            "provider": success.provider,
            "responsePreview": success.response_preview,
            "usage": success.usage,
        }),
        LlmProviderTestResult::Error(error) => serde_json::json!({
            "ok": false,
            "error": error,
        }),
    }
}

pub(crate) fn llm_provider_test_error_parts(error: &LlmError) -> (&'static str, &'static str, bool) {
    match error {
        LlmError::Api { status: 401, .. } | LlmError::Api { status: 403, .. } => {
            ("unauthorized", "Provider rejected the API key.", false)
        }
        LlmError::Api { status: 429, .. } => ("rate_limited", "Provider rate limit reached.", true),
        LlmError::Api { status: 400, .. } | LlmError::Api { status: 404, .. } => {
            ("bad_request", "Provider rejected the model or request.", false)
        }
        LlmError::Api { status: 500..=599, .. } => ("provider_error", "Provider service is currently failing.", true),
        LlmError::Http(_) => ("network", "Network error reaching provider.", true),
        LlmError::Parse(_) => ("invalid_response", "Provider returned an unexpected response.", true),
        LlmError::NotConfigured(_) => ("not_configured", "Provider is not configured for this deployment.", false),
        LlmError::NotImplemented(_) => ("not_implemented", "Provider is not supported by this deployment.", false),
        LlmError::Api { .. } => ("provider_error", "Provider rejected the connection test.", true),
    }
}

fn api_key_payload(key: &ApiKey) -> serde_json::Value {
    serde_json::json!({
        "id": key.id,
        "orgId": key.organization_id,
        "organizationId": key.organization_id,
        "userId": key.user_id,
        "name": &key.name,
        "keyPrefix": &key.key_prefix,
        "scopes": &key.scopes,
        "expiresAt": &key.expires_at,
        "lastUsedAt": &key.last_used_at,
        "createdAt": &key.created_at,
        "revokedAt": &key.revoked_at,
    })
}

fn ssh_key_payload(key: &SshKey) -> serde_json::Value {
    serde_json::json!({
        "id": key.id,
        "orgId": key.organization_id,
        "organizationId": key.organization_id,
        "userId": key.user_id,
        "name": &key.name,
        "publicKey": &key.public_key,
        "fingerprint": &key.fingerprint,
        "keyType": &key.key_type,
        "createdAt": &key.created_at,
    })
}

fn git_credential_payload(cred: &GitCredential) -> serde_json::Value {
    serde_json::json!({
        "id": cred.id,
        "orgId": cred.organization_id,
        "organizationId": cred.organization_id,
        "userId": cred.user_id,
        "name": cred.name,
        "provider": cred.provider,
        "credentialType": cred.credential_type,
        "credential_type": cred.credential_type,
        "host": cred.remote_url,
        "remoteUrl": cred.remote_url,
        "remote_url": cred.remote_url,
        "createdAt": cred.created_at,
        "created_at": cred.created_at,
        "updatedAt": cred.updated_at,
        "updated_at": cred.updated_at,
    })
}

const API_KEY_PREFIX: &str = "af_";
const VALID_API_KEY_SCOPES: &[&str] = &["read", "write", "admin"];
const VALID_SSH_KEY_PREFIXES: &[&str] =
    &["ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521"];
const VALID_GIT_PROVIDERS: &[&str] = &["github", "gitlab", "bitbucket", "custom"];
const VALID_GIT_CREDENTIAL_TYPES: &[&str] = &["token", "ssh", "oauth"];

/// Validated pagination request for credential lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialListPage {
    limit: i64,
    offset: i64,
}

impl CredentialListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// API key display name value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApiKeyName<'a> {
    value: &'a str,
}

impl<'a> ApiKeyName<'a> {
    pub(crate) fn parse(name: &'a str) -> AppResult<Self> {
        let value = name.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// API key plaintext format policy.
pub(crate) struct ApiKeyFormat;

impl ApiKeyFormat {
    pub(crate) const PREFIX: &'static str = API_KEY_PREFIX;

    pub(crate) fn validate(key: &str) -> AppResult<()> {
        if !key.starts_with(API_KEY_PREFIX) {
            return Err(ErrorKind::Validation("key must start with 'af_'".into()).into());
        }
        if key.len() != 67 {
            return Err(ErrorKind::Validation("key must be exactly 67 characters".into()).into());
        }
        if hex::decode(&key[API_KEY_PREFIX.len()..]).is_err() {
            return Err(ErrorKind::Validation("key must contain valid hex characters after prefix".into()).into());
        }
        Ok(())
    }
}

/// API key authentication failure policy. All validation failures intentionally
/// collapse to `Unauthorized` so callers cannot distinguish malformed, missing,
/// revoked, expired, or unknown keys.
pub(crate) struct ApiKeyAuthenticationPolicy;

impl ApiKeyAuthenticationPolicy {
    pub(crate) fn unauthorized() -> ErrorKind {
        ErrorKind::Unauthorized
    }

    pub(crate) fn ensure_format(raw_key: &str) -> AppResult<()> {
        ApiKeyFormat::validate(raw_key).map_err(|_| Self::unauthorized().into())
    }

    pub(crate) fn require_key<T>(key: Option<T>) -> AppResult<T> {
        key.ok_or_else(|| Self::unauthorized().into())
    }

    pub(crate) fn ensure_not_revoked(revoked: bool) -> AppResult<()> {
        if revoked { Err(Self::unauthorized().into()) } else { Ok(()) }
    }

    pub(crate) fn ensure_not_expired(
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<()> {
        if expires_at.is_some_and(|expires_at| expires_at < now) { Err(Self::unauthorized().into()) } else { Ok(()) }
    }
}

/// API key scope policy.
pub(crate) struct ApiKeyScopePolicy;

impl ApiKeyScopePolicy {
    pub(crate) fn validate(scopes: &[String]) -> AppResult<()> {
        for scope in scopes {
            if !VALID_API_KEY_SCOPES.contains(&scope.as_str()) {
                return Err(ErrorKind::Validation(format!(
                    "invalid scope '{}', valid: {:?}",
                    scope, VALID_API_KEY_SCOPES
                ))
                .into());
            }
        }
        Ok(())
    }
}

/// SSH key display name value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SshKeyName<'a> {
    value: &'a str,
}

impl<'a> SshKeyName<'a> {
    pub(crate) fn parse(name: &'a str) -> AppResult<Self> {
        let value = name.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Supported SSH key kinds persisted with each key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SshKeyKind {
    Ed25519,
    Rsa,
    Ecdsa,
}

impl SshKeyKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Rsa => "rsa",
            Self::Ecdsa => "ecdsa",
        }
    }
}

/// Validated SSH public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SshPublicKey<'a> {
    value: &'a str,
    kind: SshKeyKind,
}

impl<'a> SshPublicKey<'a> {
    pub(crate) fn parse(public_key: &'a str) -> AppResult<Self> {
        let value = public_key.trim();
        let Some(prefix) = VALID_SSH_KEY_PREFIXES.iter().find(|prefix| value.starts_with(**prefix)) else {
            return Err(ErrorKind::Validation(format!(
                "unsupported SSH key type, expected one of: {:?}",
                VALID_SSH_KEY_PREFIXES
            ))
            .into());
        };
        let kind = if prefix.starts_with("ecdsa") {
            SshKeyKind::Ecdsa
        } else if *prefix == "ssh-rsa" {
            SshKeyKind::Rsa
        } else {
            SshKeyKind::Ed25519
        };
        Ok(Self { value, kind })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }

    pub(crate) fn kind(self) -> SshKeyKind {
        self.kind
    }

    /// Compute a SHA-256 fingerprint of the public key's binary data.
    pub(crate) fn fingerprint(self) -> String {
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;

        let bytes_to_hash = match self.value.split_whitespace().nth(1) {
            Some(b64_data) => match engine.decode(b64_data) {
                Ok(decoded) => decoded,
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to base64-decode SSH key data, hashing raw string");
                    self.value.as_bytes().to_vec()
                }
            },
            None => {
                tracing::warn!("SSH key missing base64 data field, hashing raw string");
                self.value.as_bytes().to_vec()
            }
        };

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&bytes_to_hash);
        let digest = hasher.finalize();
        format!("SHA256:{}", engine.encode(digest))
    }
}

/// Container CLI credential policy.
pub(crate) struct ContainerCliCredentialPolicy;

impl ContainerCliCredentialPolicy {
    pub(crate) fn canonical_tool(raw: &str) -> AppResult<&'static str> {
        CliToolKind::parse_legacy(raw)
            .map(CliToolKind::as_str)
            .map_err(|err| ErrorKind::Validation(err.to_string()).into())
    }

    pub(crate) fn provider_for_tool(cli_tool: &str) -> &'static str {
        match CliToolKind::parse_legacy(cli_tool).ok() {
            Some(CliToolKind::Claude | CliToolKind::Opencode) => "anthropic",
            Some(CliToolKind::Codex) => "openai",
            Some(CliToolKind::Gemini) => "google",
            None => "unknown",
        }
    }

    pub(crate) fn api_key_env_for_tool(cli_tool: &str) -> Option<&'static str> {
        match CliToolKind::parse_legacy(cli_tool).ok()? {
            CliToolKind::Claude | CliToolKind::Opencode => Some("ANTHROPIC_API_KEY"),
            CliToolKind::Codex => Some("OPENAI_API_KEY"),
            CliToolKind::Gemini => Some("GEMINI_API_KEY"),
        }
    }

    pub(crate) fn provider_env(cli_tool: &str) -> Option<(&'static str, &'static str)> {
        Some((Self::provider_for_tool(cli_tool), Self::api_key_env_for_tool(cli_tool)?))
            .filter(|(provider, _)| *provider != "unknown")
    }

    pub(crate) fn validate_oauth_file_map(files: &serde_json::Value) -> AppResult<()> {
        let map = files.as_object().ok_or_else(|| {
            ErrorKind::Validation("`files` must be a JSON object mapping filename → contents".to_string())
        })?;
        if map.is_empty() {
            return Err(ErrorKind::Validation("`files` must not be empty".to_string()).into());
        }
        for (name, value) in map {
            if !value.is_string() {
                return Err(ErrorKind::Validation(format!(
                    "`files.{name}` must be a string; got {}",
                    json_value_kind(value)
                ))
                .into());
            }
        }
        Ok(())
    }

    pub(crate) fn missing_storage_key() -> ErrorKind {
        ErrorKind::Validation(
            "LLM_ENCRYPTION_KEY is not configured — refusing to store plaintext credentials".to_string(),
        )
    }

    pub(crate) fn serialize_files_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("serialize files: {err}"))
    }

    pub(crate) fn encrypt_credentials_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("failed to encrypt credentials: {err}"))
    }

    pub(crate) fn stored_user_llm_key_decrypt_failed() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!(
            "stored user LLM API key failed to decrypt (likely LLM_ENCRYPTION_KEY rotation); re-upload via /api/v1/user-llm-configs"
        ))
    }

    pub(crate) fn stored_oauth_decrypt_failed(cli_tool: &str) -> ErrorKind {
        ErrorKind::Validation(format!(
            "stored {cli_tool} credentials cannot be decrypted — reconnect via /api/v1/cli-auth-proxy or /api/v1/cli-credentials"
        ))
    }
}

/// Git credential encryption and stored-token error policy.
pub(crate) struct GitCredentialEncryptionPolicy;

impl GitCredentialEncryptionPolicy {
    pub(crate) fn missing_decrypt_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - cannot decrypt stored git credentials".into())
    }

    pub(crate) fn missing_storage_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - refusing to store plaintext git tokens".into())
    }

    pub(crate) fn encrypt_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("encrypt git credential token failed: {err}"))
    }

    pub(crate) fn ciphertext_not_utf8(provider: &str, err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Validation(format!("stored {provider} git credential ciphertext is not UTF-8: {err}"))
    }

    pub(crate) fn decrypt_failed(provider: &str) -> ErrorKind {
        ErrorKind::Validation(format!(
            "stored {provider} git credential cannot be decrypted - reconnect it in Settings"
        ))
    }
}

/// User-owned LLM provider configuration policy.
pub(crate) struct LlmProviderPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlmProviderCreateDraft {
    pub(crate) provider: String,
    pub(crate) display_name: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
    pub(crate) base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlmProviderUpdateDraft {
    pub(crate) display_name: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) is_enabled: bool,
}

impl LlmProviderPolicy {
    pub(crate) fn provider_model_conflict() -> ErrorKind {
        ErrorKind::Conflict("provider/model already exists".into())
    }

    pub(crate) fn required_test_model(model: Option<&str>) -> AppResult<String> {
        model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ErrorKind::Validation("model is required before testing a provider".into()).into())
    }

    pub(crate) fn missing_test_api_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - cannot test stored API keys".into())
    }

    pub(crate) fn missing_storage_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - refusing to store plaintext API keys".into())
    }

    pub(crate) fn decrypt_api_key_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("decrypt llm provider api key failed: {err}"))
    }

    pub(crate) fn encrypt_api_key_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("encrypt llm provider api key failed: {err}"))
    }

    pub(crate) fn normalize_supported_provider(provider: &str) -> AppResult<String> {
        let provider = normalize_provider_key(provider);
        if provider_spec(&provider).is_none() {
            return Err(ErrorKind::Validation(format!("invalid provider '{provider}'")).into());
        }
        Ok(provider)
    }

    pub(crate) fn requires_api_key(provider: &str) -> bool {
        provider_spec(provider).map(|spec| spec.requires_api_key).unwrap_or(true)
    }

    pub(crate) fn requires_base_url(provider: &str) -> bool {
        provider_spec(provider)
            .map(|spec| spec.key == "openai_compatible" && spec.default_base_url.is_none())
            .unwrap_or(false)
    }

    pub(crate) fn api_key_prefix(api_key: &str) -> String {
        api_key.chars().take(8).collect()
    }

    pub(crate) fn display_name(provider: &str) -> &'static str {
        provider_spec(provider).map(|spec| spec.display_name).unwrap_or("Custom")
    }

    pub(crate) fn create_draft(
        provider: String,
        model: String,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> AppResult<LlmProviderCreateDraft> {
        let provider = Self::normalize_supported_provider(&provider)?;
        let model = clean_required(model, "model is required")?;
        let api_key = clean_optional(api_key);
        if Self::requires_api_key(&provider) && api_key.is_none() {
            return Err(ErrorKind::Validation("apiKey is required".into()).into());
        }
        let base_url = clean_optional(base_url);
        if Self::requires_base_url(&provider) && base_url.is_none() {
            return Err(ErrorKind::Validation("baseUrl is required for this provider".into()).into());
        }
        let display_name = clean_optional(display_name).unwrap_or_else(|| Self::display_name(&provider).to_string());

        Ok(LlmProviderCreateDraft { provider, display_name, model, api_key, base_url })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_draft(
        provider: &str,
        current_model: Option<String>,
        current_display_name: Option<String>,
        current_base_url: Option<String>,
        current_is_enabled: Option<bool>,
        model: Option<String>,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
        is_enabled: Option<bool>,
    ) -> AppResult<LlmProviderUpdateDraft> {
        let model =
            clean_optional(model).or(current_model).ok_or_else(|| ErrorKind::Validation("model is required".into()))?;
        let display_name = clean_optional(display_name)
            .or(current_display_name)
            .unwrap_or_else(|| Self::display_name(provider).to_string());
        let api_key = clean_optional(api_key);
        let base_url = clean_optional(base_url).or(current_base_url);
        if Self::requires_base_url(provider) && base_url.is_none() {
            return Err(ErrorKind::Validation("baseUrl is required for this provider".into()).into());
        }
        let is_enabled = is_enabled.unwrap_or(current_is_enabled.unwrap_or(true));

        Ok(LlmProviderUpdateDraft { display_name, model, api_key, base_url, is_enabled })
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

fn clean_required(value: String, message: &str) -> AppResult<String> {
    clean_optional(Some(value)).ok_or_else(|| ErrorKind::Validation(message.into()).into())
}

fn json_value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::String(_) => "string",
    }
}

/// Host-side OAuth mount directory key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OauthMountContainerKey<'a> {
    value: &'a str,
}

impl<'a> OauthMountContainerKey<'a> {
    pub(crate) fn parse(value: &'a str) -> Result<Self, &'static str> {
        if value.is_empty() || value.contains('/') || value.contains('\\') || value == ".." || value == "." {
            return Err("invalid container_key");
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Validated Git credential write request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GitCredentialDraft<'a> {
    name: &'a str,
    provider: &'a str,
    credential_type: &'a str,
}

impl<'a> GitCredentialDraft<'a> {
    pub(crate) fn parse(name: &'a str, provider: &'a str, credential_type: &'a str) -> AppResult<Self> {
        let name = name.trim();
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }

        if !VALID_GIT_PROVIDERS.contains(&provider) {
            return Err(ErrorKind::Validation(format!(
                "invalid provider '{}', valid: {:?}",
                provider, VALID_GIT_PROVIDERS
            ))
            .into());
        }

        if !VALID_GIT_CREDENTIAL_TYPES.contains(&credential_type) {
            return Err(ErrorKind::Validation(format!(
                "invalid credential_type '{}', valid: {:?}",
                credential_type, VALID_GIT_CREDENTIAL_TYPES
            ))
            .into());
        }

        Ok(Self { name, provider, credential_type })
    }

    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn provider(self) -> &'a str {
        self.provider
    }

    pub(crate) fn credential_type(self) -> &'a str {
        self.credential_type
    }
}

/// Decrypted Git token policy for CLI credential injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GitCredentialToken<'a> {
    value: &'a str,
}

impl<'a> GitCredentialToken<'a> {
    pub(crate) fn parse(value: &'a str) -> Option<Self> {
        let value = value.trim();
        (!value.is_empty()).then_some(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Git remote host normalization policy for CLI credential injection.
pub(crate) struct GitRemoteHost;

impl GitRemoteHost {
    pub(crate) fn normalize(value: Option<&str>, default_host: &str) -> String {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return default_host.to_string();
        };
        if let Ok(url) = Url::parse(value)
            && let Some(host) = url.host_str()
        {
            return host.to_ascii_lowercase();
        }

        let value = value.trim_start_matches("ssh://").trim_start_matches("git+ssh://").trim_end_matches('/').trim();
        let without_user = value.rsplit_once('@').map_or(value, |(_, host)| host);
        without_user
            .split([':', '/'])
            .next()
            .filter(|host| !host.is_empty())
            .unwrap_or(default_host)
            .to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use agentforge_core::{OrgId, UserId};
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;

    fn sample_org_id() -> OrgId {
        OrgId::from(Uuid::from_u128(0x11111111111141118111111111111111))
    }

    fn sample_user_id() -> UserId {
        UserId::from(Uuid::from_u128(0x22222222222242228222222222222222))
    }

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
    }

    fn sample_api_key() -> ApiKey {
        ApiKey {
            id: Uuid::from_u128(0x33333333333343338333333333333333),
            organization_id: sample_org_id(),
            user_id: sample_user_id(),
            name: "CI".to_string(),
            key_hash: "secret-hash".to_string(),
            key_prefix: "af_12345678".to_string(),
            scopes: vec!["read".to_string()],
            expires_at: None,
            last_used_at: None,
            created_at: sample_time(),
            revoked_at: None,
        }
    }

    fn sample_ssh_key() -> SshKey {
        SshKey {
            id: Uuid::from_u128(0x44444444444444448444444444444444),
            organization_id: sample_org_id(),
            user_id: sample_user_id(),
            name: "Laptop".to_string(),
            public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 dev@example.com".to_string(),
            fingerprint: "SHA256:abc".to_string(),
            key_type: "ed25519".to_string(),
            created_at: sample_time(),
        }
    }

    fn sample_git_credential() -> GitCredential {
        GitCredential {
            id: Uuid::from_u128(0x55555555555545558555555555555555),
            organization_id: sample_org_id(),
            user_id: sample_user_id(),
            name: "GitHub".to_string(),
            provider: "github".to_string(),
            credential_type: "token".to_string(),
            token_encrypted: Some(b"secret-ciphertext".to_vec()),
            token_nonce: Some(b"secret-nonce".to_vec()),
            remote_url: Some("https://github.com/Wisdoverse/Wisdoverse-Forge".to_string()),
            created_at: sample_time(),
            updated_at: sample_time(),
        }
    }

    #[test]
    fn api_key_list_response_owns_legacy_envelope_without_hash() {
        let body = api_key_list_response(&[sample_api_key()]);
        let key = &body["data"][0];

        assert_eq!(body["ok"], true);
        assert_eq!(body["apiKeys"], body["data"]);
        assert_eq!(key["name"], "CI");
        assert_eq!(key["keyPrefix"], "af_12345678");
        assert_eq!(key["orgId"], key["organizationId"]);
        assert!(key.get("key_hash").is_none());
        assert!(key.get("keyHash").is_none());
    }

    #[test]
    fn api_key_create_response_owns_plaintext_once_contract() {
        let body = api_key_create_response(CreateApiKeyResult {
            key: sample_api_key(),
            plaintext_key: "af_plaintext".to_string(),
        });

        assert_eq!(body["ok"], true);
        assert_eq!(body["key"], "af_plaintext");
        assert_eq!(body["plaintextKey"], "af_plaintext");
        assert_eq!(body["data"]["plaintext_key"], "af_plaintext");
        assert_eq!(body["data"]["plaintextKey"], "af_plaintext");
        assert_eq!(body["apiKey"], body["data"]["apiKey"]);
        assert_eq!(body["data"]["key"], body["data"]["apiKey"]);
    }

    #[test]
    fn ssh_key_responses_own_legacy_envelope() {
        let list_body = ssh_key_list_response(&[sample_ssh_key()]);
        let create_body = ssh_key_create_response(sample_ssh_key());

        assert_eq!(list_body["ok"], true);
        assert_eq!(list_body["keys"], list_body["data"]);
        assert_eq!(list_body["data"][0]["publicKey"], "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 dev@example.com");
        assert_eq!(list_body["data"][0]["keyType"], "ed25519");
        assert_eq!(create_body["key"], create_body["data"]);
    }

    #[test]
    fn git_credential_responses_own_legacy_fields_without_secrets() {
        let credential_body = git_credential_response(&sample_git_credential());
        let list_body = git_credentials_response(&[sample_git_credential()]);
        let credential = &credential_body["data"];

        assert_eq!(credential_body["ok"], true);
        assert_eq!(credential_body["credential"], credential_body["data"]);
        assert_eq!(list_body["credentials"], list_body["data"]);
        assert_eq!(credential["credentialType"], "token");
        assert_eq!(credential["credential_type"], "token");
        assert_eq!(credential["host"], credential["remoteUrl"]);
        assert_eq!(credential["remote_url"], credential["remoteUrl"]);
        assert!(credential.get("token_encrypted").is_none());
        assert!(credential.get("tokenEncrypted").is_none());
        assert!(credential.get("token_nonce").is_none());
        assert!(credential.get("tokenNonce").is_none());
    }

    #[test]
    fn list_page_clamps_limit_and_offset() {
        assert_eq!(CredentialListPage::new(0, -1).limit(), 1);
        assert_eq!(CredentialListPage::new(101, 50).limit(), 100);
        assert_eq!(CredentialListPage::new(20, -1).offset(), 0);
        assert_eq!(CredentialListPage::new(20, 50).offset(), 50);
    }

    #[test]
    fn api_key_name_trims_and_bounds() {
        assert_eq!(ApiKeyName::parse("  valid  ").unwrap().value(), "valid");
        assert!(ApiKeyName::parse("   ").is_err());
        assert!(ApiKeyName::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn api_key_format_validates_prefix_length_and_hex() {
        assert!(ApiKeyFormat::validate(&format!("af_{}", "a".repeat(64))).is_ok());
        assert!(ApiKeyFormat::validate(&format!("bad_{}", "a".repeat(64))).is_err());
        assert!(ApiKeyFormat::validate("af_short").is_err());
        assert!(ApiKeyFormat::validate(&format!("af_{}", "g".repeat(64))).is_err());
    }

    #[test]
    fn api_key_authentication_policy_collapses_failures_to_unauthorized() {
        assert!(matches!(ApiKeyAuthenticationPolicy::unauthorized(), ErrorKind::Unauthorized));
        assert!(ApiKeyAuthenticationPolicy::ensure_format(&format!("af_{}", "a".repeat(64))).is_ok());
        assert!(ApiKeyAuthenticationPolicy::ensure_format("bad").is_err());
        assert_eq!(ApiKeyAuthenticationPolicy::require_key(Some(7)).unwrap(), 7);
        assert!(ApiKeyAuthenticationPolicy::require_key::<i32>(None).is_err());
        assert!(ApiKeyAuthenticationPolicy::ensure_not_revoked(false).is_ok());
        assert!(ApiKeyAuthenticationPolicy::ensure_not_revoked(true).is_err());

        let now = chrono::Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert!(ApiKeyAuthenticationPolicy::ensure_not_expired(Some(now + chrono::Duration::seconds(1)), now).is_ok());
        assert!(ApiKeyAuthenticationPolicy::ensure_not_expired(Some(now - chrono::Duration::seconds(1)), now).is_err());
    }

    #[test]
    fn api_key_scope_policy_is_case_sensitive() {
        assert!(ApiKeyScopePolicy::validate(&["read".into(), "write".into(), "admin".into()]).is_ok());
        assert!(ApiKeyScopePolicy::validate(&[]).is_ok());
        assert!(ApiKeyScopePolicy::validate(&["READ".into()]).is_err());
        assert!(ApiKeyScopePolicy::validate(&["delete".into()]).is_err());
    }

    #[test]
    fn ssh_public_key_recognizes_supported_kinds() {
        assert_eq!(
            SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 dev@example.com").unwrap().kind(),
            SshKeyKind::Ed25519
        );
        assert_eq!(
            SshPublicKey::parse("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQ dev@example.com").unwrap().kind(),
            SshKeyKind::Rsa
        );
        assert_eq!(
            SshPublicKey::parse("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHA dev@example.com").unwrap().kind(),
            SshKeyKind::Ecdsa
        );
        assert!(SshPublicKey::parse("ssh-dss AAAAB3NzaC1kc3M dev@example.com").is_err());
        assert!(SshPublicKey::parse("").is_err());
    }

    #[test]
    fn ssh_fingerprint_is_stable_and_key_specific() {
        let fp1 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA dev1@example.com").unwrap().fingerprint();
        let fp2 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA dev1@example.com").unwrap().fingerprint();
        let fp3 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5BBBB dev2@example.com").unwrap().fingerprint();
        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3);
        assert!(fp1.starts_with("SHA256:"));
    }

    #[test]
    fn cli_credential_policy_canonicalizes_and_maps_tools() {
        assert_eq!(ContainerCliCredentialPolicy::canonical_tool(" CLAUDE ").unwrap(), "claude");
        assert!(ContainerCliCredentialPolicy::canonical_tool("vim").is_err());
        assert_eq!(ContainerCliCredentialPolicy::provider_env("claude"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("codex"), Some(("openai", "OPENAI_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("gemini"), Some(("google", "GEMINI_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("opencode"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("vim"), None);
    }

    #[test]
    fn cli_credential_policy_validates_oauth_file_map_shape() {
        assert!(
            ContainerCliCredentialPolicy::validate_oauth_file_map(&serde_json::json!({
                "auth.json": "{}",
                "credentials": "token"
            }))
            .is_ok()
        );

        let non_object =
            ContainerCliCredentialPolicy::validate_oauth_file_map(&serde_json::json!(["token"])).unwrap_err();
        let empty = ContainerCliCredentialPolicy::validate_oauth_file_map(&serde_json::json!({})).unwrap_err();
        let typed = ContainerCliCredentialPolicy::validate_oauth_file_map(&serde_json::json!({
            "auth.json": 123
        }))
        .unwrap_err();

        assert!(matches!(non_object.kind, ErrorKind::Validation(message) if message.contains("JSON object mapping")));
        assert!(matches!(empty.kind, ErrorKind::Validation(message) if message.contains("must not be empty")));
        assert!(matches!(typed.kind, ErrorKind::Validation(message) if message.contains("got number")));
    }

    #[test]
    fn cli_credential_domain_owns_file_map_plaintext_serialization() {
        let files = serde_json::json!({
            "auth.json": "{\"tokens\":{\"access_token\":\"x\"}}",
            "credentials": "token",
        });

        let plaintext = container_cli_oauth_file_map_plaintext(&files).expect("files serializes");
        let roundtrip: serde_json::Value = serde_json::from_str(&plaintext).expect("plaintext remains JSON");
        assert_eq!(roundtrip, files);
    }

    #[test]
    fn llm_provider_policy_rejects_unknown_provider() {
        let err = LlmProviderPolicy::normalize_supported_provider("bogus").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Validation(_)));
    }

    #[test]
    fn llm_provider_policy_normalizes_aliases() {
        assert_eq!(LlmProviderPolicy::normalize_supported_provider("Lite_LLM").unwrap(), "litellm");
        assert_eq!(LlmProviderPolicy::normalize_supported_provider("openai-compatible").unwrap(), "openai_compatible");
    }

    #[test]
    fn llm_provider_policy_reports_keyless_and_base_url_requirements() {
        assert!(!LlmProviderPolicy::requires_api_key("ollama"));
        assert!(LlmProviderPolicy::requires_api_key("openai"));
        assert!(LlmProviderPolicy::requires_api_key("litellm"));
        assert!(!LlmProviderPolicy::requires_base_url("litellm"));
        assert!(LlmProviderPolicy::requires_base_url("openai_compatible"));
    }

    #[test]
    fn llm_provider_policy_api_key_prefix_is_short_and_secret_safe() {
        assert_eq!(LlmProviderPolicy::api_key_prefix("sk-1234567890"), "sk-12345");
    }

    #[test]
    fn llm_provider_create_draft_trims_and_defaults_display_name() {
        let draft = LlmProviderPolicy::create_draft(
            " OpenAI ".to_string(),
            " gpt-5.5 ".to_string(),
            Some(" ".to_string()),
            Some(" sk-secret ".to_string()),
            Some(" ".to_string()),
        )
        .unwrap();

        assert_eq!(draft.provider, "openai");
        assert_eq!(draft.model, "gpt-5.5");
        assert_eq!(draft.display_name, "OpenAI");
        assert_eq!(draft.api_key.as_deref(), Some("sk-secret"));
        assert_eq!(draft.base_url, None);
    }

    #[test]
    fn llm_provider_create_draft_preserves_keyless_provider_contract() {
        let draft = LlmProviderPolicy::create_draft(
            "ollama".to_string(),
            " llama3 ".to_string(),
            None,
            Some(" ".to_string()),
            Some(" http://ollama:11434 ".to_string()),
        )
        .unwrap();

        assert_eq!(draft.provider, "ollama");
        assert_eq!(draft.api_key, None);
        assert_eq!(draft.base_url.as_deref(), Some("http://ollama:11434"));
    }

    #[test]
    fn llm_provider_create_draft_rejects_missing_required_fields() {
        assert!(
            LlmProviderPolicy::create_draft("openai".to_string(), " ".to_string(), None, Some("sk".to_string()), None)
                .is_err()
        );
        assert!(
            LlmProviderPolicy::create_draft("openai".to_string(), "gpt-5.5".to_string(), None, None, None).is_err()
        );
        assert!(
            LlmProviderPolicy::create_draft(
                "openai_compatible".to_string(),
                "custom".to_string(),
                None,
                Some("sk".to_string()),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn llm_provider_update_draft_merges_patch_with_current_values() {
        let draft = LlmProviderPolicy::update_draft(
            "openai",
            Some("gpt-4o".to_string()),
            Some("OpenAI primary".to_string()),
            None,
            Some(false),
            Some(" gpt-5.5 ".to_string()),
            Some(" ".to_string()),
            Some(" sk-new ".to_string()),
            Some(" ".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(draft.model, "gpt-5.5");
        assert_eq!(draft.display_name, "OpenAI primary");
        assert_eq!(draft.api_key.as_deref(), Some("sk-new"));
        assert_eq!(draft.base_url, None);
        assert!(!draft.is_enabled);
    }

    #[test]
    fn llm_provider_update_draft_rejects_base_url_required_provider_without_url() {
        let err = LlmProviderPolicy::update_draft(
            "openai_compatible",
            Some("custom".to_string()),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(
            matches!(err.kind, ErrorKind::Validation(message) if message == "baseUrl is required for this provider")
        );
    }

    #[test]
    fn llm_provider_policy_owns_test_and_encryption_error_contracts() {
        assert_eq!(LlmProviderPolicy::required_test_model(Some(" gpt-5.5 ")).unwrap(), "gpt-5.5");
        assert!(LlmProviderPolicy::required_test_model(Some(" ")).is_err());
        assert!(format!("{}", LlmProviderPolicy::provider_model_conflict()).contains("provider/model already exists"));
        assert!(format!("{}", LlmProviderPolicy::missing_test_api_key()).contains("cannot test stored API keys"));
        assert!(
            format!("{}", LlmProviderPolicy::missing_storage_key()).contains("refusing to store plaintext API keys")
        );
    }

    #[test]
    fn supported_provider_shape_contains_all_frontend_keys() {
        let providers = supported_provider_list();
        let keys: Vec<_> = providers.iter().map(|provider| provider.provider).collect();
        assert_eq!(
            keys,
            vec![
                "anthropic",
                "openai",
                "google",
                "ollama",
                "groq",
                "deepseek",
                "xai",
                "openrouter",
                "together",
                "fireworks",
                "litellm",
                "openai_compatible",
            ]
        );
        assert!(providers.iter().all(|provider| provider.allow_custom_models || !provider.models.is_empty()));

        let litellm =
            providers.iter().find(|provider| provider.provider == "litellm").expect("litellm supported provider");
        assert_eq!(litellm.default_base_url, Some("http://litellm:4000"));
        assert_eq!(litellm.default_model, Some("gpt-4o-mini"));

        let custom = providers
            .iter()
            .find(|provider| provider.provider == "openai_compatible")
            .expect("generic OpenAI-compatible provider");
        assert_eq!(custom.default_base_url, None);
        assert_eq!(custom.default_model, None);
    }

    #[test]
    fn llm_provider_response_owns_provider_envelope() {
        let provider = LlmProviderConfigResponse {
            id: Uuid::from_u128(0x66666666666646668666666666666666),
            provider: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            model: "gpt-5.5".to_string(),
            base_url: None,
            api_key_prefix: Some("sk-12345".to_string()),
            priority: 1,
            is_enabled: true,
            is_default: true,
            last_test_status: Some("passed".to_string()),
            last_test_error_code: None,
            last_test_error_message: None,
            last_tested_at: Some("2026-05-20T13:00:00Z".to_string()),
        };

        let single = llm_provider_response(&provider);
        let list = llm_provider_list_response(std::slice::from_ref(&provider));

        assert_eq!(single["ok"], true);
        assert_eq!(single["provider"]["model"], "gpt-5.5");
        assert_eq!(single["provider"]["apiKeyPrefix"], "sk-12345");
        assert_eq!(list["providers"][0], single["provider"]);
    }

    #[test]
    fn llm_provider_test_error_payload_redacts_upstream_body() {
        let result = LlmProviderTestResult::from_llm_error(&LlmError::Api {
            status: 401,
            message: "invalid key sk-secret-value".to_string(),
        });
        let payload = llm_provider_test_response(result);

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["code"], "unauthorized");
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(!message.contains("sk-secret-value"));
    }

    #[test]
    fn llm_provider_test_success_response_limits_preview() {
        let usage = Usage { input_tokens: 10, output_tokens: 4 };
        let result = LlmProviderTestResult::success(
            Uuid::from_u128(0x77777777777747778777777777777777),
            "openai",
            "gpt-5.5",
            &"x".repeat(140),
            Some(usage),
        );
        let body = llm_provider_test_response(result);

        assert_eq!(body["ok"], true);
        assert_eq!(body["provider"]["provider"], "openai");
        assert_eq!(body["responsePreview"].as_str().unwrap().len(), 120);
        assert_eq!(body["usage"]["input_tokens"], 10);
    }

    #[test]
    fn oauth_mount_container_key_rejects_path_escape() {
        assert_eq!(OauthMountContainerKey::parse("container-xyz").unwrap().value(), "container-xyz");
        assert!(OauthMountContainerKey::parse("").is_err());
        assert!(OauthMountContainerKey::parse("..").is_err());
        assert!(OauthMountContainerKey::parse(".").is_err());
        assert!(OauthMountContainerKey::parse("a/b").is_err());
        assert!(OauthMountContainerKey::parse("a\\b").is_err());
    }

    #[test]
    fn git_credential_draft_trims_name_and_validates_enums() {
        let draft = GitCredentialDraft::parse("  GitHub  ", "github", "token").unwrap();
        assert_eq!(draft.name(), "GitHub");
        assert_eq!(draft.provider(), "github");
        assert_eq!(draft.credential_type(), "token");

        assert!(GitCredentialDraft::parse("", "github", "token").is_err());
        assert!(GitCredentialDraft::parse(&"a".repeat(256), "github", "token").is_err());
        assert!(GitCredentialDraft::parse("GitHub", "azure", "token").is_err());
        assert!(GitCredentialDraft::parse("GitHub", "github", "password").is_err());
    }

    #[test]
    fn git_credential_draft_accepts_existing_provider_and_type_sets() {
        for provider in ["github", "gitlab", "bitbucket", "custom"] {
            assert!(GitCredentialDraft::parse("name", provider, "token").is_ok());
        }
        for credential_type in ["token", "ssh", "oauth"] {
            assert!(GitCredentialDraft::parse("name", "github", credential_type).is_ok());
        }
    }

    #[test]
    fn git_credential_token_trims_and_rejects_empty_values() {
        assert_eq!(GitCredentialToken::parse("  ghp-secret  ").map(GitCredentialToken::value), Some("ghp-secret"));
        assert_eq!(GitCredentialToken::parse("  "), None);
    }

    #[test]
    fn credential_encryption_policies_own_user_visible_error_messages() {
        assert!(format!("{}", ContainerCliCredentialPolicy::missing_storage_key()).contains("plaintext credentials"));
        assert!(format!("{}", ContainerCliCredentialPolicy::serialize_files_failed("bad")).contains("serialize files"));
        assert!(
            format!("{}", ContainerCliCredentialPolicy::encrypt_credentials_failed("bad"))
                .contains("failed to encrypt credentials")
        );
        assert!(
            format!("{}", ContainerCliCredentialPolicy::stored_user_llm_key_decrypt_failed())
                .contains("stored user LLM API key failed to decrypt")
        );
        assert!(
            format!("{}", ContainerCliCredentialPolicy::stored_oauth_decrypt_failed("codex"))
                .contains("stored codex credentials cannot be decrypted")
        );
        assert!(format!("{}", GitCredentialEncryptionPolicy::missing_decrypt_key()).contains("cannot decrypt"));
        assert!(format!("{}", GitCredentialEncryptionPolicy::missing_storage_key()).contains("plaintext git tokens"));
        assert!(
            format!("{}", GitCredentialEncryptionPolicy::encrypt_failed("bad"))
                .contains("encrypt git credential token failed")
        );
        assert!(
            format!("{}", GitCredentialEncryptionPolicy::ciphertext_not_utf8("github", "bad utf8"))
                .contains("stored github git credential ciphertext is not UTF-8")
        );
        assert!(
            format!("{}", GitCredentialEncryptionPolicy::decrypt_failed("gitlab"))
                .contains("stored gitlab git credential cannot be decrypted")
        );
        assert!(
            format!("{}", LlmProviderPolicy::decrypt_api_key_failed("bad"))
                .contains("decrypt llm provider api key failed")
        );
        assert!(
            format!("{}", LlmProviderPolicy::encrypt_api_key_failed("bad"))
                .contains("encrypt llm provider api key failed")
        );
    }

    #[test]
    fn credential_repository_policy_owns_lookup_errors() {
        let id = Uuid::new_v4();

        assert!(matches!(
            CredentialRepositoryPolicy::api_key_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("api key {id}")
        ));
        assert!(matches!(
            CredentialRepositoryPolicy::git_credential_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("git credential {id}")
        ));
        assert!(matches!(
            CredentialRepositoryPolicy::ssh_key_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("ssh key {id}")
        ));
        assert!(matches!(
            CredentialRepositoryPolicy::llm_provider_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("llm provider {id}")
        ));
    }

    #[test]
    fn git_remote_host_accepts_common_git_url_shapes() {
        assert_eq!(GitRemoteHost::normalize(None, "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("github.com/Wisdoverse/repo"), "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("git@github.com:Wisdoverse/repo.git"), "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("https://GitHub.com/Wisdoverse/repo"), "github.com"), "github.com");
        assert_eq!(
            GitRemoteHost::normalize(Some("git+ssh://git@gitlab.internal.example/group/repo"), "gitlab.com"),
            "gitlab.internal.example"
        );
    }
}
