//! LLM provider settings endpoints (nested under `/api/v1`).
//!
//! These routes expose the user-owned `user_llm_configs` table to the Settings
//! UI without serializing stored API keys.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use std::time::Duration;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind, crypto};
use agentforge_llm::{
    ChatMessage, ChatRequest, LlmError, LlmProviderBuildConfig, normalize_provider_key, provider_spec,
    supported_provider_specs,
};

use crate::health::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderModelInfo {
    model: &'static str,
    display_name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInfo {
    provider: &'static str,
    display_name: &'static str,
    default_model: Option<&'static str>,
    default_base_url: Option<&'static str>,
    requires_api_key: bool,
    allow_custom_models: bool,
    models: Vec<ProviderModelInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LlmProviderConfigResponse {
    id: Uuid,
    provider: String,
    display_name: String,
    model: String,
    base_url: Option<String>,
    api_key_prefix: Option<String>,
    priority: i32,
    is_enabled: bool,
    is_default: bool,
    last_test_status: Option<String>,
    last_test_error_code: Option<String>,
    last_test_error_message: Option<String>,
    last_tested_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct LlmProviderRow {
    id: Uuid,
    provider: String,
    model: Option<String>,
    display_name: Option<String>,
    base_url: Option<String>,
    api_key_prefix: Option<String>,
    is_enabled: Option<bool>,
    is_default: Option<bool>,
    last_test_status: Option<String>,
    last_test_error_code: Option<String>,
    last_test_error_message: Option<String>,
    last_tested_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct LlmProviderTestRow {
    id: Uuid,
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    encrypted_api_key: String,
    is_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequest {
    pub provider: String,
    pub display_name: Option<String>,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderRequest {
    pub display_name: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub is_enabled: Option<bool>,
}

fn provider_display_name(provider: &str) -> &'static str {
    provider_spec(provider).map(|spec| spec.display_name).unwrap_or("Custom")
}

fn supported_provider_list() -> Vec<ProviderInfo> {
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

fn clean_optional(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn validate_provider(provider: &str) -> AppResult<String> {
    let provider = normalize_provider_key(provider);
    if provider_spec(&provider).is_none() {
        return Err(ErrorKind::Validation(format!("invalid provider '{provider}'")).into());
    }
    Ok(provider)
}

fn api_key_prefix(api_key: &str) -> String {
    api_key.chars().take(8).collect()
}

fn provider_requires_api_key(provider: &str) -> bool {
    provider_spec(provider).map(|spec| spec.requires_api_key).unwrap_or(true)
}

fn provider_requires_base_url(provider: &str) -> bool {
    provider_spec(provider)
        .map(|spec| spec.key == "openai_compatible" && spec.default_base_url.is_none())
        .unwrap_or(false)
}

fn response_from_row(row: LlmProviderRow, priority: i32) -> LlmProviderConfigResponse {
    let provider = row.provider;
    let display_name = row.display_name.unwrap_or_else(|| provider_display_name(provider.as_str()).to_string());

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

async fn fetch_provider_row(state: &AppState, auth: &AuthUser, id: Uuid) -> AppResult<LlmProviderRow> {
    sqlx::query_as::<_, LlmProviderRow>(
        r#"SELECT id,
                  provider,
                  model,
                  display_name,
                  base_url,
                  api_key_prefix,
                  is_enabled,
                  is_default,
                  settings -> 'connection_test' ->> 'status' AS last_test_status,
                  settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                  settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                  settings -> 'connection_test' ->> 'tested_at' AS last_tested_at
           FROM user_llm_configs
          WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(auth.scope.user_id().as_uuid())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ErrorKind::NotFound(format!("llm provider {id}")).into())
}

async fn fetch_provider_test_row(state: &AppState, auth: &AuthUser, id: Uuid) -> AppResult<LlmProviderTestRow> {
    sqlx::query_as::<_, LlmProviderTestRow>(
        r#"SELECT id, provider, model, base_url, encrypted_api_key, is_enabled
           FROM user_llm_configs
          WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(auth.scope.user_id().as_uuid())
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ErrorKind::NotFound(format!("llm provider {id}")).into())
}

/// `GET /api/v1/llm-providers/supported` — static UI provider metadata.
async fn get_supported_providers() -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "providers": supported_provider_list(),
    }))
}

/// `GET /api/v1/llm-providers` — list user provider configs.
async fn list_providers(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let rows = sqlx::query_as::<_, LlmProviderRow>(
        r#"SELECT id,
                  provider,
                  model,
                  display_name,
                  base_url,
                  api_key_prefix,
                  is_enabled,
                  is_default,
                  settings -> 'connection_test' ->> 'status' AS last_test_status,
                  settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                  settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                  settings -> 'connection_test' ->> 'tested_at' AS last_tested_at
           FROM user_llm_configs
          WHERE user_id = $1
          ORDER BY COALESCE(is_default, false) DESC, updated_at DESC NULLS LAST, created_at DESC NULLS LAST"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .fetch_all(&state.pool)
    .await?;

    let providers: Vec<_> = rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| response_from_row(row, i32::try_from(idx + 1).unwrap_or(i32::MAX)))
        .collect();

    Ok(Json(json!({ "ok": true, "providers": providers })))
}

/// `POST /api/v1/llm-providers` — create a user provider config.
async fn create_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let provider = validate_provider(&req.provider)?;
    let model = req.model.trim();
    if model.is_empty() {
        return Err(ErrorKind::Validation("model is required".into()).into());
    }

    let api_key = req.api_key.as_deref().unwrap_or_default().trim();
    if provider_requires_api_key(&provider) && api_key.is_empty() {
        return Err(ErrorKind::Validation("apiKey is required".into()).into());
    }
    let base_url = clean_optional(req.base_url);
    if provider_requires_base_url(&provider) && base_url.is_none() {
        return Err(ErrorKind::Validation("baseUrl is required for this provider".into()).into());
    }

    let (encrypted_api_key, prefix) = if api_key.is_empty() {
        (String::new(), None)
    } else {
        let key = state.encryption_key.as_ref().ok_or_else(|| {
            ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - refusing to store plaintext API keys".into())
        })?;
        let encrypted_api_key = crypto::encrypt_base64(key, api_key)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encrypt llm provider api key failed: {err}")))?;
        (encrypted_api_key, Some(api_key_prefix(api_key)))
    };

    let exists = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
                 FROM user_llm_configs
                WHERE user_id = $1
                  AND provider = $2
                  AND model = $3
           )"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .bind(&provider)
    .bind(model)
    .fetch_one(&state.pool)
    .await?;
    if exists {
        return Err(ErrorKind::Conflict("provider/model already exists".into()).into());
    }

    let should_be_default = sqlx::query_scalar::<_, bool>(
        r#"SELECT NOT EXISTS (
               SELECT 1
                 FROM user_llm_configs
                WHERE user_id = $1
                  AND provider = $2
                  AND COALESCE(is_default, false) = true
           )"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .bind(&provider)
    .fetch_one(&state.pool)
    .await?;

    let display_name =
        clean_optional(req.display_name).unwrap_or_else(|| provider_display_name(provider.as_str()).to_string());
    let row = sqlx::query_as::<_, LlmProviderRow>(
        r#"INSERT INTO user_llm_configs
              (user_id, provider, model, display_name, base_url, api_key_prefix, encrypted_api_key, is_enabled, is_default, settings)
           VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, '{}'::jsonb)
           RETURNING id,
                     provider,
                     model,
                     display_name,
                     base_url,
                     api_key_prefix,
                     is_enabled,
                     is_default,
                     settings -> 'connection_test' ->> 'status' AS last_test_status,
                     settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                     settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                     settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .bind(provider)
    .bind(model)
    .bind(display_name)
    .bind(base_url)
    .bind(prefix)
    .bind(encrypted_api_key)
    .bind(should_be_default)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "provider": response_from_row(row, 1) })))
}

