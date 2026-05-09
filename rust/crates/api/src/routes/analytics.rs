//! Analytics endpoints (nested under `/api/v1`).
//!
//! - `POST /analytics/events`  — track event
//! - `GET  /analytics/events`  — list events
//! - `GET  /analytics/summary` — aggregate stats
//! - `GET  /analytics/context-usage` — governed context usage analytics

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::{AppState, ContextFeature, ensure_context_feature_enabled};
use crate::repositories::analytics::AnalyticsRepository;
use crate::services::analytics::AnalyticsService;
use crate::services::usage_analytics::{ContextUsageQuery, UsageAnalyticsService};

/// Request body for tracking an analytics event.
#[derive(Deserialize)]
pub struct TrackEventRequest {
    pub event_name: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

/// Query parameters for listing analytics events.
#[derive(Deserialize)]
pub struct ListEventsQuery {
    pub event_name: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for governed context usage analytics.
#[derive(Deserialize)]
pub struct ContextUsageQueryParams {
    pub limit: Option<i64>,
    pub min_applied: Option<i64>,
    pub stale_after_days: Option<i64>,
    pub min_success_rate: Option<f64>,
    pub negative_rate: Option<f64>,
}

/// Build an AnalyticsService from shared state.
fn make_service(state: &AppState) -> AnalyticsService {
    AnalyticsService::new(AnalyticsRepository::new(state.pool.clone()))
}

fn make_usage_service(state: &AppState) -> UsageAnalyticsService {
    UsageAnalyticsService::new(state.pool.clone())
}

/// `POST /analytics/events` — track an event.
async fn track_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<TrackEventRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let event = service.track(&auth.scope, &req.event_name, &req.properties).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": event })))
}

/// `GET /analytics/events` — list events.
async fn list_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListEventsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let events = service.list(&auth.scope, q.event_name.as_deref(), q.limit, q.offset).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": events })))
}

/// `GET /analytics/summary` — aggregate stats.
async fn summary(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let summary = service.summary(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": summary })))
}

/// `GET /analytics/context-usage` — governed context usage analytics snapshot.
async fn context_usage(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ContextUsageQueryParams>,
) -> AppResult<Json<serde_json::Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Analytics).await?;
    let service = make_usage_service(&state);
    let data = service
        .context_usage(
            &auth.scope,
            ContextUsageQuery {
                limit: q.limit.unwrap_or(10),
                min_applied: q.min_applied.unwrap_or(10),
                stale_after_days: q.stale_after_days.unwrap_or(30),
                min_success_rate: q.min_success_rate.unwrap_or(0.70),
                negative_rate: q.negative_rate.unwrap_or(0.30),
            },
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": data })))
}

/// Build analytics routes sub-router.
pub fn analytics_routes() -> Router<AppState> {
    Router::new()
        .route("/analytics/events", post(track_event).get(list_events))
        .route("/analytics/summary", get(summary))
        .route("/analytics/context-usage", get(context_usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_event_request_deserialization() {
        let req: TrackEventRequest =
            serde_json::from_str(r#"{"event_name": "page_view", "properties": {"page": "/home"}}"#).unwrap();
        assert_eq!(req.event_name, "page_view");
        assert_eq!(req.properties["page"], "/home");
    }

    #[test]
    fn track_event_request_minimal() {
        let req: TrackEventRequest = serde_json::from_str(r#"{"event_name": "click"}"#).unwrap();
        assert_eq!(req.event_name, "click");
        // Default properties is empty object or null
    }

    #[test]
    fn list_events_query_deserialization() {
        let q: ListEventsQuery = serde_json::from_str(r#"{"event_name": "login", "limit": 10, "offset": 5}"#).unwrap();
        assert_eq!(q.event_name.as_deref(), Some("login"));
        assert_eq!(q.limit, Some(10));
        assert_eq!(q.offset, Some(5));
    }

    #[test]
    fn list_events_query_empty() {
        let q: ListEventsQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert!(q.event_name.is_none());
        assert!(q.limit.is_none());
        assert!(q.offset.is_none());
    }
}
