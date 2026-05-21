//! Governed context usage analytics read model.

use std::time::Instant;

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use chrono::Utc;
use sqlx::PgPool;

pub use crate::domain::usage_analytics::{
    ContextUsageAnalyticsResponse, ContextUsageItem, ContextUsageQuery, ContextUsageQuerySummary, ContextUsageSummary,
};
use crate::repositories::usage_analytics::UsageAnalyticsRepository;

const STALE_AFTER_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Refreshed,
    SkippedLocked,
}

#[derive(Debug, Clone)]
pub struct UsageAnalyticsService {
    repo: UsageAnalyticsRepository,
}

impl UsageAnalyticsService {
    pub fn new(pool: PgPool) -> Self {
        Self { repo: UsageAnalyticsRepository::new(pool) }
    }

    pub async fn context_usage(
        &self,
        scope: &TenantScope,
        query: ContextUsageQuery,
    ) -> AppResult<ContextUsageAnalyticsResponse> {
        let Some(workspace_id) = scope.workspace_id() else {
            return Err(ErrorKind::Forbidden.into());
        };
        let query = query.normalized();
        let refresh = self.repo.refresh_status().await?;
        let summary = self.repo.summary(scope, workspace_id).await?;
        let top_useful = self.repo.top_useful(scope, workspace_id, query).await?;
        let stale_items = self.repo.stale_items(scope, workspace_id, query).await?;
        let needs_review = self.repo.needs_review(scope, workspace_id, query).await?;
        let is_stale = refresh.last_refresh_error.is_some()
            || Utc::now().signed_duration_since(refresh.last_refreshed_at).num_hours() > STALE_AFTER_HOURS;

        Ok(ContextUsageAnalyticsResponse {
            last_refreshed_at: refresh.last_refreshed_at,
            last_refresh_started_at: refresh.last_refresh_started_at,
            last_refresh_error: refresh.last_refresh_error,
            stale_after_hours: STALE_AFTER_HOURS,
            is_stale,
            query: query.into(),
            summary,
            top_useful,
            stale_items,
            needs_review,
        })
    }

    pub async fn refresh_context_usage_snapshot(&self) -> AppResult<RefreshOutcome> {
        let got_lock = self.repo.try_acquire_refresh_lock().await?;
        if !got_lock {
            metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "skipped_locked").increment(1);
            return Ok(RefreshOutcome::SkippedLocked);
        }

        let started = Instant::now();
        let result = self.repo.refresh_snapshot().await;
        if let Err(err) = self.repo.release_refresh_lock().await {
            tracing::warn!(error = ?err, "failed to unlock context usage analytics refresh advisory lock");
        }

        match result {
            Ok(()) => {
                metrics::histogram!("context_usage_analytics_refresh_seconds").record(started.elapsed().as_secs_f64());
                metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "success").increment(1);
                Ok(RefreshOutcome::Refreshed)
            }
            Err(err) => {
                metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "error").increment(1);
                Err(err)
            }
        }
    }
}

pub fn register_usage_analytics_metrics() {
    metrics::describe_counter!(
        "context_usage_analytics_refresh_total",
        "Count of context usage analytics materialized view refresh outcomes."
    );
    metrics::describe_histogram!(
        "context_usage_analytics_refresh_seconds",
        "Duration of successful context usage analytics materialized view refreshes."
    );
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "success").increment(0);
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "error").increment(0);
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "skipped_locked").increment(0);
}
