//! Orchestration A2A endpoints (nested under `/api/v1`).
//!
//! Routes (all prefixed `/api/v1/orchestration/`):
//! - `POST   /tasks`                              — create task (auto-dispatches when no assignee)
//! - `GET    /tasks?status=&limit=&offset=`       — flat list (org-scoped)
//! - `GET    /tasks/{id}`                         — single task
//! - `PATCH  /tasks/{id}`                         — update state/priority/progress/assignedTo
//! - `POST   /tasks/{id}/dispatch`                — explicit dispatch to next available
//! - `POST   /tasks/{id}/publish-with-context`    — dispatch with a preview hash guard
//! - `POST   /tasks/{id}/complete`                — mark complete (returns task summary)
//! - `POST   /tasks/{id}/fail`                    — mark failed
//! - `POST   /tasks/{id}/approve`                 — approve a waiting_approval task
//! - `POST   /tasks/{id}/cancel`                  — cancel (terminal)
//! - `POST   /tasks/{id}/retry`                   — reset to backlog and re-dispatch
//! - `GET    /tasks/{id}/context`                  — task detail Context tab read model
//! - `GET    /tasks/{id}/runs`                     — task execution attempts
//! - `GET    /tasks/{id}/comments`                — human updates (comments & blocker signals)
//! - `POST   /tasks/{id}/comments`                — add a comment / block / unblock
//! - `DELETE /tasks/{id}/comments/{comment_id}`   — delete your own comment
//! - `GET    /tasks/comments/latest?taskIds=`        — latest blocker/unblock marks
//! - `GET    /tasks/export?limit=`                    — compliance CSV of task history
//! - `GET    /groups/{group_id}/tasks?state=`     — kanban list (group-scoped)
//! - `GET    /groups/{group_id}/tasks/stats`      — `byState` counts for the kanban
//! - `POST   /groups/{group_id}/tasks/retire-stale` — batch-retire stale tasks (org admin)
//! - `POST   /participants`                       — register participant (sweeps after)
//! - `GET    /participants?status=`               — list participants (UI dropdown source)
//! - `POST   /participants/{agent_id}/heartbeat`  — heartbeat (sweeps after)
//! - `DELETE /participants/{agent_id}`            — unregister
//!
//! Response shape matches the frontend `TaskSummary`/`Participant` interfaces in
//! `src/app/api/orchestration.ts` so the kanban consumes the API directly.

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};

use crate::domain::orchestration::TaskAssignmentPatchPolicy;
use crate::health::AppState;
use crate::services::context_feature::ContextFeatureService;
use crate::services::context_preview::{ContextPreviewService, PublishWithContextInput};
use crate::services::orchestration::{
    CreateTaskParamsInput, OrchestrationService, ParticipantSummary, TaskSummary, create_task_request_parts,
    orchestration_delete_response, orchestration_human_marks_response, orchestration_participant_response,
    orchestration_participants_response, orchestration_stats_response, orchestration_task_comment_response,
    orchestration_task_comments_response, orchestration_task_context_response, orchestration_task_export_response,
    orchestration_task_response, orchestration_task_review_check_response, orchestration_task_review_checks_response,
    orchestration_task_review_gates_response, orchestration_task_runs_response, orchestration_tasks_response,
};
use crate::services::task_context::TaskContextService;

// ---------------------------------------------------------------------------
// Query / request bodies
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    /// Filter to tasks assigned to this agent — used by the agent detail Tasks tab.
    #[serde(rename = "agentId")]
    pub agent_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Deserialize)]
pub struct ListGroupTasksQuery {
    pub state: Option<String>,
}

#[derive(Deserialize)]
pub struct ListParticipantsQuery {
    pub status: Option<String>,
}

fn default_limit() -> i64 {
    20
}

/// Body of `POST /tasks`. `params.task` carries the user-facing instruction and
/// `params.message` carries the optional prompt body so the existing A2A
/// runtime contract (`tasks/send`) keeps working.
#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    #[serde(rename = "groupId")]
    pub group_id: Option<Uuid>,
    #[serde(rename = "assignedTo")]
    pub assigned_to: Option<Uuid>,
    pub params: Option<TaskParamsRequest>,
    #[serde(rename = "parentTaskId")]
    pub parent_task_id: Option<Uuid>,
    #[serde(default, rename = "requiresApproval")]
    pub requires_approval: bool,
}

