//! Context feedback repository.

use agentforge_core::{AppResult, ErrorKind, ScopedRead, WorkspaceId};
use agentforge_db::entities::{ContextFeedback, MemoryItem, Skill};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct CreateContextFeedbackRecord<'a> {
    pub workspace_id: WorkspaceId,
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: &'a str,
    pub label: &'a str,
    pub note: Option<&'a str>,
}

pub struct ContextFeedbackRepository {
    pool: PgPool,
}

impl ContextFeedbackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn run_status_in_scope_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        run_id: Uuid,
    ) -> AppResult<String> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT status
                 FROM task_runs
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = $3"#,
        )
        .bind(run_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::Forbidden.into())
    }

    pub async fn lock_memory_for_feedback_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        item_id: Uuid,
    ) -> AppResult<MemoryItem> {
        if proof.workspace_ids().is_empty() {
            return Err(ErrorKind::Forbidden.into());
        }

        sqlx::query_as::<_, MemoryItem>(
            r#"SELECT *
                 FROM memory_items
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = $3
                  AND state IN ('active', 'needs_review', 'revoked')
                  AND (
                      (scope_kind = 'user' AND scope_id = $4)
                      OR (scope_kind = 'team' AND scope_id = ANY($5))
                      OR (scope_kind = 'project' AND scope_id = ANY($6))
                  )
                FOR UPDATE"#,
        )
        .bind(item_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(proof.user_id().as_uuid())
        .bind(team_ids(proof))
        .bind(project_ids(proof))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::Forbidden.into())
    }

    pub async fn lock_skill_for_feedback_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        item_id: Uuid,
    ) -> AppResult<Skill> {
        if proof.workspace_ids().is_empty() {
            return Err(ErrorKind::Forbidden.into());
        }

        sqlx::query_as::<_, Skill>(
            r#"SELECT *
                 FROM skills
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = $3
                  AND state <> 'candidate'
                  AND (
                      scope_kind = 'org'
                      OR (scope_kind = 'user' AND scope_id = $4)
                      OR (scope_kind = 'team' AND scope_id = ANY($5))
                      OR (scope_kind = 'project' AND scope_id = ANY($6))
                  )
                FOR UPDATE"#,
        )
        .bind(item_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(proof.user_id().as_uuid())
        .bind(team_ids(proof))
        .bind(project_ids(proof))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::Forbidden.into())
    }

    pub async fn upsert_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        record: CreateContextFeedbackRecord<'_>,
    ) -> AppResult<ContextFeedback> {
        sqlx::query_as::<_, ContextFeedback>(
            r#"INSERT INTO context_feedback (
                   organization_id, workspace_id, run_id, item_id, item_kind, label, note, user_id
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT ON CONSTRAINT context_feedback_unique_user_run_item DO UPDATE
                  SET label = EXCLUDED.label,
                      note = EXCLUDED.note,
                      updated_at = now()
               RETURNING *"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.run_id)
        .bind(record.item_id)
        .bind(record.item_kind)
        .bind(record.label)
        .bind(record.note)
        .bind(proof.user_id().as_uuid())
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn count_label_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        item_id: Uuid,
        item_kind: &str,
        label: &str,
    ) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*)
                 FROM context_feedback
                WHERE organization_id = $1
                  AND workspace_id = $2
                  AND item_id = $3
                  AND item_kind = $4
                  AND label = $5"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(item_id)
        .bind(item_kind)
        .bind(label)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn mark_memory_useful_in_tx(tx: &mut Transaction<'_, Postgres>, item_id: Uuid) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
                  SET last_verified_at = now()
                WHERE id = $1
                RETURNING *"#,
        )
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn mark_memory_stale_in_tx(tx: &mut Transaction<'_, Postgres>, item_id: Uuid) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
                  SET last_verified_at = NULL
                WHERE id = $1
                RETURNING *"#,
        )
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn mark_memory_needs_review_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        item_id: Uuid,
    ) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
                  SET state = 'needs_review'
                WHERE id = $1
                  AND revoked_at IS NULL
                RETURNING *"#,
        )
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_memory_if_active_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        item_id: Uuid,
    ) -> AppResult<Option<MemoryItem>> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
                  SET state = 'revoked',
                      revoked_at = now()
                WHERE id = $1
                  AND revoked_at IS NULL
                RETURNING *"#,
        )
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_skill_if_active_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        item_id: Uuid,
    ) -> AppResult<Option<Skill>> {
        sqlx::query_as::<_, Skill>(
            r#"UPDATE skills
                  SET state = 'revoked',
                      enabled = false,
                      revoked_at = now()
                WHERE id = $1
                  AND revoked_at IS NULL
                RETURNING *"#,
        )
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
    }
}

fn team_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.team_ids().iter().map(|id| id.as_uuid()).collect()
}

fn project_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.project_ids().iter().map(|id| id.as_uuid()).collect()
}