/// `PATCH /api/v1/llm-providers/{id}` — update non-secret metadata and optional API key.
async fn update_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let current = fetch_provider_row(&state, &auth, id).await?;
    let model =
        clean_optional(req.model).or(current.model).ok_or_else(|| ErrorKind::Validation("model is required".into()))?;
    let display_name = clean_optional(req.display_name)
        .or(current.display_name)
        .unwrap_or_else(|| provider_display_name(current.provider.as_str()).to_string());
    let base_url = clean_optional(req.base_url).or(current.base_url);
    if provider_requires_base_url(&current.provider) && base_url.is_none() {
        return Err(ErrorKind::Validation("baseUrl is required for this provider".into()).into());
    }
    let is_enabled = req.is_enabled.unwrap_or(current.is_enabled.unwrap_or(true));

    let encrypted_and_prefix = if let Some(api_key) =
        req.api_key.as_deref().map(str::trim).filter(|value| !value.is_empty())
    {
        let key = state.encryption_key.as_ref().ok_or_else(|| {
            ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - refusing to store plaintext API keys".into())
        })?;
        let encrypted_api_key = crypto::encrypt_base64(key, api_key)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encrypt llm provider api key failed: {err}")))?;
        Some((encrypted_api_key, api_key_prefix(api_key)))
    } else {
        None
    };

    let row = if let Some((encrypted_api_key, prefix)) = encrypted_and_prefix {
        sqlx::query_as::<_, LlmProviderRow>(
            r#"UPDATE user_llm_configs
                  SET model = $1,
                      display_name = $2,
                      base_url = $3,
                      is_enabled = $4,
                      encrypted_api_key = $5,
                      api_key_prefix = $6,
                      settings = COALESCE(settings, '{}'::jsonb) - 'connection_test',
                      updated_at = now()
                WHERE id = $7 AND user_id = $8
            RETURNING id,
                      provider,
                      model,
                      display_name,
                      base_url,
                      api_key_prefix,
                      is_enabled,
                      is_default,
                      settings -> 'connection_test' ->> 'status' AS last_test_status,
                      settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                      settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                      settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
        )
        .bind(model)
        .bind(display_name)
        .bind(base_url)
        .bind(is_enabled)
        .bind(encrypted_api_key)
        .bind(prefix)
        .bind(id)
        .bind(auth.scope.user_id().as_uuid())
        .fetch_one(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, LlmProviderRow>(
            r#"UPDATE user_llm_configs
                  SET model = $1,
                      display_name = $2,
                      base_url = $3,
                      is_enabled = $4,
                      settings = COALESCE(settings, '{}'::jsonb) - 'connection_test',
                      updated_at = now()
                WHERE id = $5 AND user_id = $6
            RETURNING id,
                      provider,
                      model,
                      display_name,
                      base_url,
                      api_key_prefix,
                      is_enabled,
                      is_default,
                      settings -> 'connection_test' ->> 'status' AS last_test_status,
                      settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                      settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                      settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
        )
        .bind(model)
        .bind(display_name)
        .bind(base_url)
        .bind(is_enabled)
        .bind(id)
        .bind(auth.scope.user_id().as_uuid())
        .fetch_one(&state.pool)
        .await?
    };

    Ok(Json(json!({ "ok": true, "provider": response_from_row(row, 1) })))
}

