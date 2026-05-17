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
use serde::{Deserialize, Serialize};

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind, TenantScope};

use crate::domain::configuration::{GatewaySettingsPolicy, RuntimeSettingsPolicy};
use crate::health::AppState;
use crate::repositories::setting::SettingRepository;
use crate::services::setting::SettingService;

const RUNTIME_KEY: &str = "runtime";
const GATEWAY_KEY: &str = "gateway";

/// Request body for upserting a setting.
#[derive(Deserialize)]
pub struct UpsertSettingRequest {
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettings {
    default_runtime: String,
    available_runtimes: Vec<String>,
    default_cli_tool: String,
    available_cli_tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRuntimeSettingsRequest {
    default_runtime: Option<String>,
    default_cli_tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GatewaySettings {
    routing_strategy: String,
    circuit_breaker_threshold: u32,
    circuit_breaker_reset_ms: u32,
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
    SettingService::new(SettingRepository::new(state.pool.clone()))
}

fn runtime_defaults() -> RuntimeSettings {
    RuntimeSettings {
        default_runtime: RuntimeSettingsPolicy::default_runtime().to_string(),
        available_runtimes: RuntimeSettingsPolicy::available_runtimes(),
        default_cli_tool: RuntimeSettingsPolicy::default_cli_tool().to_string(),
        available_cli_tools: RuntimeSettingsPolicy::available_cli_tools(),
    }
}

fn gateway_defaults() -> GatewaySettings {
    GatewaySettings {
        routing_strategy: GatewaySettingsPolicy::default_routing_strategy().to_string(),
        circuit_breaker_threshold: GatewaySettingsPolicy::default_circuit_breaker_threshold(),
        circuit_breaker_reset_ms: GatewaySettingsPolicy::default_circuit_breaker_reset_ms(),
    }
}

fn runtime_from_settings(scope: &TenantScope, settings: &[agentforge_db::entities::Setting]) -> RuntimeSettings {
    let mut defaults = runtime_defaults();
    let value = settings
        .iter()
        .find(|setting| setting.key == RUNTIME_KEY && setting.user_id == Some(scope.user_id()))
        .or_else(|| settings.iter().find(|setting| setting.key == RUNTIME_KEY))
        .map(|setting| &setting.value);

    if let Some(value) = value {
        if let Some(default_runtime) = value.get("defaultRuntime").and_then(serde_json::Value::as_str)
            && let Some(default_runtime) = RuntimeSettingsPolicy::runtime_from_stored(default_runtime)
        {
            defaults.default_runtime = default_runtime.to_string();
        }
        if let Some(default_cli_tool) = value.get("defaultCliTool").and_then(serde_json::Value::as_str)
            && let Some(default_cli_tool) = RuntimeSettingsPolicy::cli_tool_from_stored(default_cli_tool)
        {
            defaults.default_cli_tool = default_cli_tool.to_string();
        }
    }

    defaults
}

fn runtime_response(runtime: RuntimeSettings) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "data": &runtime,
        // Legacy cached frontends read settings fields from the top-level
        // response instead of the `data` envelope.
        "defaultRuntime": runtime.default_runtime,
        "availableRuntimes": runtime.available_runtimes,
        "defaultCliTool": runtime.default_cli_tool,
        "availableCliTools": runtime.available_cli_tools,
    })
}

fn gateway_from_settings(scope: &TenantScope, settings: &[agentforge_db::entities::Setting]) -> GatewaySettings {
    let mut defaults = gateway_defaults();
    let value = settings
        .iter()
        .find(|setting| setting.key == GATEWAY_KEY && setting.user_id == Some(scope.user_id()))
        .or_else(|| settings.iter().find(|setting| setting.key == GATEWAY_KEY))
        .map(|setting| &setting.value);

    if let Some(value) = value {
        if let Some(routing_strategy) = value.get("routingStrategy").and_then(serde_json::Value::as_str)
            && let Some(routing_strategy) = GatewaySettingsPolicy::routing_strategy_from_stored(routing_strategy)
        {
            defaults.routing_strategy = routing_strategy.to_string();
        }
        if let Some(threshold) =
            value.get("circuitBreakerThreshold").and_then(serde_json::Value::as_u64).and_then(|v| u32::try_from(v).ok())
        {
            defaults.circuit_breaker_threshold = threshold;
        }
        if let Some(reset_ms) =
            value.get("circuitBreakerResetMs").and_then(serde_json::Value::as_u64).and_then(|v| u32::try_from(v).ok())
        {
            defaults.circuit_breaker_reset_ms = reset_ms;
        }
    }

    defaults
}

