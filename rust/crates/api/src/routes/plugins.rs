//! Plugin endpoints (nested under `/api/v1`).
//!
//! Org-scoped plugin management:
//! - `GET    /plugins`      — list plugins (includes global)
//! - `POST   /plugins`      — install/create
//! - `GET    /plugins/{id}` — get
//! - `PATCH  /plugins/{id}` — update config/enable/disable
//! - `DELETE /plugins/{id}` — uninstall
//!
//! Per-agent overrides (issue #33):
//! - `GET    /agents/{agent_id}/plugins`              — list plugins joined with this agent's overrides
//! - `PUT    /agents/{agent_id}/plugins/{plugin_id}`  — set per-agent enabled / config
//! - `DELETE /agents/{agent_id}/plugins/{plugin_id}`  — remove override (revert to plugin default)

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};

use crate::health::AppState;
use crate::repositories::plugin::PluginRepository;
use crate::services::plugin::PluginService;

/// Request body for creating a plugin.
#[derive(Deserialize)]
pub struct CreatePluginRequest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
}

/// Request body for updating a plugin.
#[derive(Deserialize)]
pub struct UpdatePluginRequest {
    pub config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

/// Build a PluginService from shared state.
fn make_service(state: &AppState) -> PluginService {
    PluginService::new(PluginRepository::new(state.pool.clone()))
}

/// `GET /plugins` — list plugins.
async fn list_plugins(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let plugins = service.list(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": plugins })))
}

/// `POST /plugins` — create a plugin.
async fn create_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreatePluginRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let plugin = service
        .create(&auth.scope, &req.name, req.version.as_deref(), req.description.as_deref(), req.config.as_ref())
        .await?;
    tracing::info!(org_id = %auth.scope.org_id(), plugin = %plugin.name, "Plugin created");
    Ok(Json(serde_json::json!({ "ok": true, "data": plugin })))
}

/// `GET /plugins/{id}` — get a plugin.
async fn get_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let plugin = service.get(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": plugin })))
}

/// `PATCH /plugins/{id}` — update a plugin.
async fn update_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePluginRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let plugin = service.update(&auth.scope, id, req.config.as_ref(), req.enabled).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": plugin })))
}

/// `DELETE /plugins/{id}` — uninstall a plugin.
async fn delete_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Body for `PUT /agents/{agent_id}/plugins/{plugin_id}` — set per-agent override.
#[derive(Deserialize)]
pub struct SetAgentPluginRequest {
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

async fn list_agent_plugins(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let rows = service.list_for_agent(&auth.scope, AgentId::from(agent_id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "plugins": rows })))
}

async fn set_agent_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((agent_id, plugin_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetAgentPluginRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.set_for_agent(&auth.scope, AgentId::from(agent_id), plugin_id, req.enabled, req.config.as_ref()).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unset_agent_plugin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((agent_id, plugin_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove_for_agent(&auth.scope, AgentId::from(agent_id), plugin_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Build plugin routes sub-router.
pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/plugins", get(list_plugins).post(create_plugin))
        .route("/plugins/{id}", get(get_plugin).patch(update_plugin).delete(delete_plugin))
        .route("/agents/{agent_id}/plugins", get(list_agent_plugins))
        .route("/agents/{agent_id}/plugins/{plugin_id}", put(set_agent_plugin).delete(unset_agent_plugin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_plugin_request_deserialization() {
        let req: CreatePluginRequest = serde_json::from_str(r#"{"name": "my-plugin", "version": "1.0.0"}"#).unwrap();
        assert_eq!(req.name, "my-plugin");
        assert_eq!(req.version.as_deref(), Some("1.0.0"));
        assert!(req.description.is_none());
    }

    #[test]
    fn create_plugin_request_minimal() {
        let req: CreatePluginRequest = serde_json::from_str(r#"{"name": "basic"}"#).unwrap();
        assert_eq!(req.name, "basic");
        assert!(req.version.is_none());
    }

    #[test]
    fn update_plugin_request_deserialization() {
        let req: UpdatePluginRequest = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert_eq!(req.enabled, Some(false));
        assert!(req.config.is_none());
    }

    #[test]
    fn update_plugin_request_with_config() {
        let req: UpdatePluginRequest =
            serde_json::from_str(r#"{"config": {"key": "value"}, "enabled": true}"#).unwrap();
        assert!(req.config.is_some());
        assert_eq!(req.enabled, Some(true));
    }
}
