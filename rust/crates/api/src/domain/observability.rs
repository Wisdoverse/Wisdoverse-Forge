//! Observability domain rules.
//!
//! This module owns validation and pagination policies for analytics events,
//! audit logs, and runtime event streams.

use agentforge_core::{AppResult, ErrorKind};
use agentforge_db::entities::Event;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn analytics_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

/// Analytics event name policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnalyticsEventName<'a> {
    value: &'a str,
}

impl<'a> AnalyticsEventName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("event_name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Analytics list pagination. Preserves the existing no-lower-bound limit rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnalyticsListPage {
    limit: i64,
    offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AnalyticsTopEvent {
    pub(crate) event_name: String,
    pub(crate) count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AnalyticsSummary {
    pub(crate) total_events: i64,
    pub(crate) unique_users: i64,
    pub(crate) top_events: Vec<AnalyticsTopEvent>,
}

impl AnalyticsListPage {
    pub(crate) fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        Self { limit: limit.unwrap_or(50).min(1000), offset: offset.unwrap_or(0).max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Context usage analytics query bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ContextUsageQueryBounds {
    limit: i64,
    min_applied: i64,
    stale_after_days: i64,
    min_success_rate: f64,
    negative_rate: f64,
}

impl ContextUsageQueryBounds {
    pub(crate) fn normalize(
        limit: i64,
        min_applied: i64,
        stale_after_days: i64,
        min_success_rate: f64,
        negative_rate: f64,
    ) -> Self {
        Self {
            limit: limit.clamp(1, 50),
            min_applied: min_applied.clamp(1, 10_000),
            stale_after_days: stale_after_days.clamp(1, 365),
            min_success_rate: min_success_rate.clamp(0.0, 1.0),
            negative_rate: negative_rate.clamp(0.0, 1.0),
        }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn min_applied(self) -> i64 {
        self.min_applied
    }

    pub(crate) fn stale_after_days(self) -> i64 {
        self.stale_after_days
    }

    pub(crate) fn min_success_rate(self) -> f64 {
        self.min_success_rate
    }

    pub(crate) fn negative_rate(self) -> f64 {
        self.negative_rate
    }
}

/// Audit-log list pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuditListPage {
    limit: i64,
    offset: i64,
}

pub(crate) fn audit_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

impl AuditListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Runtime event type policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventType<'a> {
    value: &'a str,
}

impl<'a> EventType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if value.is_empty() {
            return Err(ErrorKind::Validation("event_type must not be empty".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Event stream list pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventListPage {
    limit: i64,
    offset: i64,
}

impl EventListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Event replay limit policy. The max mirrors the frontend MAX_EVENTS contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventReplayPage {
    limit: i64,
}

impl EventReplayPage {
    pub(crate) fn new(limit: i64) -> Self {
        Self { limit: limit.clamp(1, 2000) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn fetch_limit(self) -> i64 {
        self.limit + 1
    }

    pub(crate) fn has_more<T>(self, items: &[T]) -> bool {
        items.len() as i64 > self.limit
    }
}

/// Event replay cursor normalization.
///
/// `after_ts` accepts either Unix milliseconds or RFC3339 because the browser
/// and container-watch paths have both emitted cursor timestamps. Empty or
/// malformed values fall back to epoch/nil so reconnect catch-up never turns a
/// client-side cursor glitch into a server error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventReplayCursor {
    after_ts: DateTime<Utc>,
    after_id: Uuid,
}

impl EventReplayCursor {
    pub(crate) fn from_query(after_ts: Option<&str>, after_id: Option<&str>) -> Self {
        Self {
            after_ts: after_ts.and_then(parse_after_ts).unwrap_or_else(epoch_cursor_ts),
            after_id: after_id.map(parse_after_id).unwrap_or_else(Uuid::nil),
        }
    }

    pub(crate) fn after_ts(self) -> DateTime<Utc> {
        self.after_ts
    }

    pub(crate) fn after_id(self) -> Uuid {
        self.after_id
    }
}

fn epoch_cursor_ts() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).expect("epoch is valid")
}

fn parse_after_ts(raw: &str) -> Option<DateTime<Utc>> {
    if raw.is_empty() {
        return None;
    }
    if let Ok(ms) = raw.parse::<i64>() {
        return DateTime::from_timestamp_millis(ms);
    }
    DateTime::parse_from_rfc3339(raw).ok().map(|dt| dt.with_timezone(&Utc))
}

fn parse_after_id(raw: &str) -> Uuid {
    if raw.is_empty() {
        return Uuid::nil();
    }
    Uuid::parse_str(raw).unwrap_or_else(|_| Uuid::nil())
}

/// Shape an `Event` DB entity into the JSON object `shared/types/events.ts`
/// (ClaudeEvent union) expects: flat keys with `type` + ms `timestamp` +
/// camelCase `sessionId`, with object payload fields spread at the root.
pub(crate) fn event_to_claude_event_json(event: &Event) -> Value {
    let mut out = match &event.payload {
        Value::Object(map) => map.clone(),
        other => {
            let mut map = serde_json::Map::new();
            map.insert("payload".to_owned(), other.clone());
            map
        }
    };

    out.insert("id".to_owned(), json!(event.id.as_uuid()));
    out.insert("type".to_owned(), json!(event.event_type));
    out.insert("timestamp".to_owned(), json!(event.created_at.timestamp_millis()));
    out.insert("sessionId".to_owned(), json!(event.session_id));
    out.insert("orgId".to_owned(), json!(event.organization_id.as_uuid()));
    out.insert("agentId".to_owned(), json!(event.agent_id.as_uuid()));

    Value::Object(out)
}

pub(crate) fn event_ingest_response(event: Event) -> Value {
    json!({ "ok": true, "data": event })
}

pub(crate) fn event_list_response(events: &[Event]) -> Value {
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    json!({ "ok": true, "events": shaped })
}

pub(crate) fn event_replay_cursor_response(events: &[Event], has_more: bool) -> Value {
    let shaped: Vec<Value> = events.iter().map(event_to_claude_event_json).collect();
    json!({ "ok": true, "events": shaped, "hasMore": has_more })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_event(payload: Value) -> Event {
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
    fn analytics_event_name_trims_and_checks_bounds() {
        assert_eq!(AnalyticsEventName::parse(" page.view ").unwrap().value(), "page.view");
        assert!(AnalyticsEventName::parse("").is_err());
        assert!(AnalyticsEventName::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn analytics_list_page_preserves_existing_bounds() {
        assert_eq!(AnalyticsListPage::new(None, None).limit(), 50);
        assert_eq!(AnalyticsListPage::new(Some(5000), Some(0)).limit(), 1000);
        assert_eq!(AnalyticsListPage::new(Some(-1), Some(-10)).limit(), -1);
        assert_eq!(AnalyticsListPage::new(Some(10), Some(-10)).offset(), 0);
    }

    #[test]
    fn context_usage_query_bounds_clamp_low_values() {
        let query = ContextUsageQueryBounds::normalize(0, 0, 0, -0.1, -5.0);

        assert_eq!(query.limit(), 1);
        assert_eq!(query.min_applied(), 1);
        assert_eq!(query.stale_after_days(), 1);
        assert_eq!(query.min_success_rate(), 0.0);
        assert_eq!(query.negative_rate(), 0.0);
    }

    #[test]
    fn context_usage_query_bounds_clamp_high_values() {
        let query = ContextUsageQueryBounds::normalize(500, 20_000, 500, 2.0, 5.0);

        assert_eq!(query.limit(), 50);
        assert_eq!(query.min_applied(), 10_000);
        assert_eq!(query.stale_after_days(), 365);
        assert_eq!(query.min_success_rate(), 1.0);
        assert_eq!(query.negative_rate(), 1.0);
    }

    #[test]
    fn context_usage_query_bounds_preserve_valid_values() {
        let query = ContextUsageQueryBounds::normalize(25, 500, 90, 0.7, 0.3);

        assert_eq!(query.limit(), 25);
        assert_eq!(query.min_applied(), 500);
        assert_eq!(query.stale_after_days(), 90);
        assert_eq!(query.min_success_rate(), 0.7);
        assert_eq!(query.negative_rate(), 0.3);
    }

    #[test]
    fn audit_list_page_clamps_limit_and_offset() {
        assert_eq!(AuditListPage::new(0, -5).limit(), 1);
        assert_eq!(AuditListPage::new(500, 10).limit(), 100);
        assert_eq!(AuditListPage::new(50, -5).offset(), 0);
        assert_eq!(AuditListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn event_type_rejects_empty_only() {
        assert!(EventType::parse("pre_tool_use").is_ok());
        assert!(EventType::parse("").is_err());
        assert!(EventType::parse(" ").is_ok());
    }

    #[test]
    fn event_list_page_clamps_bounds() {
        assert_eq!(EventListPage::new(0, -10).limit(), 1);
        assert_eq!(EventListPage::new(200, 50).limit(), 100);
        assert_eq!(EventListPage::new(50, -10).offset(), 0);
        assert_eq!(EventListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn event_replay_page_caps_and_fetches_one_extra() {
        let page = EventReplayPage::new(5000);
        assert_eq!(page.limit(), 2000);
        assert_eq!(page.fetch_limit(), 2001);
        assert!(EventReplayPage::new(2).has_more(&[1, 2, 3]));
        assert!(!EventReplayPage::new(3).has_more(&[1, 2, 3]));
    }

    #[test]
    fn event_replay_cursor_accepts_rfc3339_after_ts() {
        let cursor =
            EventReplayCursor::from_query(Some("2026-04-22T10:00:00Z"), Some("550e8400-e29b-41d4-a716-446655440000"));

        assert_eq!(cursor.after_ts().to_rfc3339(), "2026-04-22T10:00:00+00:00");
        assert_eq!(cursor.after_id().to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn event_replay_cursor_accepts_unix_millis_after_ts() {
        let cursor = EventReplayCursor::from_query(Some("1745316000000"), None);

        assert_eq!(cursor.after_ts().timestamp_millis(), 1_745_316_000_000);
    }

    #[test]
    fn event_replay_cursor_empty_after_ts_uses_epoch() {
        let cursor = EventReplayCursor::from_query(Some(""), None);

        assert_eq!(cursor.after_ts().timestamp(), 0);
    }

    #[test]
    fn event_replay_cursor_empty_after_id_becomes_nil_uuid() {
        let cursor = EventReplayCursor::from_query(None, Some(""));

        assert_eq!(cursor.after_id(), Uuid::nil());
    }

    #[test]
    fn event_replay_cursor_malformed_after_id_becomes_nil_uuid() {
        let cursor = EventReplayCursor::from_query(None, Some("not-a-uuid"));

        assert_eq!(cursor.after_id(), Uuid::nil());
    }

    #[test]
    fn event_to_json_renames_columns_to_claude_event_keys() {
        let event = fixture_event(json!({}));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["type"], "pre_tool_use");
        assert_eq!(out["sessionId"], "cli-sess-123");
        assert_eq!(out["id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(out["orgId"], "22222222-2222-2222-2222-222222222222");
        assert_eq!(out["agentId"], "33333333-3333-3333-3333-333333333333");
        assert!(out["event_type"].is_null());
        assert!(out["created_at"].is_null());
        assert!(out["organization_id"].is_null());
    }

    #[test]
    fn event_to_json_timestamp_is_unix_millis_not_iso_string() {
        let event = fixture_event(json!({}));
        let out = event_to_claude_event_json(&event);

        assert!(out["timestamp"].is_i64());
        assert_eq!(out["timestamp"].as_i64().unwrap(), event.created_at.timestamp_millis());
    }

    #[test]
    fn event_to_json_spreads_payload_at_root() {
        let event = fixture_event(json!({
            "tool": "Read",
            "toolInput": {"path": "/tmp/x"},
            "toolUseId": "tu_123",
        }));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["tool"], "Read");
        assert_eq!(out["toolInput"]["path"], "/tmp/x");
        assert_eq!(out["toolUseId"], "tu_123");
        assert!(out["payload"].is_null());
    }

    #[test]
    fn event_to_json_base_columns_override_payload_keys() {
        let event = fixture_event(json!({
            "type": "session_end",
            "id": "forged-id",
            "timestamp": 0,
            "sessionId": "forged-session",
        }));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["type"], "pre_tool_use");
        assert_eq!(out["id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(out["sessionId"], "cli-sess-123");
        assert_eq!(out["timestamp"].as_i64().unwrap(), event.created_at.timestamp_millis());
    }

    #[test]
    fn event_to_json_handles_non_object_payload() {
        let event = fixture_event(json!("raw text"));
        let out = event_to_claude_event_json(&event);

        assert_eq!(out["payload"], "raw text");
        assert_eq!(out["type"], "pre_tool_use");
    }

    #[test]
    fn event_to_json_session_id_is_null_when_missing() {
        let mut event = fixture_event(json!({}));
        event.session_id = None;
        let out = event_to_claude_event_json(&event);

        assert!(out.as_object().unwrap().contains_key("sessionId"));
        assert!(out["sessionId"].is_null());
    }

    #[test]
    fn event_responses_own_legacy_envelopes() {
        let event = fixture_event(json!({ "tool": "Read" }));
        let ingest = event_ingest_response(event.clone());
        let list = event_list_response(std::slice::from_ref(&event));
        let replay = event_replay_cursor_response(std::slice::from_ref(&event), false);

        assert_eq!(ingest["ok"], true);
        assert!(ingest["data"]["event_type"].is_string());
        assert_eq!(list["ok"], true);
        assert!(list["events"].is_array());
        assert!(list["data"].is_null());
        assert_eq!(replay["hasMore"], false);
    }
}
