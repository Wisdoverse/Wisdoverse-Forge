//! Agent CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/agents`                      — list agents (paginated)
//! - `POST   /api/v1/agents`                      — create agent
//! - `GET    /api/v1/agents/:id`                  — get agent by ID
//! - `DELETE /api/v1/agents/:id`                  — delete agent
//! - `PATCH  /api/v1/agents/:id/status`           — update agent status
//! - `GET    /api/v1/agents/:id/messages`         — list chat history
//! - `DELETE /api/v1/agents/:id/messages`         — wipe chat history
//! - `POST   /api/v1/agents/:id/prompt/interrupt` — cancel in-flight SSE stream

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AgentStatus, AppResult};

use crate::health::AppState;
use crate::services::agent::{
    AgentService, CreateAgentParams, agent_data_response, agent_delete_response, agent_git_status_response,
    agent_list_response, agent_messages_deleted_response, agent_messages_response, agent_permission_response,
    agent_prompt_sent_response, agent_response, agent_status_response,
};
use crate::services::agent_container_lifecycle::AgentContainerLifecycleService;
use crate::services::agent_message::AgentMessageService;
use crate::services::agent_prompt::{AgentPromptDispatch, AgentPromptService};

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for creating an agent.
#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub name: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    /// `claude` / `codex` / `gemini` / `opencode` for Container CLI agents.
    /// Omit for provider+prompt agents (no container shell).
    #[serde(default, alias = "cliTool")]
    pub cli_tool: Option<String>,
    /// Requested working directory. For Container CLI agents this is resolved
    /// server-side under the managed Wisdoverse Forge workspace root.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Workspace execution/access boundary. If omitted, it is inferred from
    /// `project_id`, then from the tenant's default workspace.
    #[serde(default, alias = "workspaceId")]
    pub workspace_id: Option<Uuid>,
    /// Primary project context for task routing and UI ownership. It does not
    /// narrow the container filesystem mount by itself.
    #[serde(default, alias = "projectId")]
    pub project_id: Option<Uuid>,
    /// System prompt for provider+prompt agents. `alias = "systemPrompt"` allows
    /// both camelCase (frontend) and snake_case (API clients) to be accepted.
    #[serde(default, alias = "systemPrompt")]
    pub system_prompt: Option<String>,
}

/// Request body for updating agent fields (name, model, provider, system_prompt).
#[derive(Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    /// System prompt for provider+prompt agents. `alias = "systemPrompt"` allows
    /// both camelCase (frontend) and snake_case (API clients) to be accepted.
    #[serde(default, alias = "systemPrompt")]
    pub system_prompt: Option<String>,
}

/// Request body for updating agent status.
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: AgentStatus,
}

/// Request body for sending a prompt to an agent.
#[derive(Deserialize)]
pub struct PromptRequest {
    pub content: String,
    #[serde(default)]
    pub images: Option<Vec<String>>,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> AgentService {
    AgentService::from_pool_with_workspace(state.pool.clone())
}

fn make_message_service(state: &AppState) -> AgentMessageService {
    AgentMessageService::from_pool(state.pool.clone())
}

fn make_prompt_service(state: &AppState) -> AgentPromptService {
    AgentPromptService::from_runtime(
        state.pool.clone(),
        state.llm_factory.clone(),
        state.encryption_key,
        state.agent_command_bus.clone(),
        state.nats.clone(),
        state.inflight_prompts.clone(),
    )
}

fn make_container_lifecycle_service(state: &AppState) -> AgentContainerLifecycleService {
    AgentContainerLifecycleService::from_runtime(state.pool.clone(), state.docker.clone())
}

/// `GET /api/v1/agents` — list agents for the authenticated tenant.
///
/// Response shape matches the frontend `AgentListResponse` contract:
/// `{ ok: true, agents: AgentListItem[] }`. Each item includes `ownerUsername`,
/// `ownerEmail`, and `projectName` so the UI can display the owner.
async fn list_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let agents = service.list_with_owner(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(agent_list_response(agents)))
}