/// `DELETE /api/v1/llm-providers/{id}` — remove a user provider config.
async fn delete_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query("DELETE FROM user_llm_configs WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.scope.user_id().as_uuid())
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ErrorKind::NotFound(format!("llm provider {id}")).into());
    }

    Ok(Json(json!({ "ok": true })))
}

/// `POST /api/v1/llm-providers/{id}/default` — mark provider as default for its provider key.
async fn set_default_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let current = fetch_provider_row(&state, &auth, id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        r#"UPDATE user_llm_configs
              SET is_default = false, updated_at = now()
            WHERE user_id = $1 AND provider = $2"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .bind(&current.provider)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query_as::<_, LlmProviderRow>(
        r#"UPDATE user_llm_configs
              SET is_default = true, updated_at = now()
            WHERE id = $1 AND user_id = $2
        RETURNING id,
                  provider,
                  model,
                  display_name,
                  base_url,
                  api_key_prefix,
                  is_enabled,
                  is_default,
                  settings -> 'connection_test' ->> 'status' AS last_test_status,
                  settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                  settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                  settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
    )
    .bind(id)
    .bind(auth.scope.user_id().as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "ok": true, "provider": response_from_row(row, 1) })))
}

fn llm_test_error_payload(error: &LlmError) -> serde_json::Value {
    let (code, message, retryable) = llm_test_error_parts(error);

    json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
            "retryable": retryable,
        },
    })
}

