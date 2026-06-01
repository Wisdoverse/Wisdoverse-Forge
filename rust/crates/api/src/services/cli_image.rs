//! CLI agent-image auto-updater status service (deployment-global, read-only).
//!
//! Coordinates the worker's in-memory per-tool snapshot with the cross-tenant
//! container counts and deployment config, then hands the merged shape to the
//! domain projection. Holds no tenant scope — image state is per host, not per
//! org — and is only reachable from the admin-gated status route.

use std::collections::BTreeMap;
use std::sync::Arc;

use sqlx::PgPool;

use agentforge_core::{AppConfig, AppResult};
use agentforge_jobs::{
    CliImageUpdateStatus, configured_image_tag, configured_registry, effective_interval_secs, pollable_tool_names,
};

use crate::repositories::admin::AdminRepository;

pub(crate) use crate::domain::cli_image::cli_image_status_response;
pub use crate::domain::cli_image::{CliImageStatusReport, CliImageToolStatus};

pub struct CliImageService {
    repo: AdminRepository,
    status: Arc<CliImageUpdateStatus>,
    auto_update_enabled: bool,
    poll_interval_secs: u64,
    prune_configured: bool,
}

impl CliImageService {
    pub fn from_runtime(pool: PgPool, status: Arc<CliImageUpdateStatus>, config: &AppConfig) -> Self {
        Self {
            repo: AdminRepository::new(pool),
            status,
            auto_update_enabled: config.cli_image_auto_update_enabled,
            // Report the EFFECTIVE cadence (post-floor), so the panel never
            // claims a faster poll rate than the worker actually runs.
            poll_interval_secs: effective_interval_secs(config.cli_image_auto_update_interval_secs),
            // Operator INTENT — surfaced even when the worker never ran a sweep
            // (e.g. prune on but auto-update off), so a misconfiguration is not
            // reported as a reassuring "off".
            prune_configured: config.cli_image_prune_enabled,
        }
    }

    /// Build the read-only status report for `GET /admin/cli-images`.
    pub async fn status_report(&self) -> AppResult<CliImageStatusReport> {
        let snapshot = self.status.snapshot().await;
        let prune = self.status.prune_snapshot().await;
        let counts: BTreeMap<String, i64> = self.repo.container_agent_counts_by_tool().await?.into_iter().collect();
        let tool_names = pollable_tool_names();

        Ok(CliImageStatusReport::build(
            self.auto_update_enabled,
            self.poll_interval_secs,
            configured_registry(),
            configured_image_tag(),
            &tool_names,
            &snapshot,
            &counts,
            self.prune_configured,
            prune,
        ))
    }
}
