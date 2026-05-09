//! Typed access to run-scoped evidence projections.

use agentforge_core::{AppResult, TenantScope};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::task_run::{RunEvidenceRow, TaskRunRepository};

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(rename = "organizationId")]
    pub organization_id: Uuid,
    #[serde(rename = "workspaceId", skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceId")]
    pub source_id: Uuid,
    pub payload: serde_json::Value,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

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
