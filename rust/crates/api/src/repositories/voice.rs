//! Voice repository — database queries for the voice_providers table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::VoiceProvider;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::voice::VoiceRepositoryPolicy;

/// Database access layer for voice providers.
pub struct VoiceRepository {
    pool: PgPool,
}

impl VoiceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List voice providers for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<VoiceProvider>> {
        let providers = sqlx::query_as::<_, VoiceProvider>(
            r#"SELECT * FROM voice_providers
               WHERE organization_id = $1
               ORDER BY name ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(providers)
    }

    /// Create a new voice provider.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        provider_type: &str,
        config: &serde_json::Value,
    ) -> AppResult<VoiceProvider> {
        sqlx::query_as::<_, VoiceProvider>(
            r#"INSERT INTO voice_providers (organization_id, name, provider_type, config)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(provider_type)
        .bind(config)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update a voice provider.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        name: Option<&str>,
        provider_type: Option<&str>,
        config: Option<&serde_json::Value>,
    ) -> AppResult<VoiceProvider> {
        sqlx::query_as::<_, VoiceProvider>(
            r#"UPDATE voice_providers
               SET name = COALESCE($3, name),
                   provider_type = COALESCE($4, provider_type),
                   config = COALESCE($5, config),
                   updated_at = now()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(provider_type)
        .bind(config)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| VoiceRepositoryPolicy::provider_not_found(id))
    }

    /// Delete a voice provider.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM voice_providers
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(VoiceRepositoryPolicy::provider_not_found(id));
        }
        Ok(())
    }

    /// Set a provider as the default (unset others first).
    pub async fn set_default(&self, scope: &TenantScope, id: Uuid) -> AppResult<VoiceProvider> {
        // Unset all defaults for this org
        sqlx::query(r#"UPDATE voice_providers SET is_default = false WHERE organization_id = $1"#)
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;

        // Set the specified one as default
        sqlx::query_as::<_, VoiceProvider>(
            r#"UPDATE voice_providers
               SET is_default = true, updated_at = now()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| VoiceRepositoryPolicy::provider_not_found(id))
    }
}