#[derive(Deserialize)]
pub struct TaskParamsRequest {
    pub task: Option<String>,
    pub message: Option<String>,
    #[serde(default, rename = "requiredInputs")]
    pub required_inputs: Vec<String>,
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
    #[serde(default)]
    pub env: Option<serde_json::Value>,
    #[serde(default, rename = "apiKeys")]
    pub api_keys: Option<serde_json::Value>,
    /// #793/#875 opt-in completion contract. Captured as a raw `Value` and stored
    /// verbatim in the task's params so the NATS result consumer's verifier can
    /// parse it (`core::ExpectedResult::from_params`). Over-typing here would drop
    /// sub-keys a newer producer adds.
    #[serde(default, rename = "expectedResult", skip_serializing_if = "Option::is_none")]
    pub expected_result: Option<serde_json::Value>,
    /// Attachment UUIDs of instruction images (vision-capable container CLI tasks).
    #[serde(default, rename = "imageAttachmentIds")]
    pub image_attachment_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    pub state: Option<String>,
    pub priority: Option<String>,
    pub progress: Option<i16>,
    #[serde(default, rename = "assignedTo")]
    pub assigned_to: Option<String>, // "" = explicit unassign; UUID string = assign
}

#[derive(Deserialize)]
pub struct PublishWithContextRequest {
    #[serde(rename = "contextPreviewId")]
    pub context_preview_id: Uuid,
    #[serde(rename = "previewHash")]
    pub preview_hash: String,
    #[serde(default, rename = "pinnedIds")]
    pub pinned_ids: Vec<Uuid>,
    #[serde(default, rename = "removedIds")]
    pub removed_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct CompleteTaskRequest {
    pub result: serde_json::Value,
}

#[derive(Deserialize)]
pub struct LatestHumanMarksQuery {
    /// Comma-separated task ids (board badges). Unknown ids are ignored.
    #[serde(rename = "taskIds")]
    pub task_ids: String,
}

#[derive(Deserialize)]
pub struct TaskExportQuery {
    /// Max rows (capped at 1000 server-side).
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct SetTaskReviewCheckRequest {
    pub done: bool,
}

#[derive(Deserialize)]
pub struct FailTaskRequest {
    pub error: serde_json::Value,
}

#[derive(Deserialize)]
pub struct CreateTaskCommentRequest {
    /// 'comment' (default), 'blocker', or 'unblock'.
    pub kind: Option<String>,
    pub body: String,
}

#[derive(Deserialize)]
pub struct RegisterParticipantRequest {
    #[serde(rename = "agent_id", alias = "agentId")]
    pub agent_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_service(state: &AppState) -> OrchestrationService {
    state.orchestration_service()
}

fn make_task_context_service(state: &AppState) -> TaskContextService {
    state.task_context_service()
}

fn make_context_preview_service(state: &AppState) -> ContextPreviewService {
    state.context_preview_service()
}

fn make_feature_service(state: &AppState) -> ContextFeatureService {
    state.context_feature_service()
}

fn extract_params(req: &CreateTaskRequest) -> (String, Option<String>, Option<serde_json::Value>) {
    create_task_request_parts(
        req.title.as_deref(),
        req.description.as_deref(),
        req.params.as_ref().map(|p| CreateTaskParamsInput {
            task: p.task.as_deref(),
            message: p.message.as_deref(),
            required_inputs: &p.required_inputs,
            inputs: p.inputs.as_ref(),
            env: p.env.as_ref(),
            api_keys: p.api_keys.as_ref(),
            expected_result: p.expected_result.as_ref(),
            image_attachment_ids: &p.image_attachment_ids,
        }),
    )
}

// ---------------------------------------------------------------------------
// Task handlers
// ---------------------------------------------------------------------------

async fn create_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let (title, description, params) = extract_params(&req);
    let task = service
        .create_task(
            &auth.scope,
            &title,
            description.as_deref(),
            params,
            req.priority.as_deref(),
            req.group_id,
            req.assigned_to.map(AgentId::from),
            req.parent_task_id,
            req.requires_approval,
        )
        .await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.created", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn list_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListTasksQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tasks = service
        .list_tasks(&auth.scope, query.status.as_deref(), query.agent_id.map(AgentId::from), query.limit, query.offset)
        .await?;
    let summaries: Vec<TaskSummary> = service.summarize_tasks(&auth.scope, tasks).await?;
    Ok(Json(orchestration_tasks_response(&summaries)))
}

async fn list_group_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(group_id): Path<Uuid>,
    Query(query): Query<ListGroupTasksQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tasks = service.list_tasks_by_group(&auth.scope, group_id, query.state.as_deref()).await?;
    let summaries: Vec<TaskSummary> = service.summarize_tasks(&auth.scope, tasks).await?;
    Ok(Json(orchestration_tasks_response(&summaries)))
}

/// Body for batch retirement of stale tasks in a group.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetireStaleTasksRequest {
    pub older_than_days: Option<i32>,
    pub batch_limit: Option<i64>,
}