/// `GET /api/agents/:id` — get a single agent by ID.
///
/// Response shape: `{ ok: true, agent: AgentListItem }` including owner info.
async fn get_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let agent = service.get_with_owner(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_response(agent)))
}

/// `POST /api/v1/agents` — create a new agent.
///
/// Returns the enriched `{ ok: true, agent: AgentListItem }` shape expected by
/// the frontend `CreateAgentResponse` contract, so callers can read
/// `response.agent.name` / `response.agent.ownerUsername` directly.
async fn create_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let cli_tool = req.cli_tool.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let agent = service
        .create(
            &auth.scope,
            CreateAgentParams {
                name: req.name.as_deref(),
                model: req.model.as_deref(),
                provider: req.provider.as_deref(),
                cli_tool,
                cwd: req.cwd.as_deref(),
                workspace_id: req.workspace_id,
                project_id: req.project_id,
                system_prompt: req.system_prompt.as_deref(),
            },
        )
        .await?;
    // Re-fetch so the response includes owner + project names via the JOIN view.
    let enriched = service.get_with_owner(&auth.scope, agent.id).await?;
    Ok(Json(agent_response(enriched)))
}

/// `PATCH /api/v1/agents/:id/status` — update agent status with state machine validation.
async fn update_agent_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStatusRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let agent = service.update_status(&auth.scope, AgentId::from(id), req.status).await?;
    Ok(Json(agent_data_response(agent)))
}

/// `DELETE /api/v1/agents/:id` — delete an agent.
async fn delete_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_delete_response()))
}

/// `PATCH /api/v1/agents/:id` — update agent fields (name, model, provider).
async fn update_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAgentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let agent = service
        .update(
            &auth.scope,
            AgentId::from(id),
            req.name.as_deref(),
            req.model.as_deref(),
            req.provider.as_deref(),
            req.system_prompt.as_deref(),
        )
        .await?;
    Ok(Json(agent_data_response(agent)))
}

/// `POST /api/v1/agents/:id/prompt` — send a prompt to the agent.
///
/// Container CLI agents (cli_tool = Some) keep the existing NATS publish path.
/// Provider+prompt agents (cli_tool = None) run an SSE stream via the prompt
/// application service. The response Content-Type differs accordingly.
async fn send_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PromptRequest>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    use axum::response::sse::{Event, KeepAlive, Sse};

    let service = make_prompt_service(&state);
    let dispatch =
        service.send_prompt(auth.scope.clone(), AgentId::from(id), &req.content, req.images.as_deref()).await?;

    let AgentPromptDispatch::ProviderStream { frames: frame_stream } = dispatch else {
        return Ok(Json(agent_prompt_sent_response(id)).into_response());
    };

    type SseBoxError = Box<dyn std::error::Error + Send + Sync>;
    let sse_events = frame_stream.map(move |frame_result| {
        let frame = frame_result.map_err(|e| -> SseBoxError { e.kind.to_string().into() })?;
        let (event_name, data) = frame.split();
        Ok::<_, SseBoxError>(Event::default().event(event_name).data(data.to_string()))
    });

    let sse = Sse::new(sse_events).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)));
    let mut resp = sse.into_response();
    resp.headers_mut().insert("cache-control", "no-cache".parse().expect("static header value"));
    resp.headers_mut().insert("x-accel-buffering", "no".parse().expect("static header value"));

    Ok(resp)
}

/// `POST /api/v1/agents/:id/interrupt` — interrupt the agent.
async fn interrupt_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_prompt_service(&state);
    service.interrupt_sidecar(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_status_response("interrupting")))
}

