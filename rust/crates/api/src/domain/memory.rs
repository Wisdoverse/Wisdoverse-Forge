//! Memory domain rules.
//!
//! This module owns pure memory item input, pagination, and retention policies
//! that are independent of repositories, HTTP route DTOs, and audit emission.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

/// Memory list pagination policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryListPage {
    limit: i64,
    offset: i64,
}

impl MemoryListPage {
    pub(crate) fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        Self { limit: limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT), offset: offset.unwrap_or(0).max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Validated memory title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryTitle<'a> {
    value: &'a str,
}

impl<'a> MemoryTitle<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("memory title must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Memory visibility policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryVisibility {
    Private,
    Shared,
}

impl MemoryVisibility {
    pub(crate) fn parse(value: Option<&str>) -> AppResult<Self> {
        match value.unwrap_or("shared") {
            "private" => Ok(Self::Private),
            "shared" => Ok(Self::Shared),
            other => Err(ErrorKind::Validation(format!("unsupported memory visibility `{other}`")).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

/// Memory TTL policy.
pub(crate) struct MemoryTtlPolicy;

impl MemoryTtlPolicy {
    pub(crate) fn validate(ttl_expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> AppResult<()> {
        if let Some(ttl) = ttl_expires_at
            && ttl <= now
        {
            return Err(ErrorKind::Validation("ttl_expires_at must be in the future".into()).into());
        }
        Ok(())
    }
}

/// Memory confidence score policy.
pub(crate) struct MemoryConfidencePolicy;

impl MemoryConfidencePolicy {
    pub(crate) fn validate(confidence: Option<f64>) -> AppResult<()> {
        if let Some(value) = confidence
            && !(0.0..=1.0).contains(&value)
        {
            return Err(ErrorKind::Validation("confidence must be between 0 and 1".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn memory_list_page_clamps_limit_and_offset() {
        assert_eq!(MemoryListPage::new(None, None).limit(), 50);
        assert_eq!(MemoryListPage::new(Some(0), Some(-10)).limit(), 1);
        assert_eq!(MemoryListPage::new(Some(500), Some(10)).limit(), 200);
        assert_eq!(MemoryListPage::new(Some(20), Some(-10)).offset(), 0);
        assert_eq!(MemoryListPage::new(Some(20), Some(10)).offset(), 10);
    }

    #[test]
    fn memory_title_trims_and_checks_bounds() {
        assert_eq!(MemoryTitle::parse(" title ").unwrap().value(), "title");
        assert!(MemoryTitle::parse("").is_err());
        assert!(MemoryTitle::parse("   ").is_err());
        assert!(MemoryTitle::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn memory_visibility_defaults_and_rejects_unsupported_values() {
        assert_eq!(MemoryVisibility::parse(None).unwrap().as_str(), "shared");
        assert_eq!(MemoryVisibility::parse(Some("private")).unwrap().as_str(), "private");
        assert_eq!(MemoryVisibility::parse(Some("shared")).unwrap().as_str(), "shared");
        assert!(MemoryVisibility::parse(Some("team")).is_err());
    }

    #[test]
    fn memory_ttl_requires_future_expiry_when_present() {
        let now = Utc::now();

        assert!(MemoryTtlPolicy::validate(None, now).is_ok());
        assert!(MemoryTtlPolicy::validate(Some(now + Duration::seconds(1)), now).is_ok());
        assert!(MemoryTtlPolicy::validate(Some(now), now).is_err());
        assert!(MemoryTtlPolicy::validate(Some(now - Duration::seconds(1)), now).is_err());
    }

    #[test]
    fn memory_confidence_accepts_unit_interval_only() {
        assert!(MemoryConfidencePolicy::validate(None).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(0.0)).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(1.0)).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(-0.1)).is_err());
        assert!(MemoryConfidencePolicy::validate(Some(1.1)).is_err());
    }
}
