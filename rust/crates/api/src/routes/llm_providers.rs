//! LLM provider settings endpoints (nested under `/api/v1`).
//!
//! These routes expose the user-owned `user_llm_configs` table to the Settings
//! UI without serializing stored API keys.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::llm_provider::LlmProviderService;

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

fn make_service(state: &AppState) -> LlmProviderService {
    LlmProviderService::from_pool(state.pool.clone(), state.encryption_key, state.llm_factory.clone())
}

/// `GET /api/v1/llm-providers/supported` — static UI provider metadata.
async fn get_supported_providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(make_service(&state).supported_providers())
}

/// `GET /api/v1/llm-providers` — list user provider configs.
async fn list_providers(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(make_service(&state).list_providers(&auth.scope).await?))
}

/// `POST /api/v1/llm-providers` — create a user provider config.
async fn create_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let body = make_service(&state)
        .create_provider(&auth.scope, req.provider, req.model, req.display_name, req.api_key, req.base_url)
        .await?;
    Ok(Json(body))
}

/// `PATCH /api/v1/llm-providers/{id}` — update non-secret metadata and optional API key.
async fn update_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let body = make_service(&state)
        .update_provider(&auth.scope, id, req.model, req.display_name, req.api_key, req.base_url, req.is_enabled)
        .await?;
    Ok(Json(body))
}

/// `DELETE /api/v1/llm-providers/{id}` — remove a user provider config.
async fn delete_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(make_service(&state).delete_provider(&auth.scope, id).await?))
}

/// `POST /api/v1/llm-providers/{id}/default` — mark provider as default for its provider key.
async fn set_default_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(make_service(&state).set_default_provider(&auth.scope, id).await?))
}

/// `POST /api/v1/llm-providers/{id}/test` — send a tiny real request through the Rust LLM gateway.
async fn test_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(make_service(&state).test_provider(&auth.scope, id).await?))
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
    Ok(Json(make_service(&state).get_provider(&auth.scope, id).await?))
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
