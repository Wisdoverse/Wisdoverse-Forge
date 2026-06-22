use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::audit::{AuditAction, AuditLog};
use crate::auth;
use crate::state::AppState;

use super::dag::validate_dag;
use super::errors::WorkflowError;
use super::model::{CreateWorkflowRequest, NodeStatus, SignalRequest, Workflow, WorkflowNode, WorkflowStatus};
use super::service::WorkflowService;
use super::store::Store;

const MAX_WORKFLOW_NODES: usize = 256;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::get(list).post(create))
        .route("/{id}", axum::routing::get(get))
        .route("/{id}/run", axum::routing::post(run))
        .route("/{id}/status", axum::routing::get(status))
        .route("/{id}/cancel", axum::routing::post(cancel))
        .route("/{id}/signal", axum::routing::post(signal))
        .route("/{id}/history", axum::routing::get(history))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_store(state: &AppState) -> Result<Arc<dyn Store>, Response> {
    state.workflow_store.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "database not configured"))
}

#[allow(clippy::result_large_err)]
fn require_service(state: &AppState) -> Result<Arc<WorkflowService>, Response> {
    state.workflow_service.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "temporal not configured"))
}

fn map_error(err: WorkflowError) -> Response {
    match err {
        WorkflowError::NotFound => error(StatusCode::NOT_FOUND, "workflow not found"),
        WorkflowError::InvalidInput(message) => error(StatusCode::BAD_REQUEST, &message),
        WorkflowError::Unavailable(message) => error(StatusCode::SERVICE_UNAVAILABLE, &message),
        WorkflowError::Internal(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.list(&identity.org_id, 50, 0).await {
        Ok(workflows) => (StatusCode::OK, Json(json!({"ok": true, "workflows": workflows}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn create(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateWorkflowRequest>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };
    let Some(name) = req.name.filter(|name| !name.trim().is_empty()) else {
        return error(StatusCode::BAD_REQUEST, "name is required");
    };
    if req.nodes.len() > MAX_WORKFLOW_NODES {
        return error(StatusCode::BAD_REQUEST, "workflow node count exceeds limit");
    }
    if !req.nodes.is_empty()
        && let Err(err) = validate_dag(&req.nodes)
    {
        return map_error(err);
    }

    let mut workflow = Workflow {
        id: String::new(),
        name,
        description: req.description.unwrap_or_default(),
        status: WorkflowStatus::Draft,
        org_id: identity.org_id,
        created_by: identity.user_id,
        temporal_workflow_id: None,
        temporal_run_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut nodes = Vec::new();
    for (index, node) in req.nodes.into_iter().enumerate() {
        let node_name = match node.name.filter(|name| !name.trim().is_empty()) {
            Some(name) => name,
            None => return error(StatusCode::BAD_REQUEST, "node name is required"),
        };
        let node_type = match node.node_type {
            Some(node_type) => node_type,
            None => return error(StatusCode::BAD_REQUEST, "node type is required"),
        };
        nodes.push(WorkflowNode {
            id: String::new(),
            workflow_id: String::new(),
            name: node_name,
            node_type,
            depends_on: node.depends_on,
            config: node.config,
            position: index as i32,
            status: NodeStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
            output: None,
        });
    }

    match store.create(&mut workflow, &mut nodes).await {
        Ok(()) => {
            (StatusCode::CREATED, Json(json!({"ok": true, "workflow": workflow, "nodes": nodes}))).into_response()
        }
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

    let workflow = match store.get_by_id(&id, &identity.org_id).await {
        Ok(workflow) => workflow,
        Err(err) => return map_error(err),
    };
    let nodes = match store.get_nodes(&id).await {
        Ok(nodes) => nodes,
        Err(err) => return map_error(err),
    };
    (StatusCode::OK, Json(json!({"ok": true, "workflow": workflow, "nodes": nodes}))).into_response()
}

async fn run(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if let Err(response) = require_store(&state) {
        return response;
    }
    let service = match require_service(&state) {
        Ok(service) => service,
        Err(response) => return response,
    };

    match service.start_workflow(&id, &identity.org_id).await {
        Ok(workflow) => {
            (StatusCode::ACCEPTED, Json(json!({"ok": true, "workflow": workflow, "status": "started"}))).into_response()
        }
        Err(err) => map_error(err),
    }
}

async fn status(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if let Err(response) = require_store(&state) {
        return response;
    }
    let service = match require_service(&state) {
        Ok(service) => service,
        Err(response) => return response,
    };

    match service.get_status(&id, &identity.org_id).await {
        Ok((workflow, nodes)) => {
            (StatusCode::OK, Json(json!({"ok": true, "workflow": workflow, "nodes": nodes}))).into_response()
        }
        Err(err) => map_error(err),
    }
}

async fn cancel(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if let Err(response) = require_store(&state) {
        return response;
    }
    let service = match require_service(&state) {
        Ok(service) => service,
        Err(response) => return response,
    };

    match service.cancel_workflow(&id, &identity.org_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true, "status": "cancelled"}))).into_response(),
        Err(err) => map_error(err),
    }
}

/// Emit an audit log entry for a workflow signal. Best-effort: when no audit store is
/// configured this is a no-op; when the store is present and write fails the error is
/// returned as a `String` so the caller can log it without rolling back the already-sent
/// Temporal signal.
async fn record_audit(
    state: &AppState,
    action: AuditAction,
    resource_id: Option<String>,
    org_id: String,
    actor_id: String,
    changes: Option<serde_json::Value>,
) -> Result<(), String> {
    let Some(audit_store) = state.audit_store.as_ref() else {
        return Ok(());
    };
    let mut log = AuditLog {
        id: String::new(),
        action,
        actor_id,
        actor_type: "user".to_string(),
        resource: "workflow".to_string(),
        resource_id,
        org_id,
        changes,
        ip_address: None,
        user_agent: None,
        created_at: chrono::Utc::now(),
    };
    audit_store.create(&mut log).await.map_err(|err| format!("audit log failed: {err}"))
}

/// Map a human-review `Decision` to the corresponding workflow audit action.
pub fn audit_action_for_decision(decision: super::model::Decision) -> AuditAction {
    match decision {
        super::model::Decision::Approve => AuditAction::WorkflowReviewApprove,
        super::model::Decision::Reject => AuditAction::WorkflowReviewReject,
    }
}

async fn signal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SignalRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if let Err(response) = require_store(&state) {
        return response;
    }
    let service = match require_service(&state) {
        Ok(service) => service,
        Err(response) => return response,
    };

    // Capture fields before req is consumed by signal_workflow.
    let decision = req.decision;
    let node_id = req.node_id.clone();
    let comment = req.comment.clone();

    match service.signal_workflow(&id, &identity.org_id, req).await {
        Ok(()) => {
            // Emit audit BEST-EFFORT when the signal carries a human-review decision.
            // The signal has already dispatched to Temporal and is NOT rollbackable, so
            // an audit failure must NOT return 500 (that would mislead the caller into
            // retrying an already-sent signal) — it is logged loudly instead.
            if let Some(decision) = decision
                && let Err(err) = record_audit(
                    &state,
                    audit_action_for_decision(decision),
                    Some(id.clone()),
                    identity.org_id.clone(),
                    identity.user_id.clone(),
                    Some(json!({"nodeId": node_id, "decision": decision, "comment": comment})),
                )
                .await
            {
                tracing::error!(workflow_id = %id, ?decision, "audit write failed after signal dispatch: {err}");
            }
            (StatusCode::OK, Json(json!({"ok": true, "status": "signalled"}))).into_response()
        }
        Err(err) => map_error(err),
    }
}

async fn history(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if let Err(response) = require_store(&state) {
        return response;
    }
    let service = match require_service(&state) {
        Ok(service) => service,
        Err(response) => return response,
    };

    match service.get_history(&id, &identity.org_id).await {
        Ok(history) => (StatusCode::OK, Json(json!({"ok": true, "history": history}))).into_response(),
        Err(err) => map_error(err),
    }
}
