//! Quota repository — database queries for the quota_usage table.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::QuotaUsage;
use sqlx::PgPool;

/// Database access layer for quota usage.
pub struct QuotaRepository {
    pool: PgPool,
}

impl QuotaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all quota usage records for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<QuotaUsage>> {
        let quotas = sqlx::query_as::<_, QuotaUsage>(
            r#"SELECT * FROM quota_usage
               WHERE organization_id = $1
               ORDER BY resource_type ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(quotas)
    }

    /// Get quota usage for a specific resource type.
    pub async fn get_by_type(&self, scope: &TenantScope, resource_type: &str) -> AppResult<QuotaUsage> {
        sqlx::query_as::<_, QuotaUsage>(
            r#"SELECT * FROM quota_usage
               WHERE organization_id = $1 AND resource_type = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(resource_type)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("quota for resource_type '{resource_type}'")).into())
    }
}
