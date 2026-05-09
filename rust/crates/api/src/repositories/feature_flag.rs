//! Feature flag repository — database queries for the feature_flags table.

use agentforge_core::{AppResult, ErrorKind, OrgId};
use agentforge_db::entities::FeatureFlag;
use sqlx::PgPool;

/// Database access layer for feature flags.
pub struct FeatureFlagRepository {
    pool: PgPool,
}

impl FeatureFlagRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List feature flags for the given org (includes global flags where org is NULL).
    pub async fn list(&self, org_id: OrgId) -> AppResult<Vec<FeatureFlag>> {
        let flags = sqlx::query_as::<_, FeatureFlag>(
            r#"SELECT * FROM feature_flags
               WHERE organization_id = $1 OR organization_id IS NULL
               ORDER BY name ASC"#,
        )
        .bind(org_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(flags)
    }

    /// Get a specific flag by name for the given org (or global).
    pub async fn find_by_name(&self, org_id: OrgId, name: &str) -> AppResult<FeatureFlag> {
        sqlx::query_as::<_, FeatureFlag>(
            r#"SELECT * FROM feature_flags
               WHERE (organization_id = $1 OR organization_id IS NULL) AND name = $2
               ORDER BY organization_id IS NULL ASC
               LIMIT 1"#,
        )
        .bind(org_id.as_uuid())
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("feature flag '{name}'")).into())
    }

    /// Upsert a feature flag by name (org-scoped).
    pub async fn upsert(
        &self,
        org_id: OrgId,
        name: &str,
        enabled: bool,
        metadata: &serde_json::Value,
    ) -> AppResult<FeatureFlag> {
        sqlx::query_as::<_, FeatureFlag>(
            r#"INSERT INTO feature_flags (organization_id, name, enabled, metadata)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (organization_id, name)
               DO UPDATE SET enabled = EXCLUDED.enabled, metadata = EXCLUDED.metadata, updated_at = now()
               RETURNING *"#,
        )
        .bind(org_id.as_uuid())
        .bind(name)
        .bind(enabled)
        .bind(metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
