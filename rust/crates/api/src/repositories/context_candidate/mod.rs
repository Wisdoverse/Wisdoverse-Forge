//! Context candidate aggregate — approval queue state, approvals, and feedback.

pub mod approval;
pub mod feedback;

pub use approval::{ContextApprovalRepository, CreateContextApprovalRecord};
pub use feedback::{ContextFeedbackRepository, CreateContextFeedbackRecord};

use agentforge_core::{AppResult, OrgId, ScopedRead, SkillId, TenantScope, UserId, WorkspaceId};
use agentforge_db::entities::ContextCandidate;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::context::ContextCandidatePolicy;

pub struct CreateContextCandidateRecord<'a> {
    pub workspace_id: WorkspaceId,
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<Uuid>,
    pub item_kind: &'a str,
    pub proposed_content: &'a Value,
    pub owner_user_id: UserId,
}

pub struct ContextCandidateRepository {
    pool: PgPool,
}

#[derive(Debug, Clone, FromRow)]
pub struct ContextCandidateListRow {
    pub id: Uuid,
    pub organization_id: OrgId,
    pub workspace_id: WorkspaceId,
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<SkillId>,
    pub item_kind: String,
    pub proposed_content: Value,
    pub state: String,
    pub owner_user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub proposed_scope_kind: String,
    pub source_available: bool,
}

impl ContextCandidateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        record: CreateContextCandidateRecord<'_>,
    ) -> AppResult<ContextCandidate> {
        sqlx::query_as::<_, ContextCandidate>(
            r#"INSERT INTO context_candidates (
                   organization_id, workspace_id, source_run_id, target_skill_id,
                   item_kind, proposed_content, owner_user_id
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.source_run_id)
        .bind(record.target_skill_id)
        .bind(record.item_kind)
        .bind(record.proposed_content)
        .bind(record.owner_user_id.as_uuid())
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn list_pending(&self, proof: &ScopedRead, limit: i64, offset: i64) -> AppResult<Vec<ContextCandidate>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, ContextCandidate>(
            r#"SELECT *
                 FROM context_candidates
                WHERE organization_id = $1
                  AND workspace_id = ANY($2)
                  AND state = 'pending'
                ORDER BY created_at DESC, id DESC
                LIMIT $3 OFFSET $4"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn list_visible(
        &self,
        proof: &ScopedRead,
        state: Option<&str>,
        item_kind: Option<&str>,
        scope_kind: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ContextCandidateListRow>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, ContextCandidateListRow>(
            r#"SELECT c.id,
                      c.organization_id,
                      c.workspace_id,
                      c.source_run_id,
                      c.target_skill_id,
                      c.item_kind,
                      c.proposed_content,
                      c.state,
                      c.owner_user_id,
                      c.created_at,
                      c.updated_at,
                      COALESCE(NULLIF(c.proposed_content ->> 'scope_kind', ''), s.scope_kind, 'user') AS proposed_scope_kind,
                      COALESCE(tr.status = 'completed', FALSE) AS source_available
                 FROM context_candidates c
                 LEFT JOIN task_runs tr
                   ON tr.id = c.source_run_id
                  AND tr.organization_id = c.organization_id
                  AND tr.workspace_id = c.workspace_id
                 LEFT JOIN skills s
                   ON s.id = c.target_skill_id
                  AND s.organization_id = c.organization_id
                  AND s.workspace_id = c.workspace_id
                WHERE c.organization_id = $1
                  AND c.workspace_id = ANY($2)
                  AND ($3::TEXT IS NULL OR c.state = $3)
                  AND ($4::TEXT IS NULL OR c.item_kind = $4)
                  AND ($5::TEXT IS NULL OR COALESCE(NULLIF(c.proposed_content ->> 'scope_kind', ''), s.scope_kind, 'user') = $5)
                ORDER BY CASE WHEN c.state = 'pending' THEN 0 ELSE 1 END,
                         c.created_at DESC,
                         c.id DESC
                LIMIT $6 OFFSET $7"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .bind(state)
        .bind(item_kind)
        .bind(scope_kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn lock_visible_for_update(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        id: Uuid,
    ) -> AppResult<ContextCandidate> {
        if proof.workspace_ids().is_empty() {
            return Err(ContextCandidatePolicy::not_found(id));
        }

        sqlx::query_as::<_, ContextCandidate>(
            r#"SELECT *
                 FROM context_candidates
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)
                FOR UPDATE"#,
        )
        .bind(id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ContextCandidatePolicy::not_found(id))
    }

    pub async fn update_state_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        state: &str,
    ) -> AppResult<ContextCandidate> {
        sqlx::query_as::<_, ContextCandidate>(
            r#"UPDATE context_candidates
                  SET state = $2,
                      updated_at = now()
                WHERE id = $1
                RETURNING *"#,
        )
        .bind(id)
        .bind(state)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn source_run_status_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        source_run_id: Uuid,
    ) -> AppResult<Option<String>> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT status
                 FROM task_runs
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)"#,
        )
        .bind(source_run_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .fetch_optional(&mut **tx)
        .await
        .map_err(Into::into)
    }
}

fn workspace_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.workspace_ids().iter().map(|id| id.as_uuid()).collect()
}
