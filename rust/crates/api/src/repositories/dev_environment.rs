//! Dev environment repository — database queries for the dev_environments table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::DevEnvironment;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::dev_environment::DevEnvironmentRepositoryPolicy;

/// Database access layer for dev environments.
pub struct DevEnvironmentRepository {
    pool: PgPool,
}

impl DevEnvironmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List dev environments for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<DevEnvironment>> {
        let envs = sqlx::query_as::<_, DevEnvironment>(
            r#"SELECT * FROM dev_environments
               WHERE organization_id = $1
               ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(envs)
    }

    /// Get a single dev environment by ID (tenant-scoped).
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment> {
        sqlx::query_as::<_, DevEnvironment>(
            r#"SELECT * FROM dev_environments
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DevEnvironmentRepositoryPolicy::dev_environment_not_found(id))
    }

    /// Create a new dev environment.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        project_id: Option<Uuid>,
        config: &serde_json::Value,
    ) -> AppResult<DevEnvironment> {
        let env = sqlx::query_as::<_, DevEnvironment>(
            r#"INSERT INTO dev_environments (organization_id, name, project_id, config, created_by)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(project_id)
        .bind(config)
        .bind(scope.user_id().as_uuid())
        .fetch_one(&self.pool)
        .await?;
        Ok(env)
    }

    /// Update dev environment status and persist the exact container reference.
    ///
    /// `container_id = None` intentionally clears the container association.
    /// Start/stop lifecycle code needs explicit clearing; preserving the old
    /// value would leave a stopped environment pointing at a removed container.
    pub async fn update_status(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        container_id: Option<&str>,
    ) -> AppResult<DevEnvironment> {
        let env = sqlx::query_as::<_, DevEnvironment>(
            r#"UPDATE dev_environments SET
                status = $3,
                container_id = $4
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(status)
        .bind(container_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DevEnvironmentRepositoryPolicy::dev_environment_not_found(id))?;
        Ok(env)
    }

    /// Delete a dev environment by ID (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM dev_environments
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DevEnvironmentRepositoryPolicy::dev_environment_not_found(id));
        }
        Ok(())
    }
}
