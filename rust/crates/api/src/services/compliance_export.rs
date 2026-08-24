//! Compliance export service — scheduled per-org CSV exports.

use std::path::Path;
use std::time::SystemTime;

use agentforge_core::{AppResult, OrgId, TenantScope};
use chrono::Utc;
use sqlx::PgPool;

use crate::domain::compliance_export::ExportSchedule;
use crate::repositories::compliance_export::ComplianceExportRepository;
use crate::repositories::orchestration::{OrchestrationTaskRepository, TaskHistoryExportRow};
use crate::services::orchestration::task_history_csv;
use crate::services::orchestration::task_history_projection;

/// Business logic layer for scheduled compliance exports.
pub struct ComplianceExportService {
    repo: ComplianceExportRepository,
    task_repo: OrchestrationTaskRepository,
}

impl ComplianceExportService {
    pub fn new(repo: ComplianceExportRepository, task_repo: OrchestrationTaskRepository) -> Self {
        Self { repo, task_repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(ComplianceExportRepository::new(pool.clone()), OrchestrationTaskRepository::new(pool))
    }

    /// One scheduled sweep. Returns how many CSV rows were written; skips
    /// while the `.last_run` marker is younger than the interval.
    pub async fn sweep(&self, dir: &Path, interval_secs: i64) -> AppResult<usize> {
        if interval_secs <= 0 {
            return Ok(0);
        }
        let marker = dir.join(ExportSchedule::MARKER);
        let last = std::fs::metadata(&marker).ok().and_then(|meta| meta.modified().ok());
        if !ExportSchedule::is_due(last, interval_secs, SystemTime::now()) {
            return Ok(0);
        }
        write_dir(dir)?;
        let orgs = self.repo.list_orgs().await?;
        let mut rows_written = 0usize;
        for (org_id, slug) in orgs {
            if !ExportSchedule::safe_slug(&slug) {
                tracing::warn!(slug = %slug, "compliance export: unsafe org slug skipped");
                continue;
            }
            let Some(user_id) = self.repo.any_org_member(OrgId::from(org_id)).await? else {
                continue;
            };
            let scope = TenantScope::with_axes(OrgId::from(org_id), user_id, None, None, None);
            let rows = self.task_repo.export_task_history(&scope, 1000).await?;
            if rows.is_empty() {
                continue;
            }
            let projections: Vec<_> =
                rows.into_iter().map(task_history_projection as fn(TaskHistoryExportRow) -> _).collect();
            let csv = task_history_csv(&projections);
            let org_dir = dir.join(&slug);
            write_dir(&org_dir)?;
            let path = org_dir.join(ExportSchedule::file_name(Utc::now()));
            std::fs::write(&path, csv).map_err(|err| anyhow::anyhow!("compliance export write failed: {err}"))?;
            rows_written += projections.len();
        }
        std::fs::write(&marker, b"ok").map_err(|err| anyhow::anyhow!("compliance marker write failed: {err}"))?;
        Ok(rows_written)
    }
}

fn write_dir(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir)
        .map_err(|err| anyhow::anyhow!("compliance export dir {} not writable: {err}", dir.display()))?;
    Ok(())
}