/// Query parameters for the messages list endpoint.
#[derive(Deserialize)]
pub struct MessagesQuery {
    #[serde(default = "default_messages_limit")]
    pub limit: i64,
    pub before: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_messages_limit() -> i64 {
    50
}

/// `GET /api/v1/agents/:id/messages` — return chronological chat history.
async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<MessagesQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_message_service(&state);
    let (msgs, has_more) = service.list(&auth.scope, AgentId::from(id), q.limit, q.before).await?;
    Ok(Json(agent_messages_response(msgs, has_more)))
}

/// `DELETE /api/v1/agents/:id/messages` — wipe all chat history for the agent.
async fn delete_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_message_service(&state);
    let deleted = service.delete_all(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_messages_deleted_response(deleted)))
}

/// `POST /api/v1/agents/:id/prompt/interrupt` — cancel the in-flight LLM stream.
///
/// Tenant ownership check on the agent before touching the in-flight map —
/// without this, an attacker who learned any streaming agent's UUID could
/// cancel another tenant's active prompt (the map is keyed by bare AgentId).
async fn interrupt_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_prompt_service(&state);
    service.interrupt_provider_stream(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_delete_response()))
}

/// `POST /api/v1/agents/:id/restart` — restart agent container.
async fn restart_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    make_container_lifecycle_service(&state).restart(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_status_response("restarted")))
}

/// `POST /api/v1/agents/:id/resume` — resume a stopped agent.
async fn resume_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    make_container_lifecycle_service(&state).resume(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_status_response("resumed")))
}

/// Request body for adding a collaborator.
#[derive(Deserialize)]
pub struct AddCollaboratorRequest {
    pub user_id: Uuid,
    #[serde(default = "default_permission")]
    pub permission: String,
}

fn default_permission() -> String {
    "view".to_string()
}

/// Request body for updating a collaborator's permission.
#[derive(Deserialize)]
pub struct UpdateCollaboratorRequest {
    pub permission: String,
}

/// Request body for checking/granting permission.
#[derive(Deserialize)]
pub struct PermissionRequest {
    pub user_id: Uuid,
    pub action: String,
}

/// `GET /api/v1/agents/:id/collaborators` — list collaborators for an agent.
async fn list_collaborators(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let collabs = service.list_collaborators(&auth.scope, AgentId::from(id)).await?;
    Ok(Json(agent_data_response(collabs)))
}

/// `POST /api/v1/agents/:id/collaborators` — add a collaborator.
async fn add_collaborator(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AddCollaboratorRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let collab = service.add_collaborator(&auth.scope, AgentId::from(id), req.user_id, &req.permission).await?;
    Ok(Json(agent_data_response(collab)))
}

/// `PATCH /api/v1/agents/:id/collaborators/:user_id` — update a collaborator's permission.
async fn update_collaborator(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateCollaboratorRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let collab = service.update_collaborator(&auth.scope, AgentId::from(id), user_id, &req.permission).await?;
    Ok(Json(agent_data_response(collab)))
}

/// `DELETE /api/v1/agents/:id/collaborators/:user_id` — remove a collaborator.
async fn remove_collaborator(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove_collaborator(&auth.scope, AgentId::from(id), user_id).await?;
    Ok(Json(agent_delete_response()))
}

/// `GET /api/v1/agents/:id/git` — get git status (stub).
async fn get_git_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    // Verify agent exists and belongs to tenant
    let _agent = service.get(&auth.scope, AgentId::from(id)).await?;

    // Stub: return empty git status
    Ok(Json(agent_git_status_response()))
}

/// `POST /api/v1/agents/:id/permission` — check/grant permission.
async fn check_permission(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PermissionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let projection = service.check_permission(&auth.scope, AgentId::from(id), req.user_id, &req.action).await?;
    Ok(Json(agent_permission_response(projection)))
}

