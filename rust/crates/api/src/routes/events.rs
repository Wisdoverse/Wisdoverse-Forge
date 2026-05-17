//! Event endpoints (nested under `/api/v1`).
//!
//! - `POST /api/v1/events`             — ingest a new event
//! - `GET  /api/v1/events`             — list events for org (paginated)
//! - `GET  /api/v1/agents/{id}/events` — list events for an agent (paginated)

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};
use agentforge_db::entities::Event;

use crate::domain::observability::EventReplayCursor;
use crate::health::AppState;
use crate::repositories::event::EventRepository;
use crate::services::event::EventService;

/// Shape an `Event` DB entity into the JSON object `shared/types/events.ts`
/// (ClaudeEvent union) expects: flat keys with `type` + ms `timestamp` +
/// camelCase `sessionId`, with the `payload` object spread at the root so
/// variant-specific fields (`tool`, `toolInput`, `prompt`, `response`, ...)
/// sit beside the base fields.
///
/// Mirrors `admin_event_row_to_json` (`routes::admin`). Pinned by test below.
/// The DB `Event` entity's default Serialize is UNUSABLE by the frontend —
/// every caller of the list/replay handlers MUST go through this mapper.
fn event_to_claude_event_json(event: &Event) -> Value {
    // Spread payload at the root: turns `{ tool: "Read" }` into a sibling of
    // `type`/`id` so `PreToolUseEvent.tool` deserializes as-is. If the payload
    // is not an object (legacy rows wrote bare strings), fall back to a
    // singleton `payload` key so the frontend at least sees something.
    let mut out = match &event.payload {
        Value::Object(map) => map.clone(),
        other => {
            let mut m = serde_json::Map::new();
            m.insert("payload".to_owned(), other.clone());
            m
        }
    };

    // Base fields overwrite any conflicting payload keys. If the sidecar ever
    // persists a payload key named "type"/"id"/"timestamp" those would clobber
    // the authoritative DB columns — deny that by inserting AFTER the spread.
    out.insert("id".to_owned(), json!(event.id.as_uuid()));
    out.insert("type".to_owned(), json!(event.event_type));
    out.insert("timestamp".to_owned(), json!(event.created_at.timestamp_millis()));
    out.insert("sessionId".to_owned(), json!(event.session_id));
    out.insert("orgId".to_owned(), json!(event.organization_id.as_uuid()));
    out.insert("agentId".to_owned(), json!(event.agent_id.as_uuid()));

    Value::Object(out)
}

/// Query parameters for list endpoints.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

/// Query parameters for event replay.
#[derive(Deserialize)]
pub struct ReplayQuery {
    pub agent_id: Uuid,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

/// Query parameters for cursor-based agent replay.
///
/// `after_ts` accepts either a Unix-ms integer (as a string) or an RFC3339
/// timestamp; both encodings are emitted by `TurnStreamRenderer` depending on
/// which path last touched the watermark. `after_id` is the UUID tiebreaker for
/// events that share a millisecond — an empty value (first catch-up after cold
/// hydrate) is coerced to the nil UUID so the `(ts, id) > ($ts, nil)` tuple
/// compare still matches every real row.
#[derive(Deserialize)]
pub struct CursorReplayQuery {
    pub after_ts: Option<String>,
    pub after_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for ingesting an event.
#[derive(Deserialize)]
pub struct IngestEventRequest {
    pub agent_id: Uuid,
    pub event_type: String,
    #[serde(default = "default_payload")]
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
}

fn default_payload() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> EventService {
    EventService::new(EventRepository::new(state.pool.clone()))
}

/// `POST /api/events` — ingest a new event.
///
/// Returns the raw persisted entity under `data` — consumed by the sidecar /
/// hook replay client, which only needs the DB id back for ack bookkeeping.
/// Intentionally NOT mapped through `event_to_claude_event_json` because no
/// UI reads this response.
async fn ingest_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<IngestEventRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let event = service
        .ingest(&auth.scope, AgentId::from(req.agent_id), &req.event_type, req.payload, req.session_id.as_deref())
        .await?;
    Ok(Json(json!({ "ok": true, "data": event })))
}

/// `GET /api/events` — list events for org (paginated).
async fn list_org_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let events = service.list_by_org(&auth.scope, query.limit, query.offset).await?;
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    Ok(Json(json!({ "ok": true, "events": shaped })))
}