/// `POST /groups/{group_id}/tasks/retire-stale` — batch-retire stale,
/// never-started tasks (org admin; the operation is audited).
async fn retire_stale_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<RetireStaleTasksRequest>,
) -> AppResult<Json<serde_json::Value>> {
    crate::services::admin::AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let (count, ids) = service.retire_stale_tasks(&auth.scope, group_id, req.older_than_days, req.batch_limit).await?;
    let _ = state
        .audit_service()
        .log_action(
            auth.scope.org_id(),
            Some(auth.scope.user_id()),
            "orchestration.tasks.retired_stale",
            "orchestration_group",
            Some(group_id),
            &crate::domain::orchestration::retired_stale_audit_payload(count),
            None,
        )
        .await;
    Ok(Json(crate::domain::orchestration::retired_stale_response(count, ids)))
}
async fn group_task_stats(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(group_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let stats = service.task_stats_by_group(&auth.scope, group_id).await?;
    Ok(Json(orchestration_stats_response(&stats)))
}

async fn get_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task = service.get_task(&auth.scope, id).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn get_task_context(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let context = make_task_context_service(&state).for_task(&auth.scope, id).await?;
    Ok(Json(orchestration_task_context_response(&context)))
}

async fn list_task_runs(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let runs = service.list_task_runs(&auth.scope, id).await?;
    Ok(Json(orchestration_task_runs_response(&runs)))
}

async fn list_task_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let comments = service.list_task_comments(&auth.scope, id).await?;
    Ok(Json(orchestration_task_comments_response(&comments)))
}

async fn create_task_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateTaskCommentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let comment = service.create_task_comment(&auth.scope, id, req.kind.as_deref(), &req.body).await?;
    Ok(Json(orchestration_task_comment_response(&comment)))
}

async fn delete_task_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete_task_comment(&auth.scope, id, comment_id).await?;
    Ok(Json(orchestration_delete_response()))
}

async fn latest_human_marks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<LatestHumanMarksQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task_ids: Vec<Uuid> = query.task_ids.split(',').filter_map(|part| Uuid::parse_str(part.trim()).ok()).collect();
    let marks = service.latest_human_marks(&auth.scope, &task_ids).await?;
    Ok(Json(orchestration_human_marks_response(&marks)))
}

async fn export_task_history(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<TaskExportQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let (content, count) = service.export_task_history_csv(&auth.scope, query.limit).await?;
    // The export itself is audited: compliance reviewers can prove who
    // downloaded the team's task history (governance exports already do this).
    let _ = state
        .audit_service()
        .log_action(
            auth.scope.org_id(),
            Some(auth.scope.user_id()),
            "orchestration.task_history.exported",
            "orchestration_task_export",
            None,
            &crate::domain::orchestration::task_history_export_audit_payload(count),
            None,
        )
        .await;
    Ok(Json(orchestration_task_export_response("csv", &content, count)))
}

async fn list_task_review_checks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let checks = service.list_task_review_checks(&auth.scope, id).await?;
    Ok(Json(orchestration_task_review_checks_response(&checks)))
}

async fn set_task_review_check(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, check_key)): Path<(Uuid, String)>,
    Json(req): Json<SetTaskReviewCheckRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let check = service.set_task_review_check(&auth.scope, id, &check_key, req.done).await?;
    Ok(Json(orchestration_task_review_check_response(&check)))
}