fn gateway_response(gateway: GatewaySettings) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "data": &gateway,
        // Legacy cached frontends read settings fields from the top-level
        // response instead of the `data` envelope.
        "routingStrategy": gateway.routing_strategy,
        "circuitBreakerThreshold": gateway.circuit_breaker_threshold,
        "circuitBreakerResetMs": gateway.circuit_breaker_reset_ms,
    })
}

/// `GET /api/settings` — list settings.
async fn list_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let settings = service.list(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": settings })))
}

/// `GET /api/settings/runtime` — read runtime settings.
async fn get_runtime_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let settings = service.list(&auth.scope).await?;
    let runtime = runtime_from_settings(&auth.scope, &settings);
    Ok(Json(runtime_response(runtime)))
}

/// `PATCH /api/settings/runtime` — update runtime settings.
async fn update_runtime_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateRuntimeSettingsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let current = runtime_from_settings(&auth.scope, &service.list(&auth.scope).await?);
    let mut runtime = current;

    if let Some(default_runtime) = req.default_runtime {
        runtime.default_runtime = RuntimeSettingsPolicy::canonical_runtime(&default_runtime)?.to_string();
    }

    if let Some(default_cli_tool) = req.default_cli_tool {
        runtime.default_cli_tool = RuntimeSettingsPolicy::canonical_cli_tool(&default_cli_tool)?.to_string();
    }

    let value = serde_json::to_value(&runtime).map_err(|err| ErrorKind::Internal(err.into()))?;
    service.upsert(&auth.scope, RUNTIME_KEY, &value).await?;
    Ok(Json(runtime_response(runtime)))
}

/// `GET /api/settings/gateway` — read gateway settings.
async fn get_gateway_settings(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let settings = service.list(&auth.scope).await?;
    let gateway = gateway_from_settings(&auth.scope, &settings);
    Ok(Json(gateway_response(gateway)))
}

/// `PATCH /api/settings/gateway` — update gateway settings.
async fn update_gateway_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateGatewaySettingsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let current = gateway_from_settings(&auth.scope, &service.list(&auth.scope).await?);
    let mut gateway = current;

    if let Some(routing_strategy) = req.routing_strategy {
        gateway.routing_strategy = GatewaySettingsPolicy::canonical_routing_strategy(&routing_strategy)?.to_string();
    }
    if let Some(threshold) = req.circuit_breaker_threshold {
        gateway.circuit_breaker_threshold = threshold;
    }
    if let Some(reset_ms) = req.circuit_breaker_reset_ms {
        gateway.circuit_breaker_reset_ms = reset_ms;
    }

    let value = serde_json::to_value(&gateway).map_err(|err| ErrorKind::Internal(err.into()))?;
    service.upsert(&auth.scope, GATEWAY_KEY, &value).await?;
    Ok(Json(gateway_response(gateway)))
}

/// `PUT /api/settings/{key}` — upsert setting.
async fn upsert_setting(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
    Json(req): Json<UpsertSettingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let setting = service.upsert(&auth.scope, &key, &req.value).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": setting })))
}

/// `DELETE /api/settings/{key}` — delete setting.
async fn delete_setting(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, &key).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
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
    fn runtime_update_rejects_unknown_runtime() {
        let req: UpdateRuntimeSettingsRequest = serde_json::from_str(r#"{"defaultRuntime": "legacy"}"#).unwrap();
        assert_eq!(req.default_runtime.as_deref(), Some("legacy"));
        assert!(RuntimeSettingsPolicy::canonical_runtime(req.default_runtime.as_deref().unwrap()).is_err());
    }

    #[test]
    fn runtime_defaults_are_frontend_contract() {
        let defaults = runtime_defaults();
        assert_eq!(defaults.default_runtime, "container");
        assert!(defaults.available_runtimes.contains(&"api".to_string()));
        assert!(defaults.available_cli_tools.contains(&"claude".to_string()));
    }

    #[test]
    fn runtime_response_keeps_legacy_top_level_fields() {
        let body = runtime_response(runtime_defaults());
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"]["defaultRuntime"], "container");
        assert_eq!(body["defaultRuntime"], "container");
        assert_eq!(body["availableRuntimes"], body["data"]["availableRuntimes"]);
        assert_eq!(body["availableCliTools"], body["data"]["availableCliTools"]);
    }

    #[test]
    fn gateway_response_keeps_legacy_top_level_fields() {
        let body = gateway_response(gateway_defaults());
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"]["routingStrategy"], "specified");
        assert_eq!(body["routingStrategy"], "specified");
        assert_eq!(body["circuitBreakerThreshold"], body["data"]["circuitBreakerThreshold"]);
        assert_eq!(body["circuitBreakerResetMs"], body["data"]["circuitBreakerResetMs"]);
    }
}
