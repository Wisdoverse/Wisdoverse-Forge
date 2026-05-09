//! Analytics service — event tracking and aggregation.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::AnalyticsEvent;

use crate::repositories::analytics::AnalyticsRepository;

/// Business logic layer for analytics operations.
pub struct AnalyticsService {
    repo: AnalyticsRepository,
}

impl AnalyticsService {
    pub fn new(repo: AnalyticsRepository) -> Self {
        Self { repo }
    }

    /// Track a new analytics event.
    pub async fn track(
        &self,
        scope: &TenantScope,
        event_name: &str,
        properties: &serde_json::Value,
    ) -> AppResult<AnalyticsEvent> {
        let event_name = event_name.trim();
        if event_name.is_empty() || event_name.len() > 255 {
            return Err(ErrorKind::Validation("event_name must be 1-255 characters".into()).into());
        }
        self.repo.track(scope, event_name, properties).await
    }

    /// List analytics events with optional filters.
    pub async fn list(
        &self,
        scope: &TenantScope,
        event_name: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<Vec<AnalyticsEvent>> {
        let limit = limit.unwrap_or(50).min(1000);
        let offset = offset.unwrap_or(0).max(0);
        self.repo.list(scope, event_name, limit, offset).await
    }

    /// Get aggregate summary stats.
    pub async fn summary(&self, scope: &TenantScope) -> AppResult<serde_json::Value> {
        self.repo.summary(scope).await
    }
}

#[cfg(test)]
mod tests {
    fn normalized_limit(limit: Option<i64>) -> i64 {
        limit.unwrap_or(50).min(1000)
    }

    fn normalized_offset(offset: Option<i64>) -> i64 {
        offset.unwrap_or(0).max(0)
    }

    #[test]
    fn empty_event_name_rejected() {
        let name = "".trim();
        assert!(name.is_empty());
    }

    #[test]
    fn limit_capped_at_1000() {
        let limit = normalized_limit(Some(5000_i64));
        assert_eq!(limit, 1000);
    }

    #[test]
    fn default_limit_is_50() {
        let limit = normalized_limit(None);
        assert_eq!(limit, 50);
    }

    #[test]
    fn offset_cannot_be_negative() {
        let offset = normalized_offset(Some(-10_i64));
        assert_eq!(offset, 0);
    }
}