async fn patch_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    if req.state.as_deref() == Some("completed") {
        let gates = required_review_gates(&state);
        service.assert_review_gates(&auth.scope, id, &gates).await?;
    }
    let assigned_to = TaskAssignmentPatchPolicy::parse(req.assigned_to.as_deref())?;
    let task = service.update_task(&auth.scope, id, req.state, req.priority, req.progress, assigned_to).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.updated", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn dispatch_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task = service.dispatch_task(&auth.scope, id).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.dispatched", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn publish_task_with_context(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PublishWithContextRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let feature_service = make_feature_service(&state);
    feature_service.ensure_preview_enabled(&auth.scope).await?;
    feature_service.ensure_injection_enabled(&auth.scope).await?;
    let service = make_service(&state);
    let task = make_context_preview_service(&state)
        .publish_existing_task(
            &service,
            &auth.scope,
            id,
            PublishWithContextInput {
                context_preview_id: req.context_preview_id,
                preview_hash: req.preview_hash,
                pinned_item_ids: req.pinned_ids,
                removed_item_ids: req.removed_ids,
            },
        )
        .await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.published", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

/// Required review gates from config (empty = no gate enforced).
fn required_review_gates(state: &AppState) -> Vec<String> {
    state.required_review_gates()
}

async fn review_task_gates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let gates = service.review_gate_status(&auth.scope, id, &required_review_gates(&state)).await?;
    Ok(Json(orchestration_task_review_gates_response(&gates)))
}

async fn complete_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteTaskRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let gates = required_review_gates(&state);
    service.assert_review_gates(&auth.scope, id, &gates).await?;
    let task = service.complete_task(&auth.scope, id, req.result).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.completed", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn fail_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<FailTaskRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task = service.fail_task(&auth.scope, id, req.error).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.failed", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn approve_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    // Live per-org admin gate (#889/F014): approving a task is org-scoped and must
    // verify the caller's current organization_members.role, not the JWT claim.
    state.admin_service().require_org_admin(auth.scope.org_id().as_uuid(), auth.scope.user_id().as_uuid()).await?;
    let service = make_service(&state);
    let task = service.approve_task(&auth.scope, id).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.approved", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn cancel_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task = service.cancel_task(&auth.scope, id).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.canceled", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

async fn retry_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let task = service.retry_task(&auth.scope, id).await?;
    let summary = service.summarize_task(&auth.scope, task).await?;
    service.broadcast_task_update(&auth.scope, "task.retried", &summary).await;
    Ok(Json(orchestration_task_response(&summary)))
}

// ---------------------------------------------------------------------------
// Participant handlers
// ---------------------------------------------------------------------------

async fn register_participant(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<RegisterParticipantRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let participant =
        service.register_participant(&auth.scope, AgentId::from(req.agent_id), &req.name, &req.capabilities).await?;
    let summary = ParticipantSummary::from(participant);
    Ok(Json(orchestration_participant_response(&summary)))
}

async fn list_participants(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListParticipantsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let participants = service.list_participants(&auth.scope, query.status.as_deref()).await?;
    let summaries: Vec<ParticipantSummary> = participants.into_iter().map(Into::into).collect();
    Ok(Json(orchestration_participants_response(&summaries)))
}

async fn participant_heartbeat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let participant = service.participant_heartbeat(&auth.scope, AgentId::from(agent_id)).await?;
    let summary = ParticipantSummary::from(participant);
    Ok(Json(orchestration_participant_response(&summary)))
}

