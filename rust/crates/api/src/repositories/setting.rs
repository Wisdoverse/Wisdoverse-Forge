//! Settings repository — database queries for the settings table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Setting;
use sqlx::PgPool;

use crate::domain::configuration::ConfigurationRepositoryPolicy;

/// Database access layer for settings.
pub struct SettingRepository {
    pool: PgPool,
}

impl SettingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List settings for the authenticated user within their org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Setting>> {
        let settings = sqlx::query_as::<_, Setting>(
            r#"SELECT * FROM settings
               WHERE organization_id = $1 AND (user_id = $2 OR user_id IS NULL)
               ORDER BY key ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(settings)
    }

    /// Upsert a setting by key (user-scoped).
    pub async fn upsert(&self, scope: &TenantScope, key: &str, value: &serde_json::Value) -> AppResult<Setting> {
        sqlx::query_as::<_, Setting>(
            r#"INSERT INTO settings (organization_id, user_id, key, value)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (organization_id, user_id, key)
               DO UPDATE SET value = EXCLUDED.value, updated_at = now()
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(key)
        .bind(value)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Delete a setting by key (user-scoped).
    pub async fn delete(&self, scope: &TenantScope, key: &str) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM settings
               WHERE organization_id = $1 AND user_id = $2 AND key = $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(key)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ConfigurationRepositoryPolicy::setting_not_found(key));
        }
        Ok(())
    }
}
