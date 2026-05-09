use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::auth;
use crate::state::AppState;
use crate::task::TaskState;

use super::errors::ReviewError;
use super::model::{AddCommentRequest, CodeReview, CreateReviewRequest, ReviewComment, ReviewFilter, ReviewState};
use super::store::Store;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::get(list).post(create))
        .route("/{id}", axum::routing::get(get))
        .route("/{id}/approve", axum::routing::post(approve))
        .route("/{id}/reject", axum::routing::post(reject))
        .route("/{id}/comments", axum::routing::post(add_comment))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    task_id: Option<String>,
    state: Option<ReviewState>,
}

#[derive(Debug, Default, Deserialize)]
struct RejectRequest {
    #[allow(dead_code)]
    feedback: Option<String>,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_store(state: &AppState) -> Result<Arc<dyn Store>, Response> {
    state.review_store.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "database not configured"))
}

fn map_error(err: ReviewError) -> Response {
    match err {
        ReviewError::NotFound => error(StatusCode::NOT_FOUND, "review not found"),
        ReviewError::InvalidInput(message) => error(StatusCode::BAD_REQUEST, &message),
        ReviewError::Internal(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
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
        .list(ReviewFilter {
            org_id: identity.org_id,
            task_id: query.task_id,
            state: query.state,
            limit: 50,
            offset: 0,
        })
        .await
    {
        Ok(reviews) => (StatusCode::OK, Json(json!({"ok": true, "reviews": reviews}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn create(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateReviewRequest>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if req.task_id.trim().is_empty() || req.session_id.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "taskId and sessionId are required");
    }

    let mut review = CodeReview {
        id: String::new(),
        task_id: req.task_id.clone(),
        session_id: req.session_id,
        diff_ref: if req.diff_ref.trim().is_empty() { "manual".to_string() } else { req.diff_ref },
        diff_snapshot: None,
        state: ReviewState::Pending,
        assigned_to: req.assigned_to,
        org_id: identity.org_id.clone(),
        created_by: identity.user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    match store.create(&mut review).await {
        Ok(()) => {
            if let Some(task_store) = state.task_store.as_ref() {
                let _ = task_store.set_review_id(&review.task_id, &identity.org_id, review.id.clone()).await;
            }
            (StatusCode::CREATED, Json(json!({"ok": true, "review": review}))).into_response()
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

    match store.get_by_id(&id, &identity.org_id).await {
        Ok(review) => (StatusCode::OK, Json(json!({"ok": true, "review": review}))).into_response(),
        Err(err) => map_error(err),
    }
}

async fn approve(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    let review = match store.get_by_id(&id, &identity.org_id).await {
        Ok(review) => review,
        Err(err) => return map_error(err),
    };
    if let Err(err) = store.update_state(&id, &identity.org_id, ReviewState::Approved).await {
        return map_error(err);
    }
    if let Some(task_store) = state.task_store.as_ref() {
        let _ = task_store.update_state(&review.review.task_id, &identity.org_id, TaskState::Completed).await;
    }

    (StatusCode::OK, Json(json!({"ok": true, "state": ReviewState::Approved}))).into_response()
}

async fn reject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_req): Json<RejectRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    let review = match store.get_by_id(&id, &identity.org_id).await {
        Ok(review) => review,
        Err(err) => return map_error(err),
    };
    if let Err(err) = store.update_state(&id, &identity.org_id, ReviewState::ChangesRequested).await {
        return map_error(err);
    }
    if let Some(task_store) = state.task_store.as_ref() {
        let _ = task_store.update_state(&review.review.task_id, &identity.org_id, TaskState::ChangesRequested).await;
    }

    (StatusCode::OK, Json(json!({"ok": true, "state": ReviewState::ChangesRequested}))).into_response()
}

async fn add_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AddCommentRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if req.body.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "body is required");
    }

    let mut comment = ReviewComment {
        id: String::new(),
        review_id: String::new(),
        author_id: identity.user_id,
        body: req.body,
        file_path: req.file_path,
        line: req.line,
        created_at: chrono::Utc::now(),
    };

    match store.add_comment(&id, &identity.org_id, &mut comment).await {
        Ok(()) => (StatusCode::CREATED, Json(json!({"ok": true, "comment": comment}))).into_response(),
        Err(err) => map_error(err),
    }
}