async fn unregister_participant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.unregister_participant(&auth.scope, AgentId::from(agent_id)).await?;
    Ok(Json(orchestration_delete_response()))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn orchestration_routes() -> Router<AppState> {
    Router::new()
        // Tasks
        .route("/orchestration/tasks", get(list_tasks).post(create_task))
        // Literal segment beats the id routes below (matchit static priority).
        .route("/orchestration/tasks/comments/latest", get(latest_human_marks))
        .route("/orchestration/tasks/export", get(export_task_history))
        .route("/orchestration/tasks/{id}/review-checks", get(list_task_review_checks))
        .route("/orchestration/tasks/{id}/review-gates", get(review_task_gates))
        .route("/orchestration/tasks/{id}/review-checks/{check_key}", patch(set_task_review_check))
        .route("/orchestration/tasks/{id}", get(get_task).patch(patch_task))
        .route("/orchestration/tasks/{id}/context", get(get_task_context))
        .route("/orchestration/tasks/{id}/runs", get(list_task_runs))
        .route("/orchestration/tasks/{id}/comments", get(list_task_comments).post(create_task_comment))
        .route("/orchestration/tasks/{id}/comments/{comment_id}", delete(delete_task_comment))
        .route("/orchestration/tasks/{id}/dispatch", post(dispatch_task))
        .route("/orchestration/tasks/{id}/publish-with-context", post(publish_task_with_context))
        .route("/orchestration/tasks/{id}/complete", post(complete_task))
        .route("/orchestration/tasks/{id}/fail", post(fail_task))
        .route("/orchestration/tasks/{id}/approve", post(approve_task))
        .route("/orchestration/tasks/{id}/cancel", post(cancel_task))
        .route("/orchestration/tasks/{id}/retry", post(retry_task))
        // Group-scoped (kanban)
        .route("/orchestration/groups/{group_id}/tasks", get(list_group_tasks))
        .route("/orchestration/groups/{group_id}/tasks/stats", get(group_task_stats))
        .route("/orchestration/groups/{group_id}/tasks/retire-stale", post(retire_stale_tasks))
        // Participants (auto-pickup loop wakes on heartbeat / register)
        .route("/orchestration/participants", get(list_participants).post(register_participant))
        .route("/orchestration/participants/{agent_id}/heartbeat", post(participant_heartbeat))
        .route("/orchestration/participants/{agent_id}", delete(unregister_participant))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retire_stale_request_deserialization() {
        let req: RetireStaleTasksRequest = serde_json::from_str(r#"{"olderThanDays":7,"batchLimit":50}"#).unwrap();
        assert_eq!(req.older_than_days, Some(7));
        assert_eq!(req.batch_limit, Some(50));
        let empty: RetireStaleTasksRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(empty.older_than_days, None);
        assert_eq!(empty.batch_limit, None);
    }

    #[test]
    fn list_tasks_query_defaults() {
        let query: ListTasksQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
        assert!(query.status.is_none());
    }

    #[test]
    fn list_tasks_query_with_status() {
        let query: ListTasksQuery = serde_json::from_str(r#"{"status": "queued", "limit": 50}"#).unwrap();
        assert_eq!(query.status.as_deref(), Some("queued"));
        assert_eq!(query.limit, 50);
    }

    #[test]
    fn create_task_request_minimal_uses_top_level_title() {
        let req: CreateTaskRequest = serde_json::from_str(r#"{"title": "Build feature X"}"#).unwrap();
        let (title, description, params) = extract_params(&req);
        assert_eq!(title, "Build feature X");
        assert!(description.is_none());
        assert!(params.is_none());
    }

    #[test]
    fn create_task_request_uses_legacy_a2a_params() {
        let req: CreateTaskRequest =
            serde_json::from_str(r#"{"params": {"task": "Do thing", "message": "context"}}"#).unwrap();
        let (title, description, params) = extract_params(&req);
        assert_eq!(title, "Do thing");
        assert_eq!(description.as_deref(), Some("context"));
        assert!(params.is_some(), "params JSONB should round-trip for the runtime");
    }

    #[test]
    fn create_task_request_full() {
        let req: CreateTaskRequest = serde_json::from_str(
            r#"{"title": "Sub-task", "description": "details", "priority": "urgent",
                 "groupId": "00000000-0000-0000-0000-000000000010",
                 "assignedTo": "00000000-0000-0000-0000-000000000020",
                 "parentTaskId": "00000000-0000-0000-0000-000000000001",
                 "requiresApproval": true}"#,
        )
        .unwrap();
        assert_eq!(req.priority.as_deref(), Some("urgent"));
        assert!(req.group_id.is_some());
        assert!(req.assigned_to.is_some());
        assert!(req.parent_task_id.is_some());
        assert!(req.requires_approval);
    }

    #[test]
    fn create_task_params_preserve_required_inputs() {
        let req: CreateTaskRequest = serde_json::from_str(
            r#"{"params":{"task":"Deploy","message":"prod","requiredInputs":["ANTHROPIC_API_KEY"],"env":{}}}"#,
        )
        .unwrap();
        let (_title, _description, params) = extract_params(&req);
        let params = params.expect("params");
        assert_eq!(params["requiredInputs"][0], "ANTHROPIC_API_KEY");
        assert!(params.get("env").is_some());
    }

    #[test]
    fn create_task_params_persist_expected_result_for_verifier() {
        // #793/#875: a client POSTing `params.expectedResult` must have it land in
        // the stored params verbatim so the NATS completion verifier fires. End to
        // end: JSON body -> CreateTaskRequest -> extract_params -> stored params.
        let req: CreateTaskRequest = serde_json::from_str(
            r#"{"params":{"task":"Run suite","message":"ci","expectedResult":{"contains":"tests passed"}}}"#,
        )
        .unwrap();
        let (_title, _description, params) = extract_params(&req);
        let params = params.expect("params");
        assert_eq!(
            params["expectedResult"]["contains"], "tests passed",
            "expectedResult must survive the create path into stored params"
        );
    }

    #[test]
    fn parse_assigned_to_handles_unassign_and_uuid() {
        // missing field → leave assignment unchanged
        assert!(matches!(TaskAssignmentPatchPolicy::parse(None), Ok(None)));
        // empty string → explicit unassign
        let unassign = TaskAssignmentPatchPolicy::parse(Some("")).unwrap();
        assert!(matches!(unassign, Some(None)));
        // valid uuid → assign
        let id = TaskAssignmentPatchPolicy::parse(Some("00000000-0000-0000-0000-000000000001")).unwrap();
        assert!(matches!(id, Some(Some(_))));
        // garbage → 400
        assert!(TaskAssignmentPatchPolicy::parse(Some("not-a-uuid")).is_err());
    }

    #[test]
    fn complete_task_request() {
        let req: CompleteTaskRequest = serde_json::from_str(r#"{"result": {"output": "done"}}"#).unwrap();
        assert_eq!(req.result["output"], "done");
    }

    #[test]
    fn fail_task_request() {
        let req: FailTaskRequest = serde_json::from_str(r#"{"error": {"message": "timeout"}}"#).unwrap();
        assert_eq!(req.error["message"], "timeout");
    }

    #[test]
    fn create_task_comment_request_defaults_kind() {
        let req: CreateTaskCommentRequest = serde_json::from_str(r#"{"body": "Checking on this"}"#).unwrap();
        assert_eq!(req.kind, None);
        assert_eq!(req.body, "Checking on this");
    }

    #[test]
    fn task_export_query_defaults_limit() {
        let query: TaskExportQuery = serde_json::from_str("{}").unwrap();
        assert!(query.limit.is_none());
        let with_limit: TaskExportQuery = serde_json::from_str(r#"{"limit": 25}"#).unwrap();
        assert_eq!(with_limit.limit, Some(25));
    }

    #[test]
    fn set_task_review_check_request_deserializes_done() {
        let req: SetTaskReviewCheckRequest = serde_json::from_str(r#"{"done": true}"#).unwrap();
        assert!(req.done);
        let off: SetTaskReviewCheckRequest = serde_json::from_str(r#"{"done": false}"#).unwrap();
        assert!(!off.done);
    }

    #[test]
    fn latest_human_marks_query_parses_task_ids() {
        let query: LatestHumanMarksQuery = serde_json::from_str(
            r#"{"taskIds": "00000000-0000-0000-0000-000000000001, 00000000-0000-0000-0000-000000000002"}"#,
        )
        .unwrap();
        let ids: Vec<Uuid> = query.task_ids.split(',').filter_map(|part| Uuid::parse_str(part.trim()).ok()).collect();
        assert_eq!(ids.len(), 2);
        let empty: LatestHumanMarksQuery = serde_json::from_str(r#"{"taskIds": ""}"#).unwrap();
        assert!(empty.task_ids.is_empty());
    }

    #[test]
    fn create_task_comment_request_accepts_blocker_kind() {
        let req: CreateTaskCommentRequest =
            serde_json::from_str(r#"{"kind": "blocker", "body": "Stuck waiting for the review"}"#).unwrap();
        assert_eq!(req.kind.as_deref(), Some("blocker"));
        assert_eq!(req.body, "Stuck waiting for the review");
    }

    #[test]
    fn register_participant_accepts_snake_or_camel() {
        let snake: RegisterParticipantRequest =
            serde_json::from_str(r#"{"agent_id": "00000000-0000-0000-0000-000000000001", "name": "worker-1"}"#)
                .unwrap();
        assert_eq!(snake.name, "worker-1");

        let camel: RegisterParticipantRequest =
            serde_json::from_str(r#"{"agentId": "00000000-0000-0000-0000-000000000001", "name": "worker-2"}"#).unwrap();
        assert_eq!(camel.name, "worker-2");
    }

    #[test]
    fn list_participants_query_defaults() {
        let query: ListParticipantsQuery = serde_json::from_str("{}").unwrap();
        assert!(query.status.is_none());
    }

    #[test]
    fn list_participants_query_with_status() {
        let query: ListParticipantsQuery = serde_json::from_str(r#"{"status": "available"}"#).unwrap();
        assert_eq!(query.status.as_deref(), Some("available"));
    }
}
