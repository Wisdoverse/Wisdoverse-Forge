//! Observability domain rules.
//!
//! This module owns validation and pagination policies for analytics events,
//! audit logs, and runtime event streams.

use agentforge_core::{AppResult, ErrorKind};

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
}
