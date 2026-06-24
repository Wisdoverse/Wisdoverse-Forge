//! SSH key repository — database queries for the ssh_keys table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::SshKey;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::credential::CredentialRepositoryPolicy;

/// Database access layer for SSH keys.
pub struct SshKeyRepository {
    pool: PgPool,
}

impl SshKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new SSH key (tenant-scoped).
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        public_key: &str,
        fingerprint: &str,
        key_type: &str,
    ) -> AppResult<SshKey> {
        sqlx::query_as::<_, SshKey>(
            r#"INSERT INTO ssh_keys (organization_id, user_id, name, public_key, fingerprint, key_type)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(name)
        .bind(public_key)
        .bind(fingerprint)
        .bind(key_type)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// List SSH keys for the tenant user (paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<SshKey>> {
        let keys = sqlx::query_as::<_, SshKey>(
            r#"SELECT * FROM ssh_keys
               WHERE organization_id = $1 AND user_id = $2
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(keys)
    }

    /// Find an SSH key by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<SshKey> {
        sqlx::query_as::<_, SshKey>(
            r#"SELECT * FROM ssh_keys
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| CredentialRepositoryPolicy::ssh_key_not_found(id))
    }

    /// Delete an SSH key (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM ssh_keys
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(CredentialRepositoryPolicy::ssh_key_not_found(id));
        }
        Ok(())
    }
}