fn llm_test_error_parts(error: &LlmError) -> (&'static str, &'static str, bool) {
    let (code, message, retryable) = match error {
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
    };

    (code, message, retryable)
}

async fn record_provider_test_result(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE user_llm_configs
              SET settings = jsonb_set(
                    COALESCE(settings, '{}'::jsonb),
                    '{connection_test}',
                    jsonb_build_object(
                      'status', $3::text,
                      'tested_at', to_jsonb(now()),
                      'error_code', $4::text,
                      'error_message', $5::text
                    ),
                    true
                  ),
                  updated_at = now()
            WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(auth.scope.user_id().as_uuid())
    .bind(status)
    .bind(error_code)
    .bind(error_message)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// `POST /api/v1/llm-providers/{id}/test` — send a tiny real request through the Rust LLM gateway.
async fn test_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let provider = fetch_provider_test_row(&state, &auth, id).await?;
    if !provider.is_enabled.unwrap_or(true) {
        record_provider_test_result(&state, &auth, id, "failed", Some("disabled"), Some("Provider is disabled."))
            .await?;
        return Ok(Json(json!({
            "ok": false,
            "error": {
                "code": "disabled",
                "message": "Provider is disabled.",
                "retryable": false,
            },
        })));
    }

    let model = provider
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| ErrorKind::Validation("model is required before testing a provider".into()))?
        .to_string();

    let api_key = if provider.provider == "ollama" {
        String::new()
    } else {
        let key = state.encryption_key.as_ref().ok_or_else(|| {
            ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - cannot test stored API keys".into())
        })?;
        crypto::decrypt_base64(key, &provider.encrypted_api_key)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("decrypt llm provider api key failed: {err}")))?
    };

    let provider_instance = match state.llm_factory.build_with_config(LlmProviderBuildConfig {
        provider_key: provider.provider.clone(),
        api_key,
        base_url: provider.base_url.clone(),
    }) {
        Ok(instance) => instance,
        Err(error) => {
            let (code, message, _) = llm_test_error_parts(&error);
            record_provider_test_result(&state, &auth, id, "failed", Some(code), Some(message)).await?;
            return Ok(Json(llm_test_error_payload(&error)));
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
            record_provider_test_result(&state, &auth, id, "passed", None, None).await?;
            Ok(Json(json!({
                "ok": true,
                "provider": {
                    "id": provider.id,
                    "provider": provider.provider,
                    "model": model,
                },
                "responsePreview": response.content.chars().take(120).collect::<String>(),
                "usage": response.usage,
            })))
        }
        Ok(Err(error)) => {
            let (code, message, _) = llm_test_error_parts(&error);
            record_provider_test_result(&state, &auth, id, "failed", Some(code), Some(message)).await?;
            Ok(Json(llm_test_error_payload(&error)))
        }
        Err(_) => {
            record_provider_test_result(
                &state,
                &auth,
                id,
                "failed",
                Some("timeout"),
                Some("Provider connection test timed out."),
            )
            .await?;
            Ok(Json(json!({
                "ok": false,
                "error": {
                    "code": "timeout",
                    "message": "Provider connection test timed out.",
                    "retryable": true,
                },
            })))
        }
    }
}

/// Build LLM provider routes sub-router.
pub fn llm_provider_routes() -> Router<AppState> {
    Router::new()
        .route("/llm-providers/supported", get(get_supported_providers))
        .route("/llm-providers", get(list_providers).post(create_provider))
        .route("/llm-providers/{id}", get(get_provider).patch(update_provider).delete(delete_provider))
        .route("/llm-providers/{id}/default", axum::routing::post(set_default_provider))
        .route("/llm-providers/{id}/test", axum::routing::post(test_provider))
}

