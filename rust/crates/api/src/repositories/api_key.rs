//! API key repository — database queries for the api_keys table.
//!
//! `find_by_hash` is NOT tenant-scoped because API key auth lookup happens
//! before org context is established. Other methods enforce tenant isolation
//! via `TenantScope`.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::ApiKey;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for API keys.
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new API key (tenant-scoped).
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        key_hash: &str,
        key_prefix: &str,
        scopes: &[String],
        expires_at: Option<DateTime<Utc>>,
    ) -> AppResult<ApiKey> {
        sqlx::query_as::<_, ApiKey>(
            r#"INSERT INTO api_keys (organization_id, user_id, name, key_hash, key_prefix, scopes, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(name)
        .bind(key_hash)
        .bind(key_prefix)
        .bind(scopes)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// List API keys for the tenant org (paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<ApiKey>> {
        let keys = sqlx::query_as::<_, ApiKey>(
            r#"SELECT * FROM api_keys
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

    /// Find an API key by its hash — NOT tenant-scoped (for API key auth lookup).
    pub async fn find_by_hash(&self, key_hash: &str) -> AppResult<Option<ApiKey>> {
        let key = sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE key_hash = $1")
            .bind(key_hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(key)
    }

    /// Revoke an API key by setting revoked_at (tenant-scoped).
    pub async fn revoke(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE api_keys SET revoked_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND user_id = $3 AND revoked_at IS NULL"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("api key {id}")).into());
        }
        Ok(())
    }

    /// Update last_used_at timestamp — NOT tenant-scoped (called during auth).
    pub async fn update_last_used(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(())
    }
}
