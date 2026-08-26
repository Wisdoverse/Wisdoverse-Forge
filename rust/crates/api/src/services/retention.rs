//! Retention service — governed telemetry purging wrapper.

use agentforge_core::AppResult;
use sqlx::PgPool;

use crate::repositories::retention::RetentionRepository;

/// Retention policy runner.
pub struct RetentionService {
    repo: RetentionRepository,
}

impl RetentionService {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { repo: RetentionRepository::new(pool) }
    }

    /// Purge expired telemetry; returns (events, analytics_events) removed.
    pub async fn sweep(&self, days: i64) -> AppResult<(u64, u64)> {
        let removed = self.repo.purge_telemetry(days).await?;
        if removed.0 + removed.1 > 0 {
            tracing::info!(events = removed.0, analytics = removed.1, "retention purge removed expired telemetry");
        }
        Ok(removed)
    }

    /// Purge finished runs of terminal tasks older than `days`.
    pub async fn sweep_runs(&self, days: i64) -> AppResult<u64> {
        let removed = self.repo.purge_finished_runs(days).await?;
        if removed > 0 {
            tracing::info!(runs = removed, "retention purge removed finished runs");
        }
        Ok(removed)
    }
}
