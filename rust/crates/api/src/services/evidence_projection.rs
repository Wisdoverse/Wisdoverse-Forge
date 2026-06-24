//! Typed access to run-scoped evidence projections.

use agentforge_core::{AppResult, TenantScope};
use uuid::Uuid;

pub use crate::domain::evidence_projection::Evidence;
use crate::repositories::orchestration::task_run::{RunEvidenceRow, TaskRunRepository};

pub struct EvidenceProjectionService {
    task_run_repo: TaskRunRepository,
}

impl EvidenceProjectionService {
    pub fn new(task_run_repo: TaskRunRepository) -> Self {
        Self { task_run_repo }
    }

    pub async fn for_run(&self, scope: &TenantScope, run_id: Uuid) -> AppResult<Vec<Evidence>> {
        self.task_run_repo.find_by_id(scope, run_id).await?;
        let rows = self.task_run_repo.evidence_for_run(scope, run_id).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn legacy_for_agent(&self, scope: &TenantScope, agent_id: Uuid) -> AppResult<Vec<Evidence>> {
        let rows = self.task_run_repo.legacy_evidence_for_agent(scope, agent_id).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

impl From<RunEvidenceRow> for Evidence {
    fn from(row: RunEvidenceRow) -> Self {
        Self {
            run_id: row.run_id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            agent_id: row.agent_id,
            source_type: row.source_type,
            source_id: row.source_id,
            payload: row.payload,
            created_at: row.created_at,
        }
    }
}
