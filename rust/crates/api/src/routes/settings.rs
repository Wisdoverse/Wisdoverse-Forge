//! Settings endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/settings`          — list settings for user/org
//! - `GET    /api/v1/settings/runtime`  — runtime defaults
//! - `PATCH  /api/v1/settings/runtime`  — update runtime defaults
//! - `GET    /api/v1/settings/gateway`  — gateway defaults
//! - `PATCH  /api/v1/settings/gateway`  — update gateway defaults
//! - `PUT    /api/v1/settings/{key}`    — upsert setting
//! - `DELETE /api/v1/settings/{key}`    — delete setting

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::setting::{
    SettingService, UpdateGatewaySettingsInput, UpdateRuntimeSettingsInput, UpsertSettingInput,
    configuration_data_response, configuration_delete_response, gateway_settings_response,
    runtime_settings_with_cli_tools_response,
};

/// Request body for upserting a setting.
#[derive(Deserialize)]
pub struct UpsertSettingRequest {
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRuntimeSettingsRequest {
    default_runtime: Option<String>,
    default_cli_tool: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateGatewaySettingsRequest {
    routing_strategy: Option<String>,
    circuit_breaker_threshold: Option<u32>,
    circuit_breaker_reset_ms: Option<u32>,
}

/// Build a SettingService from shared state.
fn make_service(state: &AppState) -> SettingService {
    state.setting_service()
}

/// `GET /api/settings` — list settings.
async fn list_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let settings = service.list(&auth.scope).await?;
    Ok(Json(configuration_data_response(settings)))
}

/// `GET /api/settings/runtime` — read runtime settings.
async fn get_runtime_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let runtime = service.runtime_settings_with_cli_tools(&auth.scope).await?;
    Ok(Json(runtime_settings_with_cli_tools_response(&runtime)))
}

/// `PATCH /api/settings/runtime` — update runtime settings.
async fn update_runtime_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateRuntimeSettingsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let runtime = service
        .update_runtime_settings_with_cli_tools(
            &auth.scope,
            UpdateRuntimeSettingsInput { default_runtime: req.default_runtime, default_cli_tool: req.default_cli_tool },
        )
        .await?;
    Ok(Json(runtime_settings_with_cli_tools_response(&runtime)))
}

/// `GET /api/settings/gateway` — read gateway settings.
async fn get_gateway_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let gateway = service.gateway_settings(&auth.scope).await?;
    Ok(Json(gateway_settings_response(&gateway)))
}

/// `PATCH /api/settings/gateway` — update gateway settings.
async fn update_gateway_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateGatewaySettingsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let gateway = service
        .update_gateway_settings(
            &auth.scope,
            UpdateGatewaySettingsInput {
                routing_strategy: req.routing_strategy,
                circuit_breaker_threshold: req.circuit_breaker_threshold,
                circuit_breaker_reset_ms: req.circuit_breaker_reset_ms,
            },
        )
        .await?;
    Ok(Json(gateway_settings_response(&gateway)))
}

/// `PUT /api/settings/{key}` — upsert setting.
async fn upsert_setting(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
    Json(req): Json<UpsertSettingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let setting = service.upsert(&auth.scope, &key, UpsertSettingInput { value: req.value }).await?;
    Ok(Json(configuration_data_response(setting)))
}

/// `DELETE /api/settings/{key}` — delete setting.
async fn delete_setting(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, &key).await?;
    Ok(Json(configuration_delete_response()))
}

/// Build settings routes sub-router.
pub fn setting_routes() -> Router<AppState> {
    Router::new()
        .route("/settings/runtime", get(get_runtime_settings).patch(update_runtime_settings))
        .route("/settings/gateway", get(get_gateway_settings).patch(update_gateway_settings))
        .route("/settings", get(list_settings))
        .route("/settings/{key}", put(upsert_setting).delete(delete_setting))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_request_deserialization() {
        let req: UpsertSettingRequest = serde_json::from_str(r#"{"value": {"theme": "dark"}}"#).unwrap();
        assert_eq!(req.value["theme"], "dark");
    }

    #[test]
    fn upsert_request_with_scalar_value() {
        let req: UpsertSettingRequest = serde_json::from_str(r#"{"value": 42}"#).unwrap();
        assert_eq!(req.value, serde_json::json!(42));
    }

    #[test]
    fn upsert_request_with_null_value() {
        let req: UpsertSettingRequest = serde_json::from_str(r#"{"value": null}"#).unwrap();
        assert!(req.value.is_null());
    }

    #[test]
    fn runtime_update_request_deserializes_legacy_field_names() {
        let req: UpdateRuntimeSettingsRequest = serde_json::from_str(r#"{"defaultRuntime": "legacy"}"#).unwrap();
        assert_eq!(req.default_runtime.as_deref(), Some("legacy"));
    }

    #[test]
    fn gateway_update_request_deserializes_legacy_field_names() {
        let req: UpdateGatewaySettingsRequest =
            serde_json::from_str(r#"{"routingStrategy": "latency", "circuitBreakerThreshold": 10}"#).unwrap();
        assert_eq!(req.routing_strategy.as_deref(), Some("latency"));
        assert_eq!(req.circuit_breaker_threshold, Some(10));
    }
}
