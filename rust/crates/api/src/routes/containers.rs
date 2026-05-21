//! Container control endpoints for agent lifecycle management (nested under `/api/v1`).
//!
//! - `POST /api/v1/agents/{id}/start` — Start an agent container
//! - `POST /api/v1/agents/{id}/stop`  — Stop an agent container

use axum::Json;
use axum::extract::{Path, State};
use serde_json::Value;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};

use crate::health::AppState;
use crate::services::agent::{agent_container_status_response, agent_status_response};
use crate::services::agent_container_control::AgentContainerControlService;

fn make_container_control_service(state: &AppState) -> AgentContainerControlService {
    AgentContainerControlService::from_runtime(
        state.pool.clone(),
        &state.config,
        state.context_features,
        state.encryption_key,
        state.docker.clone(),
        state.auth_callout.clone(),
    )
}

/// `POST /api/agents/{id}/start` — Start an agent container.
///
/// Creates and starts a Docker container for the specified agent. Returns
/// immediately if the agent already has an associated container.
pub async fn start_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let service = make_container_control_service(&state);
    let outcome = service.start(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_container_status_response(outcome.container_id(), outcome.status())))
}

/// `POST /api/agents/{id}/stop` — Stop an agent container.
///
/// Stops the Docker container associated with the specified agent. Returns
/// an error if the agent has no running container.
pub async fn stop_agent(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let service = make_container_control_service(&state);
    service.stop(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_status_response("stopped")))
}

#[cfg(test)]
mod tests {
    use crate::services::agent::{agent_container_status_response, agent_status_response};

    #[test]
    fn start_response_format() {
        let response = agent_container_status_response("abc123", "started");
        assert_eq!(response["ok"], true);
        assert_eq!(response["container_id"], "abc123");
        assert_eq!(response["status"], "started");
    }

    #[test]
    fn stop_response_format() {
        let response = agent_status_response("stopped");
        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "stopped");
    }

    #[test]
    fn already_running_response_format() {
        let response = agent_container_status_response("existing-id", "already_running");
        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "already_running");
    }
}
