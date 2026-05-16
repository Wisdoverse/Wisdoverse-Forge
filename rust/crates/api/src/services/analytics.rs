//! Analytics service — event tracking and aggregation.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::AnalyticsEvent;

use crate::domain::observability::{AnalyticsEventName, AnalyticsListPage};
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
        let event_name = AnalyticsEventName::parse(event_name)?;
        self.repo.track(scope, event_name.value(), properties).await
    }

    /// List analytics events with optional filters.
    pub async fn list(
        &self,
        scope: &TenantScope,
        event_name: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<Vec<AnalyticsEvent>> {
        let page = AnalyticsListPage::new(limit, offset);
        self.repo.list(scope, event_name, page.limit(), page.offset()).await
    }

    /// Get aggregate summary stats.
    pub async fn summary(&self, scope: &TenantScope) -> AppResult<serde_json::Value> {
        self.repo.summary(scope).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::observability::{AnalyticsEventName, AnalyticsListPage};

    #[test]
    fn empty_event_name_rejected() {
        assert!(AnalyticsEventName::parse("").is_err());
    }

    #[test]
    fn limit_capped_at_1000() {
        assert_eq!(AnalyticsListPage::new(Some(5000_i64), None).limit(), 1000);
    }

    #[test]
    fn default_limit_is_50() {
        assert_eq!(AnalyticsListPage::new(None, None).limit(), 50);
    }

    #[test]
    fn offset_cannot_be_negative() {
        assert_eq!(AnalyticsListPage::new(None, Some(-10_i64)).offset(), 0);
    }
}
