use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::auth;
use crate::state::AppState;

use super::errors::AuditError;
use super::model::{AuditFilter, AuditLog};
use super::store::Store;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/", axum::routing::get(list)).route("/export", axum::routing::get(export))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    actor: Option<String>,
    resource: Option<String>,
    resource_id: Option<String>,
    action: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_store(state: &AppState) -> Result<Arc<dyn Store>, Response> {
    state.audit_store.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "database not configured"))
}

fn map_error(err: AuditError) -> Response {
    match err {
        AuditError::InvalidInput(message) => error(StatusCode::BAD_REQUEST, &message),
        AuditError::Internal(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

#[allow(clippy::result_large_err)]
fn parse_datetime(value: Option<String>, field: &str) -> Result<Option<DateTime<Utc>>, Response> {
    let Some(value) = value else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(&value)
        .map(|ts| Some(ts.with_timezone(&Utc)))
        .map_err(|_| error(StatusCode::BAD_REQUEST, &format!("invalid '{field}' date, expected RFC3339 format")))
}

#[allow(clippy::result_large_err)]
fn parse_filter(org_id: String, query: ListQuery) -> Result<AuditFilter, Response> {
    let action = match query.action {
        Some(action) => Some(action.parse().map_err(|_| error(StatusCode::BAD_REQUEST, "invalid action"))?),
        None => None,
    };

    let limit = query
        .limit
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=500).contains(value))
        .unwrap_or(50);
    let offset = query.offset.and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);

    Ok(AuditFilter {
        org_id,
        actor_id: query.actor.filter(|value| !value.trim().is_empty()),
        resource: query.resource.filter(|value| !value.trim().is_empty()),
        resource_id: query.resource_id.filter(|value| !value.trim().is_empty()),
        action,
        from: parse_datetime(query.from, "from")?,
        to: parse_datetime(query.to, "to")?,
        limit,
        offset,
    })
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
    let filter = match parse_filter(identity.org_id, query) {
        Ok(filter) => filter,
        Err(response) => return response,
    };

    match store.list(filter.clone()).await {
        Ok((logs, total)) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "logs": logs,
                "total": total,
                "limit": filter.limit,
                "offset": filter.offset,
            })),
        )
            .into_response(),
        Err(err) => map_error(err),
    }
}

async fn export(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<ListQuery>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };
    let filter = match parse_filter(identity.org_id, query) {
        Ok(filter) => filter,
        Err(response) => return response,
    };

    match store.export(filter).await {
        Ok(logs) => csv_response(&logs),
        Err(err) => map_error(err),
    }
}

fn csv_response(logs: &[AuditLog]) -> Response {
    let mut csv =
        String::from("id,action,actor_id,actor_type,resource,resource_id,org_id,ip_address,user_agent,created_at");
    csv.push(char::from(10));
    for log in logs {
        let row = [
            log.id.clone(),
            log.action.to_string(),
            log.actor_id.clone(),
            log.actor_type.clone(),
            log.resource.clone(),
            log.resource_id.clone().unwrap_or_default(),
            log.org_id.clone(),
            log.ip_address.clone().unwrap_or_default(),
            log.user_agent.clone().unwrap_or_default(),
            log.created_at.to_rfc3339(),
        ];
        csv.push_str(&row.into_iter().map(csv_escape).collect::<Vec<_>>().join(","));
        csv.push('\n');
    }

    let mut response = Response::new(axum::body::Body::from(csv));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=audit_{}.csv", Utc::now().format("%Y%m%d_%H%M%S")))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=audit_export.csv")),
    );
    response
}

fn csv_escape(value: String) -> String {
    if value.chars().any(|ch| ch == ',' || ch == '"' || ch == char::from(10) || ch == char::from(13)) {
        let mut escaped = String::with_capacity(value.len() + 2);
        let doubled_quote: String = ['"', '"'].into_iter().collect();
        escaped.push('"');
        escaped.push_str(&value.replace('"', doubled_quote.as_str()));
        escaped.push('"');
        escaped
    } else {
        value
    }
}