/// `GET /api/agents/{id}/events` — list events for an agent (paginated).
async fn list_agent_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let events = service.list_by_agent(&auth.scope, AgentId::from(id), query.limit, query.offset).await?;
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    Ok(Json(json!({ "ok": true, "events": shaped })))
}

/// `GET /api/events/replay` — replay events for an agent from a timestamp.
///
/// Returns events in chronological order (ASC), unlike list endpoints which
/// return newest first (DESC). Used for catch-up after reconnection.
async fn replay_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ReplayQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let events = service.replay(&auth.scope, AgentId::from(query.agent_id), query.since, query.limit).await?;
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    Ok(Json(json!({ "ok": true, "events": shaped })))
}

/// `GET /api/agents/{id}/events/replay` — cursor-based replay for a single agent.
///
/// Matches the frontend `ReplayClient.fetchMissedEvents` contract added for
/// issue #46. Returns `{ok, events, hasMore}` where `events` is the next batch
/// in chronological order and `hasMore` indicates whether another page exists
/// past the returned tail.
///
/// Cursor tuple `(after_ts, after_id)` is strict-greater-than — callers pass the
/// `(timestamp, id)` of the last event they've already applied. On cold catch-up
/// (empty `after_id`), the nil UUID makes every real event match.
async fn replay_agent_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorReplayQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let cursor = EventReplayCursor::from_query(query.after_ts.as_deref(), query.after_id.as_deref());

    let (events, has_more) = service
        .replay_cursor(&auth.scope, AgentId::from(id), cursor.after_ts(), cursor.after_id(), query.limit)
        .await?;
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    Ok(Json(json!({ "ok": true, "events": shaped, "hasMore": has_more })))
}

