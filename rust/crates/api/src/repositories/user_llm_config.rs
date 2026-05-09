//! User LLM config repository — reads the legacy `user_llm_configs` table
//! populated by the TS `UserLlmConfigService` (`server/src/modules/llm-provider`).
//!
//! Scope: per-user (no `organization_id` column), filtered by `scope.user_id()`.
//! Phase 1 only needs "resolve the default enabled key for a provider" — the
//! full CRUD surface can migrate with the LLM gateway.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgPool;

pub struct UserLlmConfigSecret {
    pub encrypted_api_key: String,
    pub base_url: Option<String>,
}

pub struct UserLlmConfigRepository {
    pool: PgPool,
}

impl UserLlmConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Base64 AES-GCM ciphertext of the user's default API key for the given
    /// provider. Prefers `is_default=true`; falls back to the most recently
    /// updated enabled row.
    pub async fn find_default_api_key(&self, scope: &TenantScope, provider: &str) -> AppResult<Option<String>> {
        Ok(self.find_default_secret(scope, provider).await?.map(|secret| secret.encrypted_api_key))
    }

    pub async fn find_default_secret(
        &self,
        scope: &TenantScope,
        provider: &str,
    ) -> AppResult<Option<UserLlmConfigSecret>> {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            r#"SELECT encrypted_api_key, base_url FROM user_llm_configs
               WHERE user_id = $1 AND provider = $2 AND is_enabled = TRUE
               ORDER BY is_default DESC, updated_at DESC
               LIMIT 1"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(provider)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(encrypted_api_key, base_url)| UserLlmConfigSecret { encrypted_api_key, base_url }))
    }
}
