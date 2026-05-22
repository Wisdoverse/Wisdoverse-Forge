//! Governed memory item repository.

use agentforge_core::{AppResult, MemoryItemId, ScopeKind, ScopedRead, ScopedWrite, WorkspaceId};
use agentforge_db::entities::MemoryItem;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::memory::MemoryAccessPolicy;

pub struct CreateMemoryRecord<'a> {
    pub workspace_id: WorkspaceId,
    pub write_scope: &'a ScopedWrite,
    pub owner_user_id: Uuid,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub title: &'a str,
    pub content: &'a str,
    pub content_redacted: bool,
    pub visibility: &'a str,
    pub sensitivity: &'a str,
    pub provenance: &'a Value,
    pub ttl_expires_at: Option<DateTime<Utc>>,
    pub confidence: Option<f64>,
    pub state: &'a str,
}

pub struct UpdateMemoryRecord<'a> {
    pub title: Option<&'a str>,
    pub content: Option<&'a str>,
    pub content_redacted: Option<bool>,
    pub sensitivity: Option<&'a str>,
    pub provenance: Option<&'a Value>,
    pub visibility: Option<&'a str>,
    pub confidence: Option<f64>,
    pub last_verified_at: Option<DateTime<Utc>>,
}

pub struct MemoryRepository {
    pool: PgPool,
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list_visible(&self, proof: &ScopedRead, limit: i64, offset: i64) -> AppResult<Vec<MemoryItem>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        let items = sqlx::query_as::<_, MemoryItem>(VISIBLE_MEMORY_QUERY)
            .bind(proof.org_id().as_uuid())
            .bind(workspace_ids(proof))
            .bind(proof.user_id().as_uuid())
            .bind(team_ids(proof))
            .bind(project_ids(proof))
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        Ok(items)
    }

    pub async fn get_visible_by_id(&self, proof: &ScopedRead, id: MemoryItemId) -> AppResult<MemoryItem> {
        if proof.workspace_ids().is_empty() {
            return Err(MemoryAccessPolicy::not_found(id));
        }

        sqlx::query_as::<_, MemoryItem>(VISIBLE_MEMORY_BY_ID_QUERY)
            .bind(id.as_uuid())
            .bind(proof.org_id().as_uuid())
            .bind(workspace_ids(proof))
            .bind(proof.user_id().as_uuid())
            .bind(team_ids(proof))
            .bind(project_ids(proof))
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| MemoryAccessPolicy::not_found(id))
    }

    pub async fn lock_visible_for_update(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        id: MemoryItemId,
    ) -> AppResult<MemoryItem> {
        if proof.workspace_ids().is_empty() {
            return Err(MemoryAccessPolicy::not_found(id));
        }

        sqlx::query_as::<_, MemoryItem>(VISIBLE_MEMORY_BY_ID_FOR_UPDATE_QUERY)
            .bind(id.as_uuid())
            .bind(proof.org_id().as_uuid())
            .bind(workspace_ids(proof))
            .bind(proof.user_id().as_uuid())
            .bind(team_ids(proof))
            .bind(project_ids(proof))
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| MemoryAccessPolicy::not_found(id))
    }

    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        proof: &ScopedRead,
        record: CreateMemoryRecord<'_>,
    ) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"INSERT INTO memory_items (
                   organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
                   source_task_id, source_run_id, title, content, content_redacted,
                   visibility, sensitivity, provenance, ttl_expires_at, confidence, state
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
               RETURNING *"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.owner_user_id)
        .bind(record.write_scope.kind().as_label())
        .bind(record.write_scope.id())
        .bind(record.source_task_id)
        .bind(record.source_run_id)
        .bind(record.title)
        .bind(record.content)
        .bind(record.content_redacted)
        .bind(record.visibility)
        .bind(record.sensitivity)
        .bind(record.provenance)
        .bind(record.ttl_expires_at)
        .bind(record.confidence)
        .bind(record.state)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn update_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: MemoryItemId,
        record: UpdateMemoryRecord<'_>,
    ) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
               SET title = COALESCE($2, title),
                   content = COALESCE($3, content),
                   content_redacted = COALESCE($4, content_redacted),
                   sensitivity = COALESCE($5, sensitivity),
                   provenance = COALESCE($6, provenance),
                   visibility = COALESCE($7, visibility),
                   confidence = COALESCE($8, confidence),
                   last_verified_at = COALESCE($9, last_verified_at)
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(record.title)
        .bind(record.content)
        .bind(record.content_redacted)
        .bind(record.sensitivity)
        .bind(record.provenance)
        .bind(record.visibility)
        .bind(record.confidence)
        .bind(record.last_verified_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn extend_ttl_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: MemoryItemId,
        ttl_expires_at: Option<DateTime<Utc>>,
    ) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
               SET ttl_expires_at = $2
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(ttl_expires_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_in_tx(tx: &mut Transaction<'_, Postgres>, id: MemoryItemId) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
               SET state = 'revoked', revoked_at = now()
               WHERE id = $1 AND revoked_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| MemoryAccessPolicy::already_revoked(id))
    }

    pub async fn reclassify_scope_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: MemoryItemId,
        write_scope: &ScopedWrite,
    ) -> AppResult<MemoryItem> {
        sqlx::query_as::<_, MemoryItem>(
            r#"UPDATE memory_items
               SET scope_kind = $2, scope_id = $3
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(write_scope.kind().as_label())
        .bind(write_scope.id())
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn resource_belongs_to_scope(
        &self,
        proof: &ScopedRead,
        kind: ScopeKind,
        scope_id: Uuid,
        workspace_id: WorkspaceId,
    ) -> AppResult<bool> {
        let exists = match kind {
            ScopeKind::User => scope_id == proof.user_id().as_uuid(),
            ScopeKind::Team => {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                           SELECT 1
                             FROM teams t
                            WHERE t.id = $3
                              AND t.organization_id = $1
                              AND t.deleted_at IS NULL
                              AND EXISTS (
                                  SELECT 1 FROM team_members tm
                                   WHERE tm.team_id = t.id AND tm.user_id = $2
                              )
                       )"#,
                )
                .bind(proof.org_id().as_uuid())
                .bind(proof.user_id().as_uuid())
                .bind(scope_id)
                .fetch_one(&self.pool)
                .await?
            }
            ScopeKind::Project => {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                           SELECT 1
                             FROM projects p
                            WHERE p.id = $3
                              AND p.organization_id = $1
                              AND p.workspace_id = $4
                              AND p.deleted_at IS NULL
                              AND (
                                  EXISTS (
                                      SELECT 1 FROM project_members pm
                                       WHERE pm.project_id = p.id AND pm.user_id = $2
                                  )
                                  OR EXISTS (
                                      SELECT 1 FROM team_members tm
                                       WHERE tm.team_id = p.team_id AND tm.user_id = $2
                                  )
                              )
                       )"#,
                )
                .bind(proof.org_id().as_uuid())
                .bind(proof.user_id().as_uuid())
                .bind(scope_id)
                .bind(workspace_id.as_uuid())
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(exists)
    }
}

