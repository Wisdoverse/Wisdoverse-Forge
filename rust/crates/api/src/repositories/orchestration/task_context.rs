//! Read-side projection for the task detail Context tab.

use agentforge_core::{AppResult, TenantScope, WorkspaceId};
use agentforge_db::entities::{ContextCandidate, TaskRun};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::repositories::orchestration::task_run::RunEvidenceRow;

#[derive(Debug, Clone, FromRow)]
pub struct AppliedContextRow {
    pub injection_id: Uuid,
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: String,
    pub position: i32,
    pub adapter: String,
    pub envelope_version: String,
    pub capability_profile: serde_json::Value,
    pub applied_snapshot: serde_json::Value,
    pub degradation_reason: Option<String>,
    pub applied_at: DateTime<Utc>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub item_state: Option<String>,
    pub sensitivity: Option<String>,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub feedback_label: Option<String>,
    pub feedback_note: Option<String>,
    pub feedback_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct TaskContextRepository {
    pool: PgPool,
}

impl TaskContextRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn runs_for_task(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        task_id: Uuid,
    ) -> AppResult<Vec<TaskRun>> {
        let rows = sqlx::query_as::<_, TaskRun>(
            r#"SELECT *
                 FROM task_runs
                WHERE organization_id = $1
                  AND workspace_id = $2
                  AND orchestration_task_id = $3
                ORDER BY started_at DESC, id DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn applied_for_runs(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        run_ids: &[Uuid],
    ) -> AppResult<Vec<AppliedContextRow>> {
        if run_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, AppliedContextRow>(
            r#"SELECT
                    rci.id AS injection_id,
                    rci.run_id,
                    rci.item_id,
                    rci.item_kind,
                    rci.position,
                    rci.adapter,
                    rci.envelope_version,
                    rci.capability_profile,
                    rci.applied_snapshot,
                    rci.degradation_reason,
                    rci.applied_at,
                    COALESCE(mi.scope_kind, sk.scope_kind) AS scope_kind,
                    COALESCE(mi.scope_id, sk.scope_id) AS scope_id,
                    COALESCE(mi.state, sk.state) AS item_state,
                    COALESCE(mi.sensitivity, sk.sensitivity) AS sensitivity,
                    mi.source_task_id,
                    mi.source_run_id,
                    mi.last_used_at,
                    mi.last_verified_at,
                    COALESCE(mi.revoked_at, sk.revoked_at) AS revoked_at,
                    cf.label AS feedback_label,
                    cf.note AS feedback_note,
                    cf.updated_at AS feedback_updated_at
               FROM run_context_injections rci
               LEFT JOIN memory_items mi
                 ON rci.item_kind = 'memory'
                AND mi.id = rci.item_id
                AND mi.organization_id = rci.organization_id
                AND mi.workspace_id = rci.workspace_id
               LEFT JOIN skills sk
                 ON rci.item_kind = 'skill'
                AND sk.id = rci.item_id
                AND sk.organization_id = rci.organization_id
                AND sk.workspace_id = rci.workspace_id
               LEFT JOIN context_feedback cf
                 ON cf.organization_id = rci.organization_id
                AND cf.workspace_id = rci.workspace_id
                AND cf.run_id = rci.run_id
                AND cf.item_id = rci.item_id
                AND cf.item_kind = rci.item_kind
                AND cf.user_id = $4
              WHERE rci.organization_id = $1
                AND rci.workspace_id = $2
                AND rci.run_id = ANY($3)
              ORDER BY rci.applied_at DESC, rci.run_id DESC, rci.position ASC, rci.id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(run_ids)
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn candidates_for_runs(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        run_ids: &[Uuid],
    ) -> AppResult<Vec<ContextCandidate>> {
        if run_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, ContextCandidate>(
            r#"SELECT *
                 FROM context_candidates
                WHERE organization_id = $1
                  AND workspace_id = $2
                  AND source_run_id = ANY($3)
                ORDER BY created_at DESC, id DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(run_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn evidence_for_runs(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        run_ids: &[Uuid],
    ) -> AppResult<Vec<RunEvidenceRow>> {
        if run_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, RunEvidenceRow>(
            r#"SELECT run_id, organization_id, workspace_id, agent_id,
                      source_type, source_id, payload, created_at
                 FROM v_run_evidence
                WHERE organization_id = $1
                  AND workspace_id = $2
                  AND run_id = ANY($3)
                ORDER BY created_at DESC, source_type ASC, source_id ASC
                LIMIT 200"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(run_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
