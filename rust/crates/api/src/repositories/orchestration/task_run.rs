//! Task-run repository — tenant-scoped execution attempts and evidence reads.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::{OrchestrationTask, TaskRun};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::orchestration::OrchestrationRepositoryPolicy;

/// Typed row projected by `v_run_evidence`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RunEvidenceRow {
    pub run_id: Option<Uuid>,
    pub organization_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub source_type: String,
    pub source_id: Uuid,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Database access layer for task execution attempts.
pub struct TaskRunRepository {
    pool: PgPool,
}

impl TaskRunRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn create_for_assignment_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: &OrchestrationTask,
        idempotency_key: &str,
        capability_profile: serde_json::Value,
    ) -> AppResult<TaskRun> {
        let agent_id = task
            .assigned_agent_id
            .ok_or_else(|| OrchestrationRepositoryPolicy::missing_assigned_agent_for_task_run(task.id))?;

        sqlx::query_as::<_, TaskRun>(
            r#"INSERT INTO task_runs
                   (id, organization_id, workspace_id, orchestration_task_id, agent_id,
                    idempotency_key, status, started_at, capability_profile)
               SELECT $1, $2, agent.workspace_id, $3, $4, $5, 'working',
                      COALESCE($6, NOW()), $7
                 FROM agents agent
                WHERE agent.id = $4
                  AND agent.organization_id = $2
               ON CONFLICT (orchestration_task_id, idempotency_key) DO UPDATE
                  SET status = CASE
                          WHEN task_runs.finished_at IS NULL THEN EXCLUDED.status
                          ELSE task_runs.status
                      END,
                      updated_at = NOW()
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(scope.org_id().as_uuid())
        .bind(task.id)
        .bind(agent_id.as_uuid())
        .bind(idempotency_key)
        .bind(task.started_at)
        .bind(capability_profile)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_run_agent_not_found(agent_id))
    }

    pub async fn finish_current_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task_id: Uuid,
        status: &str,
    ) -> AppResult<Option<TaskRun>> {
        if !matches!(status, "completed" | "failed" | "canceled") {
            return Err(OrchestrationRepositoryPolicy::invalid_terminal_task_run_status(status));
        }

        let row = sqlx::query_as::<_, TaskRun>(
            r#"WITH current_run AS (
                   SELECT id
                     FROM task_runs
                    WHERE organization_id = $1
                      AND orchestration_task_id = $2
                      AND finished_at IS NULL
                    ORDER BY started_at DESC, created_at DESC, id DESC
                    LIMIT 1
               )
               UPDATE task_runs
                  SET status = $3,
                      finished_at = COALESCE(finished_at, NOW()),
                      updated_at = NOW()
                WHERE id = (SELECT id FROM current_run)
                RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .bind(status)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row)
    }

    pub async fn list_by_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<Vec<TaskRun>> {
        let rows = sqlx::query_as::<_, TaskRun>(
            r#"SELECT *
                 FROM task_runs
                WHERE organization_id = $1
                  AND orchestration_task_id = $2
                ORDER BY started_at ASC, id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_by_id(&self, scope: &TenantScope, run_id: Uuid) -> AppResult<TaskRun> {
        sqlx::query_as::<_, TaskRun>(
            r#"SELECT *
                 FROM task_runs
                WHERE organization_id = $1
                  AND id = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_run_not_found(run_id))
    }

    pub async fn evidence_for_run(&self, scope: &TenantScope, run_id: Uuid) -> AppResult<Vec<RunEvidenceRow>> {
        let rows = sqlx::query_as::<_, RunEvidenceRow>(
            r#"SELECT run_id, organization_id, workspace_id, agent_id,
                      source_type, source_id, payload, created_at
                 FROM v_run_evidence
                WHERE organization_id = $1
                  AND run_id = $2
                ORDER BY created_at ASC, source_type ASC, source_id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn legacy_evidence_for_agent(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
    ) -> AppResult<Vec<RunEvidenceRow>> {
        let rows = sqlx::query_as::<_, RunEvidenceRow>(
            r#"SELECT run_id, organization_id, workspace_id, agent_id,
                      source_type, source_id, payload, created_at
                 FROM v_run_evidence
                WHERE organization_id = $1
                  AND agent_id = $2
                  AND run_id IS NULL
                ORDER BY created_at ASC, source_type ASC, source_id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
