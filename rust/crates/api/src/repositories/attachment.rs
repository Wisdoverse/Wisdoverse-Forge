//! Attachment repository — database queries for the attachments table.

use agentforge_core::{AgentId, AppResult, AttachmentId, ErrorKind, TenantScope};
use agentforge_db::entities::Attachment;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for attachments.
pub struct AttachmentRepository {
    pool: PgPool,
}

/// Insert payload for a new attachment metadata row.
pub struct NewAttachment<'a> {
    pub id: AttachmentId,
    pub agent_id: Option<AgentId>,
    pub filename: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
    pub storage_path: &'a str,
    pub storage_backend: &'a str,
}

impl AttachmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List attachments for the org, optionally filtered by agent_id.
    pub async fn list(&self, scope: &TenantScope, agent_id: Option<AgentId>) -> AppResult<Vec<Attachment>> {
        let attachments = match agent_id {
            Some(aid) => {
                sqlx::query_as::<_, Attachment>(
                    r#"SELECT * FROM attachments
                       WHERE organization_id = $1 AND agent_id = $2
                       ORDER BY created_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(aid.as_uuid())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Attachment>(
                    r#"SELECT * FROM attachments
                       WHERE organization_id = $1 AND user_id = $2
                       ORDER BY created_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(scope.user_id().as_uuid())
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(attachments)
    }

    /// Get a single attachment by ID (tenant-scoped).
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Attachment> {
        sqlx::query_as::<_, Attachment>(
            r#"SELECT * FROM attachments
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("attachment {id}")).into())
    }

    /// Create a new attachment metadata record.
    pub async fn create(&self, scope: &TenantScope, new: NewAttachment<'_>) -> AppResult<Attachment> {
        let att = sqlx::query_as::<_, Attachment>(
            r#"INSERT INTO attachments (id, organization_id, user_id, agent_id, filename, content_type, size_bytes, storage_path, storage_backend)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING *"#,
        )
        .bind(new.id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(new.agent_id.map(|a| a.as_uuid()))
        .bind(new.filename)
        .bind(new.content_type)
        .bind(new.size_bytes)
        .bind(new.storage_path)
        .bind(new.storage_backend)
        .fetch_one(&self.pool)
        .await?;
        Ok(att)
    }

    /// Count tenant-scoped attachments associated with one agent.
    pub async fn count_for_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*)
               FROM attachments
               WHERE organization_id = $1 AND agent_id = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Delete an attachment by ID (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM attachments
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("attachment {id}")).into());
        }
        Ok(())
    }
}
