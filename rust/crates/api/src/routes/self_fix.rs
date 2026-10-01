//! Self-fix loop review + approve endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/self-fix/repository` — platform-admin repository preflight.
//! - `POST /api/v1/self-fix/requests` — deliberate, deduplicated backlog intake.
//! - `GET /api/v1/self-fix/tasks/{id}/trace` — tenant-scoped source/run/PR lineage.
//! - `GET  /api/v1/self-fix/tasks/{id}/review`  — PR review snapshot (diff link,
//!   head SHA, live CI verdict, sensitive flag, review status).
//! - `POST /api/v1/self-fix/tasks/{id}/approve` — operator approval → server-side
//!   guarded merge. The server independently re-verifies non-sensitivity, green
//!   CI, and an unmoved head before merging; sensitive-blocked tasks are HARD
//!   refused here regardless of any client state (plan D4 — this is a dedicated
//!   review surface, NOT the pre-dispatch `waiting_approval` button).
//!
//! All handlers use the standard [`AuthUser`] authentication path. Task calls
//! are tenant-scoped by `auth.scope`; deployment-wide repository metadata is
//! disclosed only after a live platform-admin check.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::domain::self_fix::self_fix_data_response;
use crate::health::AppState;
use crate::services::self_fix::MaintenanceRequestInput;

/// Read-only, live platform-admin setup check. The service verifies the caller
/// before disclosing deployment-wide repository metadata or contacting GitHub.
async fn get_repository(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let setup = state.self_fix_service().repository_setup(&auth.scope).await?;
    Ok(Json(self_fix_data_response(setup)))
}

/// Persist one deliberately submitted source and backlog task, or return the
/// existing link. No assignment, provider comment, branch or PR is created.
async fn submit_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<MaintenanceRequestInput>,
) -> AppResult<Json<Value>> {
    let result = state.self_fix_service().submit_maintenance_request(&auth.scope, input).await?;
    if !result.reused {
        state
            .orchestration_service()
            .broadcast_task_update_by_id(&auth.scope, result.task_id, "maintenance.created")
            .await;
    }
    Ok(Json(self_fix_data_response(result)))
}

async fn get_trace(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let trace = state.self_fix_service().maintenance_trace(&auth.scope, id).await?;
    Ok(Json(self_fix_data_response(trace)))
}

/// `GET /api/v1/self-fix/tasks/{id}/review` — PR review snapshot for a self-fix
/// task. Returns `{ ok: true, data: SelfFixReview }`. `checks_green` is read live
/// and fails closed; `sensitive` is derived server-side. The frontend enables
/// Approve only when `checksGreen && !sensitive`.
async fn get_review(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let snapshot = state.self_fix_service().review_snapshot(&auth.scope, id).await?;
    Ok(Json(self_fix_data_response(snapshot)))
}

/// `POST /api/v1/self-fix/tasks/{id}/approve` — record operator approval and run
/// the server-side guarded merge. The approver identity is the authenticated
/// user's subject claim, recorded in the PR audit comment. The service
/// hard-refuses a sensitive-blocked task and merges only on a confirmed,
/// re-verified, green, unmoved head. Returns `{ ok: true, data: SelfFixMergeResult }`.
async fn approve(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let result = state.self_fix_service().approve_and_merge(&auth.scope, id, &auth.claims.sub.to_string()).await?;
    // Realtime: a confirmed merge flipped `review_status` to `merged`. Push the
    // updated task on the org broadcast subject so every other operator's board
    // badge and Review tab reflect it live, without polling. Best-effort: a
    // broadcast failure never undoes the merge that already succeeded above.
    //
    // A FAILED merge propagates via `?` before this line and broadcasts nothing
    // — by design: the merge gate leaves `review_status` UNCHANGED on failure
    // (no DB write), so there is no status transition to announce. Surfacing a
    // merge *attempt* to other operators is a separate concern, out of scope for
    // this status-change channel.
    state.orchestration_service().broadcast_task_update_by_id(&auth.scope, id, "self_fix.merged").await;
    Ok(Json(self_fix_data_response(result)))
}

/// Self-fix setup/review/approve routes, merged into `/api/v1` behind auth.
pub fn self_fix_routes() -> Router<AppState> {
    Router::new()
        .route("/self-fix/repository", get(get_repository))
        .route("/self-fix/requests", post(submit_request))
        .route("/self-fix/tasks/{id}/trace", get(get_trace))
        .route("/self-fix/tasks/{id}/review", get(get_review))
        .route("/self-fix/tasks/{id}/approve", post(approve))
}
