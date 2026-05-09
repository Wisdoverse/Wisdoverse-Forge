use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::auth;
use crate::mcp::client::CreateSessionArgs;
use crate::state::AppState;

use super::errors::TaskError;
use super::model::{
    AssignRequest, CreateTaskRequest, Task, TaskFilter, TaskPriority, TaskState, TransitionRequest, UpdateTaskRequest,
};
use super::store::Store;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::get(list).post(create))
        .route("/{id}", axum::routing::get(get).patch(update))
        .route("/{id}/assign", axum::routing::post(assign))
        .route("/{id}/transition", axum::routing::post(transition))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    state: Option<TaskState>,
    assigned_to: Option<String>,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_store(state: &AppState) -> Result<Arc<dyn Store>, Response> {
    state.task_store.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "database not configured"))
}

fn map_error(err: TaskError) -> Response {
    match err {
        TaskError::NotFound => error(StatusCode::NOT_FOUND, "task not found"),
        TaskError::InvalidInput(message) => error(StatusCode::BAD_REQUEST, &message),
        TaskError::Internal(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

async fn list(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<ListQuery>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store
        .list(TaskFilter {
            org_id: identity.org_id,
            state: query.state,
            assigned_to: query.assigned_to,
            limit: 50,
            offset: 0,
        })
        .await
    {
        Ok(tasks) => (StatusCode::OK, Json(json!({"ok": true, "tasks": tasks}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn create(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateTaskRequest>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if req.title.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "title is required");
    }

    let mut task = Task {
        id: String::new(),
        workflow_id: req.workflow_id,
        title: req.title,
        description: req.description,
        state: TaskState::Pending,
        priority: req.priority.unwrap_or(TaskPriority::Normal),
        assigned_to: req.assigned_to,
        review_id: None,
        agentforge_session_id: None,
        depends_on: req.depends_on,
        created_by: identity.user_id,
        org_id: identity.org_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    match store.create(&mut task).await {
        Ok(()) => (StatusCode::CREATED, Json(json!({"ok": true, "task": task}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn get(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.get_by_id(&id, &identity.org_id).await {
        Ok(task) => (StatusCode::OK, Json(json!({"ok": true, "task": task}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.update(&id, &identity.org_id, req).await {
        Ok(()) => match store.get_by_id(&id, &identity.org_id).await {
            Ok(task) => (StatusCode::OK, Json(json!({"ok": true, "task": task}))).into_response(),
            Err(err) => map_error(err),
        },
        Err(err) => map_error(err),
    }
}

async fn assign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AssignRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if req.participant_id.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "participantId is required");
    }

    match store.assign(&id, &identity.org_id, req.participant_id.clone(), TaskState::Assigned).await {
        Ok(()) => match store.get_by_id(&id, &identity.org_id).await {
            Ok(task) => {
                if let (Some(agent_provider), Some(outbound_mcp)) =
                    (req.agent_provider.clone(), state.outbound_mcp.clone())
                {
                    let store = store.clone();
                    let task_id = task.id.clone();
                    let org_id = identity.org_id.clone();
                    let title = task.title.clone();
                    let description = task.description.clone();
                    let project_id = req.project_id.clone().unwrap_or_default();
                    let agent_directory = state.agent_directory.clone();
                    tokio::spawn(async move {
                        let session = match outbound_mcp
                            .session_create(CreateSessionArgs {
                                project_id,
                                cli_tool: agent_provider.clone(),
                                name: Some(title.clone()),
                            })
                            .await
                        {
                            Ok(session) => session,
                            Err(err) => {
                                tracing::error!(%task_id, error = %err, "failed to create outbound MCP session");
                                return;
                            }
                        };

                        if let Some(agent_directory) = agent_directory
                            && let Err(err) = agent_directory
                                .upsert_agent(&org_id, session.session_id(), &agent_provider, &title)
                                .await
                        {
                            tracing::error!(%task_id, error = %err, "failed to upsert agent participant");
                        }

                        if let Err(err) =
                            store.set_session_id(&task_id, &org_id, session.session_id().to_string()).await
                        {
                            tracing::error!(%task_id, error = %err, "failed to persist task session id");
                            return;
                        }

                        let prompt = task_prompt(&title, &description);
                        if let Err(err) = outbound_mcp.session_prompt(session.session_id(), &prompt).await {
                            tracing::error!(%task_id, error = %err, "failed to send outbound MCP prompt");
                            return;
                        }

                        if let Err(err) = store.update_state(&task_id, &org_id, TaskState::Working).await {
                            tracing::error!(%task_id, error = %err, "failed to transition task to working");
                        }
                    });
                }

                (StatusCode::OK, Json(json!({"ok": true, "task": task}))).into_response()
            }
            Err(err) => map_error(err),
        },
        Err(err) => map_error(err),
    }
}

async fn transition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<TransitionRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    let mut task = match store.get_by_id(&id, &identity.org_id).await {
        Ok(task) => task,
        Err(err) => return map_error(err),
    };
    let allowed = valid_transitions(task.state);
    if !allowed.contains(&req.state) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "error": "invalid transition",
                "from": task.state,
                "to": req.state,
                "allowed": allowed,
            })),
        )
            .into_response();
    }

    match store.update_state(&id, &identity.org_id, req.state).await {
        Ok(()) => {
            task.state = req.state;
            (StatusCode::OK, Json(json!({"ok": true, "task": task}))).into_response()
        }
        Err(err) => map_error(err),
    }
}

fn valid_transitions(state: TaskState) -> &'static [TaskState] {
    match state {
        TaskState::Pending => &[TaskState::Assigned],
        TaskState::Assigned => &[TaskState::Working, TaskState::Pending],
        TaskState::Working => &[TaskState::Review, TaskState::Failed],
        TaskState::Review => &[TaskState::Completed, TaskState::ChangesRequested],
        TaskState::ChangesRequested => &[TaskState::Working],
        TaskState::Failed => &[TaskState::Pending],
        TaskState::Completed => &[],
    }
}

fn task_prompt(title: &str, description: &str) -> String {
    if description.is_empty() { title.to_string() } else { format!("{title}\n\n{description}") }
}