/// Build agent routes sub-router.
pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/agents", get(list_agents).post(create_agent))
        .route("/agents/{id}", get(get_agent).patch(update_agent).delete(delete_agent))
        .route("/agents/{id}/status", patch(update_agent_status))
        .route("/agents/{id}/prompt", post(send_prompt))
        .route("/agents/{id}/prompt/interrupt", post(interrupt_prompt))
        .route("/agents/{id}/messages", get(list_messages).delete(delete_messages))
        .route("/agents/{id}/interrupt", post(interrupt_agent))
        .route("/agents/{id}/restart", post(restart_agent))
        .route("/agents/{id}/resume", post(resume_agent))
        .route("/agents/{id}/collaborators", get(list_collaborators).post(add_collaborator))
        .route("/agents/{id}/collaborators/{user_id}", patch(update_collaborator).delete(remove_collaborator))
        .route("/agents/{id}/git", get(get_git_status))
        .route("/agents/{id}/permission", post(check_permission))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::ErrorKind;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn list_query_custom_values() {
        let query: ListQuery = serde_json::from_str(r#"{"limit": 50, "offset": 10}"#).unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn create_request_all_optional() {
        let req: CreateAgentRequest = serde_json::from_str("{}").unwrap();
        assert!(req.name.is_none());
        assert!(req.model.is_none());
        assert!(req.provider.is_none());
        assert!(req.cwd.is_none());
        assert!(req.workspace_id.is_none());
        assert!(req.project_id.is_none());
    }

    #[test]
    fn create_request_with_values() {
        let req: CreateAgentRequest = serde_json::from_str(
            r#"{
                "name": "test-agent",
                "model": "claude-sonnet-4-20250514",
                "provider": "anthropic",
                "cwd": "~/projects",
                "workspaceId": "650e8400-e29b-41d4-a716-446655440000",
                "projectId": "550e8400-e29b-41d4-a716-446655440000"
            }"#,
        )
        .unwrap();
        assert_eq!(req.name.as_deref(), Some("test-agent"));
        assert_eq!(req.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(req.provider.as_deref(), Some("anthropic"));
        assert_eq!(req.cwd.as_deref(), Some("~/projects"));
        assert_eq!(req.workspace_id, Some(Uuid::parse_str("650e8400-e29b-41d4-a716-446655440000").unwrap()));
        assert_eq!(req.project_id, Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()));
    }

    #[test]
    fn update_status_request_deserialization() {
        let req: UpdateStatusRequest = serde_json::from_str(r#"{"status": "working"}"#).unwrap();
        assert_eq!(req.status, AgentStatus::Working);

        let req: UpdateStatusRequest = serde_json::from_str(r#"{"status": "idle"}"#).unwrap();
        assert_eq!(req.status, AgentStatus::Idle);

        let req: UpdateStatusRequest = serde_json::from_str(r#"{"status": "offline"}"#).unwrap();
        assert_eq!(req.status, AgentStatus::Offline);
    }

    #[test]
    fn update_status_request_invalid() {
        let result = serde_json::from_str::<UpdateStatusRequest>(r#"{"status": "invalid"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_agent_request_deserialization() {
        let req: UpdateAgentRequest = serde_json::from_str(
            r#"{"name": "new-name", "model": "claude-sonnet-4-20250514", "provider": "anthropic"}"#,
        )
        .unwrap();
        assert_eq!(req.name.as_deref(), Some("new-name"));
        assert_eq!(req.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(req.provider.as_deref(), Some("anthropic"));
    }

    #[test]
    fn update_agent_request_partial() {
        let req: UpdateAgentRequest = serde_json::from_str(r#"{"name": "new-name"}"#).unwrap();
        assert_eq!(req.name.as_deref(), Some("new-name"));
        assert!(req.model.is_none());
        assert!(req.provider.is_none());
    }

    #[test]
    fn update_agent_request_empty() {
        let req: UpdateAgentRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert!(req.name.is_none());
        assert!(req.model.is_none());
        assert!(req.provider.is_none());
    }

    #[test]
    fn prompt_request_deserialization() {
        let req: PromptRequest = serde_json::from_str(r#"{"content": "hello", "images": ["base64data"]}"#).unwrap();
        assert_eq!(req.content, "hello");
        assert_eq!(req.images.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn prompt_request_content_only() {
        let req: PromptRequest = serde_json::from_str(r#"{"content": "hello"}"#).unwrap();
        assert_eq!(req.content, "hello");
        assert!(req.images.is_none());
    }

    #[test]
    fn prompt_request_with_images_is_rejected() {
        let req: PromptRequest = serde_json::from_str(r#"{"content": "hello", "images": ["base64data"]}"#).unwrap();
        let result = crate::domain::agent::PlainTextAgentPrompt::new(&req.content, req.images.as_deref());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Validation(msg) if msg.contains("not supported")));
    }

    #[test]
    fn prompt_request_empty_content_no_images_should_fail_validation() {
        let req: PromptRequest = serde_json::from_str(r#"{"content": ""}"#).unwrap();
        let result = crate::domain::agent::PlainTextAgentPrompt::new(&req.content, req.images.as_deref());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Validation(msg) if msg.contains("required")));
    }

    #[test]
    fn prompt_request_blank_content_is_rejected() {
        let req: PromptRequest = serde_json::from_str(r#"{"content": "   "}"#).unwrap();
        let result = crate::domain::agent::PlainTextAgentPrompt::new(&req.content, req.images.as_deref());
        assert!(result.is_err());
    }

    #[test]
    fn add_collaborator_request_defaults() {
        let req: AddCollaboratorRequest =
            serde_json::from_str(r#"{"user_id": "00000000-0000-0000-0000-000000000001"}"#).unwrap();
        assert_eq!(req.permission, "view"); // default
    }

    #[test]
    fn add_collaborator_request_with_permission() {
        let req: AddCollaboratorRequest =
            serde_json::from_str(r#"{"user_id": "00000000-0000-0000-0000-000000000001", "permission": "edit"}"#)
                .unwrap();
        assert_eq!(req.permission, "edit");
    }

    #[test]
    fn add_collaborator_missing_user_id_fails() {
        let result = serde_json::from_str::<AddCollaboratorRequest>(r#"{"permission": "view"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_collaborator_request_deserialization() {
        let req: UpdateCollaboratorRequest = serde_json::from_str(r#"{"permission": "admin"}"#).unwrap();
        assert_eq!(req.permission, "admin");
    }

    #[test]
    fn update_collaborator_missing_permission_fails() {
        let result = serde_json::from_str::<UpdateCollaboratorRequest>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn permission_request_deserialization() {
        let req: PermissionRequest =
            serde_json::from_str(r#"{"user_id": "00000000-0000-0000-0000-000000000001", "action": "edit"}"#).unwrap();
        assert_eq!(req.action, "edit");
    }

    #[test]
    fn permission_request_missing_fields_fails() {
        let result =
            serde_json::from_str::<PermissionRequest>(r#"{"user_id": "00000000-0000-0000-0000-000000000001"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn create_request_camel_case_system_prompt_alias() {
        let req: CreateAgentRequest = serde_json::from_str(r#"{"systemPrompt": "you are helpful"}"#).unwrap();
        assert_eq!(req.system_prompt.as_deref(), Some("you are helpful"));
    }

    #[test]
    fn update_request_camel_case_system_prompt_alias() {
        let req: UpdateAgentRequest = serde_json::from_str(r#"{"systemPrompt": "new prompt"}"#).unwrap();
        assert_eq!(req.system_prompt.as_deref(), Some("new prompt"));
    }

    #[test]
    fn messages_query_defaults() {
        let q: MessagesQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.limit, 50);
        assert!(q.before.is_none());
    }

    #[test]
    fn messages_query_custom_limit() {
        let q: MessagesQuery = serde_json::from_str(r#"{"limit": 100}"#).unwrap();
        assert_eq!(q.limit, 100);
    }
}