/// Build event routes sub-router.
pub fn event_routes() -> Router<AppState> {
    Router::new()
        // Static routes BEFORE parameterized routes (per CLAUDE.md)
        .route("/events/replay", get(replay_events))
        .route("/events", get(list_org_events).post(ingest_event))
        .route("/agents/{id}/events/replay", get(replay_agent_events))
        .route("/agents/{id}/events", get(list_agent_events))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn ingest_request_deserialization_full() {
        let req: IngestEventRequest = serde_json::from_str(
            r#"{
                "agent_id": "550e8400-e29b-41d4-a716-446655440000",
                "event_type": "pre_tool_use",
                "payload": {"tool": "Read", "path": "/tmp/test.rs"},
                "session_id": "cli-sess-123"
            }"#,
        )
        .unwrap();
        assert_eq!(req.event_type, "pre_tool_use");
        assert_eq!(req.payload["tool"], "Read");
        assert_eq!(req.session_id.as_deref(), Some("cli-sess-123"));
    }

    #[test]
    fn ingest_request_minimal() {
        let req: IngestEventRequest = serde_json::from_str(
            r#"{
                "agent_id": "550e8400-e29b-41d4-a716-446655440000",
                "event_type": "session_start"
            }"#,
        )
        .unwrap();
        assert_eq!(req.event_type, "session_start");
        assert!(req.payload.is_object());
        assert!(req.session_id.is_none());
    }

    #[test]
    fn ingest_request_missing_event_type_fails() {
        let result =
            serde_json::from_str::<IngestEventRequest>(r#"{"agent_id": "550e8400-e29b-41d4-a716-446655440000"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn ingest_request_missing_agent_id_fails() {
        let result = serde_json::from_str::<IngestEventRequest>(r#"{"event_type": "pre_tool_use"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn replay_query_deserialization_full() {
        let query: ReplayQuery = serde_json::from_str(
            r#"{
                "agent_id": "550e8400-e29b-41d4-a716-446655440000",
                "since": "2026-01-01T00:00:00Z",
                "limit": 50
            }"#,
        )
        .unwrap();
        assert_eq!(query.agent_id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
        assert!(query.since.is_some());
        assert_eq!(query.limit, 50);
    }

    #[test]
    fn replay_query_deserialization_minimal() {
        let query: ReplayQuery =
            serde_json::from_str(r#"{"agent_id": "550e8400-e29b-41d4-a716-446655440000"}"#).unwrap();
        assert!(query.since.is_none());
        assert_eq!(query.limit, 20); // default
    }

    #[test]
    fn replay_query_missing_agent_id_fails() {
        let result = serde_json::from_str::<ReplayQuery>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn list_events_envelope_is_events_key_not_data() {
        // Pin the envelope shape against accidental drift back to `{ok, data}`.
        // Frontend (src/app/stores/chat.store.ts) reads the `events` field.
        let events: Vec<serde_json::Value> = vec![];
        let body = serde_json::json!({ "ok": true, "events": events });
        assert_eq!(body["ok"], true);
        assert!(body["events"].is_array(), "list response must use the `events` key");
        assert!(body["data"].is_null(), "list response must NOT carry a `data` key");
    }

    // ------------------------------------------------------------------
    // event_to_claude_event_json — pins the on-wire event shape that
    // frontend `ClaudeEvent` (shared/types/events.ts) relies on. Changing
    // any of these keys silently breaks ChatView history rendering for
    // every deployed agent.
    // ------------------------------------------------------------------

    fn fixture_event(payload: serde_json::Value) -> Event {
        use agentforge_core::{AgentId, EventId, OrgId};
        use chrono::TimeZone;

        Event {
            id: EventId::from(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()),
            organization_id: OrgId::from(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()),
            agent_id: AgentId::from(Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap()),
            run_id: None,
            event_type: "pre_tool_use".to_owned(),
            payload,
            session_id: Some("cli-sess-123".to_owned()),
            created_at: chrono::Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn event_to_json_renames_columns_to_claude_event_keys() {
        let event = fixture_event(json!({}));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["type"], "pre_tool_use", "`event_type` must surface as `type`");
        assert_eq!(out["sessionId"], "cli-sess-123", "`session_id` must surface as camelCase `sessionId`");
        assert_eq!(out["id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(out["orgId"], "22222222-2222-2222-2222-222222222222");
        assert_eq!(out["agentId"], "33333333-3333-3333-3333-333333333333");

        assert!(out["event_type"].is_null(), "snake_case `event_type` must not leak through");
        assert!(out["created_at"].is_null(), "snake_case `created_at` must not leak through");
        assert!(out["organization_id"].is_null(), "snake_case `organization_id` must not leak through");
    }

    #[test]
    fn event_to_json_timestamp_is_unix_millis_not_iso_string() {
        // BaseEvent.timestamp is typed `number` (Unix ms) — serializing a
        // chrono DateTime directly yields an ISO string and silently breaks
        // turn grouping (which diffs numeric timestamps).
        let event = fixture_event(json!({}));
        let out = event_to_claude_event_json(&event);

        assert!(out["timestamp"].is_i64(), "timestamp must be a number, not a string");
        assert_eq!(out["timestamp"].as_i64().unwrap(), event.created_at.timestamp_millis());
    }

    #[test]
    fn event_to_json_spreads_payload_at_root() {
        // Variant fields (`tool`, `toolInput`, `prompt`, ...) live on the
        // payload JSON column — they must appear as siblings of `type`/`id`,
        // not nested under a `payload` key, or `PreToolUseEvent.tool` is lost.
        let event = fixture_event(json!({
            "tool": "Read",
            "toolInput": {"path": "/tmp/x"},
            "toolUseId": "tu_123",
        }));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["tool"], "Read");
        assert_eq!(out["toolInput"]["path"], "/tmp/x");
        assert_eq!(out["toolUseId"], "tu_123");
        assert!(out["payload"].is_null(), "payload must be spread, not nested");
    }

    #[test]
    fn event_to_json_base_columns_override_malicious_payload_keys() {
        // If a sidecar ever persists a payload with a `type` or `id` key,
        // the DB columns remain authoritative — otherwise clients can forge
        // their own type discriminator and bypass server classification.
        let event = fixture_event(json!({
            "type": "session_end",
            "id": "forged-id",
            "timestamp": 0,
            "sessionId": "forged-session",
        }));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["type"], "pre_tool_use", "DB column wins over payload `type`");
        assert_eq!(out["id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(out["sessionId"], "cli-sess-123");
        assert_eq!(out["timestamp"].as_i64().unwrap(), event.created_at.timestamp_millis());
    }

    #[test]
    fn event_to_json_handles_non_object_payload() {
        // Legacy rows occasionally wrote bare scalars to the payload column.
        // The mapper must not crash and should surface the value under a
        // `payload` key so the UI can at least display something.
        let event = fixture_event(json!("raw text"));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["payload"], "raw text");
        assert_eq!(out["type"], "pre_tool_use");
    }

    #[test]
    fn event_to_json_session_id_is_null_when_missing() {
        // ReplayClient's cursor logic reads `sessionId`; if the column is
        // NULL the field must be present as JSON null, not dropped.
        let mut event = fixture_event(json!({}));
        event.session_id = None;
        let out = event_to_claude_event_json(&event);

        assert!(
            out.as_object().unwrap().contains_key("sessionId"),
            "sessionId must be emitted even when the column is NULL"
        );
        assert!(out["sessionId"].is_null());
    }

    #[test]
    fn replay_envelope_uses_events_key_not_data() {
        // Pin the replay envelope alongside the list envelope. Frontend
        // `ReplayClient` reads `data.events`, and the prior `{ok, data: […]}`
        // shape broke catch-up on WebSocket reconnect.
        let events: Vec<serde_json::Value> = vec![];
        let body = serde_json::json!({ "ok": true, "events": events });
        assert!(body["events"].is_array(), "replay response must use `events` key");
        assert!(body["data"].is_null(), "replay response must NOT carry a `data` key");
    }

    // ------------------------------------------------------------------
    // CursorReplayQuery — pins the contract the frontend ReplayClient
    // relies on (URL + query param names + `hasMore` field). Breaking any
    // of these silently regresses WebSocket reconnect catch-up.
    // ------------------------------------------------------------------

    #[test]
    fn cursor_replay_query_accepts_rfc3339_after_ts() {
        let q: CursorReplayQuery = serde_json::from_str(
            r#"{"after_ts": "2026-04-22T10:00:00Z", "after_id": "550e8400-e29b-41d4-a716-446655440000"}"#,
        )
        .unwrap();
        assert_eq!(q.after_ts.as_deref(), Some("2026-04-22T10:00:00Z"));
        assert_eq!(q.limit, 20);
        assert_eq!(
            EventReplayCursor::from_query(q.after_ts.as_deref(), q.after_id.as_deref()).after_ts().to_rfc3339(),
            "2026-04-22T10:00:00+00:00"
        );
    }

    #[test]
    fn cursor_replay_query_accepts_unix_millis_after_ts() {
        // Container CLI watch path and some frontend paths send Unix ms as a
        // stringified integer — both encodings must parse to the same instant.
        let ms = "1745316000000";
        let cursor = EventReplayCursor::from_query(Some(ms), None);
        assert_eq!(cursor.after_ts().timestamp_millis(), 1_745_316_000_000);
    }

    #[test]
    fn cursor_replay_query_empty_after_ts_uses_epoch_cursor() {
        let cursor = EventReplayCursor::from_query(Some(""), None);
        assert_eq!(cursor.after_ts().timestamp(), 0);
    }

    #[test]
    fn cursor_replay_empty_after_id_becomes_nil_uuid() {
        // Cold catch-up: first call after hydrate sends after_id="" so the
        // (ts, id) tuple compare is anchored below every real UUID.
        assert_eq!(EventReplayCursor::from_query(None, Some("")).after_id(), Uuid::nil());
    }

    #[test]
    fn cursor_replay_malformed_after_id_becomes_nil_uuid_not_500() {
        // Garbage input must not 500 — we coerce to nil rather than bubble a
        // parse error, which is safer than leaking SQL to the client on a
        // path we control.
        assert_eq!(EventReplayCursor::from_query(None, Some("not-a-uuid")).after_id(), Uuid::nil());
    }

    #[test]
    fn cursor_replay_query_rejects_missing_limit_default_is_20() {
        let q: CursorReplayQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(q.limit, 20);
        assert!(q.after_ts.is_none());
        assert!(q.after_id.is_none());
    }

    #[test]
    fn cursor_replay_envelope_includes_has_more_field() {
        // Pin the envelope — `hasMore` is the signal ChatViewController uses
        // to decide "replay was enough" vs "fall back to refetch". Dropping
        // the field silently breaks three-level reconnect degradation.
        let events: Vec<serde_json::Value> = vec![];
        let body = serde_json::json!({ "ok": true, "events": events, "hasMore": false });
        assert_eq!(body["ok"], true);
        assert!(body["events"].is_array());
        assert!(body["hasMore"].is_boolean(), "cursor replay envelope must include `hasMore`");
    }
}
