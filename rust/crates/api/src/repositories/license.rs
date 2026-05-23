//! License repository — database queries for the licenses table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::License;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::license::LicenseRepositoryPolicy;

/// Database access layer for licenses.
pub struct LicenseRepository {
    pool: PgPool,
}

impl LicenseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List licenses for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<License>> {
        let licenses = sqlx::query_as::<_, License>(
            r#"SELECT * FROM licenses
               WHERE organization_id = $1
               ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(licenses)
    }

    /// Get a license by ID.
    pub async fn get_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<License> {
        sqlx::query_as::<_, License>(
            r#"SELECT * FROM licenses
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| LicenseRepositoryPolicy::license_not_found(id))
    }

    /// Find a license by key (cross-org for validation).
    pub async fn find_by_key(&self, license_key: &str) -> AppResult<Option<License>> {
        let license = sqlx::query_as::<_, License>(r#"SELECT * FROM licenses WHERE license_key = $1"#)
            .bind(license_key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(license)
    }

    /// Activate a license by setting is_active = true for the org.
    pub async fn activate(&self, scope: &TenantScope, license_key: &str) -> AppResult<License> {
        sqlx::query_as::<_, License>(
            r#"UPDATE licenses
               SET is_active = true, updated_at = now()
               WHERE license_key = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(license_key)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| LicenseRepositoryPolicy::license_key_not_found(license_key))
    }
}
