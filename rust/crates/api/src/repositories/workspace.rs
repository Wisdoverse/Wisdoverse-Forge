//! Workspace repository — tenant-scoped database queries for workspaces.

use agentforge_core::{AppResult, TenantScope, WorkspaceId};
use agentforge_db::entities::Workspace;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::resource::ResourceRepositoryPolicy;

/// Database access layer for workspaces.
pub struct WorkspaceRepository {
    pool: PgPool,
}

impl WorkspaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List workspaces for the current tenant, ordered by most recent first.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Workspace>> {
        let workspaces = sqlx::query_as::<_, Workspace>(
            r#"SELECT * FROM workspaces
               WHERE organization_id = $1 AND deleted_at IS NULL
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(workspaces)
    }

    /// Get a single workspace by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<Workspace> {
        sqlx::query_as::<_, Workspace>(
            "SELECT * FROM workspaces WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::workspace_not_found(id))
    }

    /// Returns true when an active workspace with `workspace_id` exists in
    /// `org_id`. Used by session-context authorization before a context
    /// switch has been minted (no tenant scope yet).
    pub async fn exists_in_org(&self, workspace_id: Uuid, org_id: Uuid) -> AppResult<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM workspaces
                    WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Create a new workspace.
    pub async fn create(&self, scope: &TenantScope, name: &str) -> AppResult<Workspace> {
        sqlx::query_as::<_, Workspace>(
            r#"INSERT INTO workspaces (organization_id, name)
               VALUES ($1, $2)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update a workspace's name (tenant-scoped).
    pub async fn update(&self, scope: &TenantScope, id: WorkspaceId, name: &str) -> AppResult<Workspace> {
        sqlx::query_as::<_, Workspace>(
            r#"UPDATE workspaces SET name = $3, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::workspace_not_found(id))
    }

    /// Soft-delete a workspace (set deleted_at).
    pub async fn delete(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE workspaces SET deleted_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::workspace_not_found(id));
        }
        Ok(())
    }
}