const VISIBLE_MEMORY_BY_ID_QUERY: &str = r#"SELECT * FROM memory_items
WHERE id = $1
  AND organization_id = $2
  AND workspace_id = ANY($3)
  AND revoked_at IS NULL
  AND state = 'active'
  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
  AND (
      (scope_kind = 'user' AND scope_id = $4)
      OR (scope_kind = 'team' AND scope_id = ANY($5))
      OR (scope_kind = 'project' AND scope_id = ANY($6))
  )"#;

const VISIBLE_MEMORY_BY_ID_FOR_UPDATE_QUERY: &str = r#"SELECT * FROM memory_items
WHERE id = $1
  AND organization_id = $2
  AND workspace_id = ANY($3)
  AND revoked_at IS NULL
  AND state = 'active'
  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
  AND (
      (scope_kind = 'user' AND scope_id = $4)
      OR (scope_kind = 'team' AND scope_id = ANY($5))
      OR (scope_kind = 'project' AND scope_id = ANY($6))
  )
FOR UPDATE"#;

const VISIBLE_MEMORY_QUERY: &str = r#"SELECT * FROM memory_items
WHERE organization_id = $1
  AND workspace_id = ANY($2)
  AND revoked_at IS NULL
  AND state = 'active'
  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
  AND (
      (scope_kind = 'user' AND scope_id = $3)
      OR (scope_kind = 'team' AND scope_id = ANY($4))
      OR (scope_kind = 'project' AND scope_id = ANY($5))
  )
ORDER BY updated_at DESC, id DESC
LIMIT $6 OFFSET $7"#;

fn workspace_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.workspace_ids().iter().map(|id| id.as_uuid()).collect()
}

fn team_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.team_ids().iter().map(|id| id.as_uuid()).collect()
}

fn project_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.project_ids().iter().map(|id| id.as_uuid()).collect()
}
