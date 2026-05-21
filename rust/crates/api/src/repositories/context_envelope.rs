//! Context envelope read-model repository.

use std::collections::HashMap;

use agentforge_core::{AgentId, AppResult, ErrorKind, ScopedRead};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ContextEnvelopeMemoryRecord {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub content_redacted: bool,
    pub content_encrypted: bool,
    pub sensitivity: String,
}

#[derive(Clone)]
pub struct ContextEnvelopeRepository {
    pool: PgPool,
}

impl ContextEnvelopeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn verify_run(
        &self,
        proof: &ScopedRead,
        run_id: Uuid,
        task_id: Uuid,
        agent_id: AgentId,
    ) -> AppResult<()> {
        if proof.workspace_ids().is_empty() {
            return Err(ErrorKind::NotFound(format!("task run {run_id}")).into());
        }
        let workspace_ids: Vec<Uuid> = proof.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let found = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id
                 FROM task_runs
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)
                  AND orchestration_task_id = $4
                  AND agent_id = $5"#,
        )
        .bind(run_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids)
        .bind(task_id)
        .bind(agent_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        found.map(|_| ()).ok_or_else(|| ErrorKind::NotFound(format!("task run {run_id}")).into())
    }

    pub async fn applied_memory_content(
        &self,
        proof: &ScopedRead,
        ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, ContextEnvelopeMemoryRecord>> {
        if ids.is_empty() || proof.workspace_ids().is_empty() {
            return Ok(HashMap::new());
        }

        let workspace_ids: Vec<Uuid> = proof.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let team_ids: Vec<Uuid> = proof.team_ids().iter().map(|id| id.as_uuid()).collect();
        let project_ids: Vec<Uuid> = proof.project_ids().iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query_as::<_, ContextEnvelopeMemoryRecord>(
            r#"SELECT id, title, content, content_redacted, content_encrypted, sensitivity
                 FROM memory_items
                WHERE id = ANY($1)
                  AND organization_id = $2
                  AND workspace_id = ANY($3)
                  AND revoked_at IS NULL
                  AND state = 'active'
                  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
                  AND (
                      (scope_kind = 'user' AND scope_id = $4)
                      OR (scope_kind = 'team' AND scope_id = ANY($5))
                      OR (scope_kind = 'project' AND scope_id = ANY($6))
                  )"#,
        )
        .bind(ids)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids)
        .bind(proof.user_id().as_uuid())
        .bind(team_ids)
        .bind(project_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row)).collect())
    }
}
