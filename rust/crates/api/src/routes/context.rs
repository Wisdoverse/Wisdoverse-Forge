//! Context approval, envelope, feedback, and preview endpoints.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::domain::memory::MemoryScopeKind;
use crate::health::{AppState, ContextFeature, ensure_context_feature_enabled};
use crate::services::context::{
    ApproveContextCandidateInput, ContextApprovalService, ContextFeedbackLabel, ContextFeedbackService,
    ContextItemKind, ListContextCandidatesInput, RecordContextFeedbackInput, RejectContextCandidateInput,
};
use crate::services::context_envelope::{ContextEnvelopeInput, ContextEnvelopeService};
use crate::services::context_preview::{ContextPreviewService, CreateContextPreviewInput};

#[derive(Debug, Deserialize)]
struct ListCandidatesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    state: Option<String>,
    item_kind: Option<String>,
    scope_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApproveContextCandidateRequest {
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    pub ttl_at: Option<DateTime<Utc>>,
    pub sensitivity: Option<String>,
    pub reason: Option<String>,
    #[serde(default)]
    pub redacted: bool,
    #[serde(default)]
    pub user_attested: bool,
    #[serde(default)]
    pub confirm_expansion: bool,
}

#[derive(Debug, Deserialize)]
pub struct RejectContextCandidateRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordContextFeedbackRequest {
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: ContextItemKind,
    pub label: ContextFeedbackLabel,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContextEnvelopeRequest {
    pub agent_id: Uuid,
    pub task_id: Uuid,
    pub run_id: Uuid,
    #[serde(default)]
    pub supported_versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateContextPreviewRequest {
    #[serde(rename = "taskId")]
    pub task_id: Uuid,
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFeatureSnapshot {
    pub governance: bool,
    pub preview: bool,
    pub injection: bool,
    pub analytics: bool,
}

fn make_service(state: &AppState) -> ContextApprovalService {
    ContextApprovalService::new(state.pool.clone(), Some(state.nats.clone()))
}

fn make_feedback_service(state: &AppState) -> ContextFeedbackService {
    ContextFeedbackService::new(state.pool.clone())
}

fn make_envelope_service(state: &AppState) -> ContextEnvelopeService {
    ContextEnvelopeService::new(state.pool.clone(), state.context_resolver.clone())
}

fn make_preview_service(state: &AppState) -> ContextPreviewService {
    ContextPreviewService::new(
        crate::repositories::context_preview::ContextPreviewRepository::new(state.pool.clone()),
        crate::repositories::orchestration::OrchestrationTaskRepository::new(state.pool.clone()),
        crate::repositories::orchestration::ParticipantRepository::new(state.pool.clone()),
        state.context_resolver.clone(),
    )
}

async fn get_context_features(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let data = ContextFeatureSnapshot {
        governance: state.context_feature_enabled(&auth.scope, ContextFeature::Governance).await?,
        preview: state.context_feature_enabled(&auth.scope, ContextFeature::Preview).await?,
        injection: state.context_feature_enabled(&auth.scope, ContextFeature::Injection).await?,
        analytics: state.context_feature_enabled(&auth.scope, ContextFeature::Analytics).await?,
    };
    Ok(Json(json!({ "ok": true, "data": data })))
}

async fn list_pending_candidates(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListCandidatesQuery>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    let candidates = make_service(&state)
        .list(
            &auth.scope,
            ListContextCandidatesInput {
                state: query.state,
                item_kind: query.item_kind,
                scope_kind: query.scope_kind,
                limit: query.limit,
                offset: query.offset,
            },
        )
        .await?;
    Ok(Json(json!({ "ok": true, "data": candidates })))
}

async fn approve_candidate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveContextCandidateRequest>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    let outcome = make_service(&state)
        .approve(
            &auth.scope,
            id,
            ApproveContextCandidateInput {
                scope_kind: req.scope_kind,
                scope_id: req.scope_id,
                ttl_at: req.ttl_at,
                sensitivity: req.sensitivity,
                reason: req.reason,
                redacted: req.redacted,
                user_attested: req.user_attested,
                confirm_expansion: req.confirm_expansion,
            },
        )
        .await?;
    Ok(Json(json!({ "ok": true, "data": outcome })))
}

async fn reject_candidate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RejectContextCandidateRequest>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    let outcome =
        make_service(&state).reject(&auth.scope, id, RejectContextCandidateInput { reason: req.reason }).await?;
    Ok(Json(json!({ "ok": true, "data": outcome })))
}

async fn record_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<RecordContextFeedbackRequest>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    let outcome = make_feedback_service(&state)
        .record(
            &auth.scope,
            RecordContextFeedbackInput {
                run_id: req.run_id,
                item_id: req.item_id,
                item_kind: req.item_kind,
                label: req.label,
                note: req.note,
            },
        )
        .await?;
    Ok(Json(json!({ "ok": true, "data": outcome })))
}

async fn build_context_envelope(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ContextEnvelopeRequest>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Injection).await?;
    let envelope = make_envelope_service(&state)
        .build(
            &auth.scope.scoped_read(),
            ContextEnvelopeInput {
                task_id: req.task_id,
                run_id: req.run_id,
                agent_id: agentforge_core::AgentId::from(req.agent_id),
                supported_versions: req.supported_versions,
            },
        )
        .await?;
    Ok(Json(json!({ "ok": true, "data": envelope })))
}

async fn create_context_preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateContextPreviewRequest>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Preview).await?;
    let preview = make_preview_service(&state)
        .create(
            &auth.scope,
            CreateContextPreviewInput { task_id: req.task_id, agent_id: agentforge_core::AgentId::from(req.agent_id) },
        )
        .await?;
    Ok(Json(json!({ "ok": true, "data": preview })))
}

pub fn context_routes() -> Router<AppState> {
    Router::new()
        .route("/context/features", get(get_context_features))
        .route("/context/envelope", post(build_context_envelope))
        .route("/context/previews", post(create_context_preview))
        .route("/context/candidates", get(list_pending_candidates))
        .route("/context/candidates/{id}/approve", post(approve_candidate))
        .route("/context/candidates/{id}/reject", post(reject_candidate))
        .route("/context/feedback", post(record_feedback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_candidate_request_defaults_flags() {
        let req: ApproveContextCandidateRequest =
            serde_json::from_str(r#"{"scope_kind":"user","sensitivity":"internal"}"#).expect("request");
        assert_eq!(req.scope_kind, MemoryScopeKind::User);
        assert!(!req.redacted);
        assert!(!req.user_attested);
        assert!(!req.confirm_expansion);
    }
}
