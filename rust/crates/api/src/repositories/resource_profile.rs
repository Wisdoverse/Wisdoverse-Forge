//! Resource profile repository — database queries for the resource_profiles table.

use agentforge_core::{AppError, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::ResourceProfile;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for resource profiles.
pub struct ResourceProfileRepository {
    pool: PgPool,
}

impl ResourceProfileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List resource profiles — org-specific + system defaults (NULL org).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<ResourceProfile>> {
        let profiles = sqlx::query_as::<_, ResourceProfile>(
            r#"SELECT * FROM resource_profiles
               WHERE organization_id = $1 OR organization_id IS NULL
               ORDER BY name ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(profiles)
    }

    /// Get a single resource profile by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<ResourceProfile> {
        sqlx::query_as::<_, ResourceProfile>(
            r#"SELECT * FROM resource_profiles
               WHERE id = $1 AND (organization_id = $2 OR organization_id IS NULL)"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("resource_profile {id}")).into())
    }

    /// Create a custom resource profile for the org.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        cpu_millicores: i32,
        memory_mb: i32,
        storage_mb: i32,
        max_pids: i32,
    ) -> AppResult<ResourceProfile> {
        let profile = sqlx::query_as::<_, ResourceProfile>(
            r#"INSERT INTO resource_profiles (organization_id, name, cpu_millicores, memory_mb, storage_mb, max_pids)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(cpu_millicores)
        .bind(memory_mb)
        .bind(storage_mb)
        .bind(max_pids)
        .fetch_one(&self.pool)
        .await?;
        Ok(profile)
    }

    /// Update a resource profile (only org-owned, not system defaults).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        name: Option<&str>,
        cpu_millicores: Option<i32>,
        memory_mb: Option<i32>,
        storage_mb: Option<i32>,
        max_pids: Option<i32>,
    ) -> AppResult<ResourceProfile> {
        let profile = sqlx::query_as::<_, ResourceProfile>(
            r#"UPDATE resource_profiles SET
                name = COALESCE($3, name),
                cpu_millicores = COALESCE($4, cpu_millicores),
                memory_mb = COALESCE($5, memory_mb),
                storage_mb = COALESCE($6, storage_mb),
                max_pids = COALESCE($7, max_pids)
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(cpu_millicores)
        .bind(memory_mb)
        .bind(storage_mb)
        .bind(max_pids)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| -> AppError { ErrorKind::NotFound(format!("resource_profile {id}")).into() })?;
        Ok(profile)
    }

    /// Delete a resource profile (only org-owned, not system defaults).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM resource_profiles
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("resource_profile {id}")).into());
        }
        Ok(())
    }
}
