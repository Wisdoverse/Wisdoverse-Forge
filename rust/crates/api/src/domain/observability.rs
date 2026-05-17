//! Observability domain rules.
//!
//! This module owns validation and pagination policies for analytics events,
//! audit logs, and runtime event streams.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use uuid::Uuid;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
