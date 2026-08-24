//! Compliance export policy — schedule math, filenames and dir safety.

use chrono::{DateTime, Utc};
use std::time::SystemTime;

/// Scheduled-export rules.
pub(crate) struct ExportSchedule;

impl ExportSchedule {
    pub(crate) const MARKER: &str = ".last_run";

    /// Due when the last run marker is older than the interval (or absent).
    pub(crate) fn is_due(last: Option<SystemTime>, interval_secs: i64, now: SystemTime) -> bool {
        if interval_secs <= 0 {
            return false;
        }
        let Some(last) = last else {
            return true;
        };
        let elapsed = now.duration_since(last).unwrap_or_default().as_secs() as i64;
        elapsed >= interval_secs
    }

    /// Timestamped CSV filename (UTC).
    pub(crate) fn file_name(at: DateTime<Utc>) -> String {
        format!("agentforge-compliance-{}.csv", at.format("%Y%m%d-%H%M%S"))
    }

    /// Org slugs used as directory names must stay within a safe charset so
    /// a crafted slug cannot escape the export root.
    pub(crate) fn safe_slug(slug: &str) -> bool {
        !slug.is_empty() && slug.len() <= 64 && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_due_math() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
        assert!(ExportSchedule::is_due(None, 3600, now), "no marker means due");
        assert!(!ExportSchedule::is_due(Some(now), 3600, now), "just ran, not due");
        let old = now - std::time::Duration::from_secs(3601);
        assert!(ExportSchedule::is_due(Some(old), 3600, now), "older than interval");
        assert!(!ExportSchedule::is_due(Some(old), 0, now), "interval 0 = off");
    }

    #[test]
    fn file_name_is_timestamped_and_safe() {
        let at = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z").unwrap().with_timezone(&Utc);
        assert_eq!(ExportSchedule::file_name(at), "agentforge-compliance-20260102-030405.csv");
    }

    #[test]
    fn slug_safety() {
        assert!(ExportSchedule::safe_slug("team-org_2"));
        assert!(!ExportSchedule::safe_slug(""));
        assert!(!ExportSchedule::safe_slug("../etc"));
        assert!(!ExportSchedule::safe_slug("a".repeat(65).as_str()));
    }
}