/// `GET /api/v1/llm-providers/{id}` — read one provider config.
async fn get_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let row = fetch_provider_row(&state, &auth, id).await?;
    Ok(Json(json!({ "ok": true, "provider": response_from_row(row, 1) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_uses_camel_case_contract() {
        let req: CreateProviderRequest = serde_json::from_str(
            r#"{"provider":"anthropic","displayName":"Claude","model":"claude-sonnet-4-20250514","apiKey":"sk-ant"}"#,
        )
        .unwrap();
        assert_eq!(req.provider, "anthropic");
        assert_eq!(req.display_name.as_deref(), Some("Claude"));
    }

    #[test]
    fn invalid_provider_is_rejected() {
        let err = validate_provider("bogus").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Validation(_)));
    }

    #[test]
    fn provider_aliases_are_normalized() {
        assert_eq!(validate_provider("Lite_LLM").unwrap(), "litellm");
        assert_eq!(validate_provider("openai-compatible").unwrap(), "openai_compatible");
    }

    #[test]
    fn api_key_prefix_is_short_and_secret_safe() {
        assert_eq!(api_key_prefix("sk-1234567890"), "sk-12345");
    }

    #[test]
    fn ollama_is_keyless_provider() {
        assert!(!provider_requires_api_key("ollama"));
        assert!(provider_requires_api_key("openai"));
        assert!(provider_requires_api_key("litellm"));
    }

    #[test]
    fn generic_openai_compatible_requires_explicit_base_url() {
        assert!(!provider_requires_base_url("litellm"));
        assert!(provider_requires_base_url("openai_compatible"));
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
    fn provider_test_error_payload_redacts_upstream_body() {
        let payload =
            llm_test_error_payload(&LlmError::Api { status: 401, message: "invalid key sk-secret-value".to_string() });

        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error"]["code"], "unauthorized");
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(!message.contains("sk-secret-value"));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn create_ollama_provider_accepts_empty_api_key(pool: sqlx::PgPool) {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt;

        let seed = crate::test_support::seed_provider_agent(&pool, "openai", "gpt-5.5").await;
        let app = crate::test_support::test_app_with_mock_provider(pool.clone(), "openai", "connection ok").await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/llm-providers")
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"provider":"ollama","displayName":"Local Ollama","model":"llama3","apiKey":"","baseUrl":"http://ollama:11434"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["provider"]["provider"], "ollama");
        assert_eq!(body["provider"]["apiKeyPrefix"], serde_json::Value::Null);

        let stored: (String, Option<String>) = sqlx::query_as(
            "SELECT encrypted_api_key, api_key_prefix FROM user_llm_configs WHERE user_id = $1 AND provider = 'ollama'",
        )
        .bind(seed.user_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("stored ollama provider");
        assert_eq!(stored.0, "");
        assert_eq!(stored.1, None);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn provider_test_route_calls_llm_gateway(pool: sqlx::PgPool) {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt;

        let seed = crate::test_support::seed_provider_agent(&pool, "openai", "gpt-5.5").await;
        let provider_id: Uuid =
            sqlx::query_scalar("SELECT id FROM user_llm_configs WHERE user_id = $1 AND provider = 'openai' LIMIT 1")
                .bind(seed.user_id.as_uuid())
                .fetch_one(&pool)
                .await
                .expect("seeded provider id");
        let query_pool = pool.clone();
        let app = crate::test_support::test_app_with_mock_provider(pool, "openai", "connection ok").await;

        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/llm-providers/{provider_id}/test"))
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["provider"]["provider"], "openai");
        assert_eq!(body["provider"]["model"], "gpt-5.5");
        assert_eq!(body["responsePreview"], "connection ok");

        let status: Option<String> = sqlx::query_scalar(
            "SELECT settings -> 'connection_test' ->> 'status'
               FROM user_llm_configs
              WHERE id = $1",
        )
        .bind(provider_id)
        .fetch_one(&query_pool)
        .await
        .expect("stored provider test status");
        assert_eq!(status.as_deref(), Some("passed"));
    }
}
