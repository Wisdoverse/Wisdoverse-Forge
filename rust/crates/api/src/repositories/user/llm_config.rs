//! User LLM config repository — reads the legacy `user_llm_configs` table
//! populated by the TS `UserLlmConfigService` (`server/src/modules/llm-provider`).
//!
//! Scope: per-user (no `organization_id` column), filtered by `scope.user_id()`.
//! Phase 1 only needs "resolve the default enabled key for a provider" — the
//! full CRUD surface can migrate with the LLM gateway.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub struct UserLlmConfigSecret {
    pub encrypted_api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct LlmProviderConfigRow {
    pub id: Uuid,
    pub provider: String,
    pub model: Option<String>,
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub api_key_prefix: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_default: Option<bool>,
    pub last_test_status: Option<String>,
    pub last_test_error_code: Option<String>,
    pub last_test_error_message: Option<String>,
    pub last_tested_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct LlmProviderTestRow {
    pub id: Uuid,
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub encrypted_api_key: String,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct InsertLlmProviderConfig {
    pub provider: String,
    pub model: String,
    pub display_name: String,
    pub base_url: Option<String>,
    pub api_key_prefix: Option<String>,
    pub encrypted_api_key: String,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateLlmProviderConfig {
    pub model: String,
    pub display_name: String,
    pub base_url: Option<String>,
    pub is_enabled: bool,
    pub encrypted_api_key: Option<String>,
    pub api_key_prefix: Option<String>,
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

    pub async fn list_configs(&self, scope: &TenantScope) -> AppResult<Vec<LlmProviderConfigRow>> {
        sqlx::query_as::<_, LlmProviderConfigRow>(
            r#"SELECT id,
                      provider,
                      model,
                      display_name,
                      base_url,
                      api_key_prefix,
                      is_enabled,
                      is_default,
                      settings -> 'connection_test' ->> 'status' AS last_test_status,
                      settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                      settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                      settings -> 'connection_test' ->> 'tested_at' AS last_tested_at
                 FROM user_llm_configs
                WHERE user_id = $1
                ORDER BY COALESCE(is_default, false) DESC,
                         updated_at DESC NULLS LAST,
                         created_at DESC NULLS LAST"#,
        )
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_config(&self, scope: &TenantScope, id: Uuid) -> AppResult<LlmProviderConfigRow> {
        sqlx::query_as::<_, LlmProviderConfigRow>(
            r#"SELECT id,
                      provider,
                      model,
                      display_name,
                      base_url,
                      api_key_prefix,
                      is_enabled,
                      is_default,
                      settings -> 'connection_test' ->> 'status' AS last_test_status,
                      settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                      settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                      settings -> 'connection_test' ->> 'tested_at' AS last_tested_at
                 FROM user_llm_configs
                WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("llm provider {id}")).into())
    }

    pub async fn get_test_config(&self, scope: &TenantScope, id: Uuid) -> AppResult<LlmProviderTestRow> {
        sqlx::query_as::<_, LlmProviderTestRow>(
            r#"SELECT id, provider, model, base_url, encrypted_api_key, is_enabled
                 FROM user_llm_configs
                WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("llm provider {id}")).into())
    }

    pub async fn provider_model_exists(&self, scope: &TenantScope, provider: &str, model: &str) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM user_llm_configs
                    WHERE user_id = $1
                      AND provider = $2
                      AND model = $3
               )"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(provider)
        .bind(model)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn should_insert_as_default(&self, scope: &TenantScope, provider: &str) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT NOT EXISTS (
                   SELECT 1
                     FROM user_llm_configs
                    WHERE user_id = $1
                      AND provider = $2
                      AND COALESCE(is_default, false) = true
               )"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(provider)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn insert_config(
        &self,
        scope: &TenantScope,
        draft: InsertLlmProviderConfig,
    ) -> AppResult<LlmProviderConfigRow> {
        sqlx::query_as::<_, LlmProviderConfigRow>(
            r#"INSERT INTO user_llm_configs
                  (user_id, provider, model, display_name, base_url, api_key_prefix, encrypted_api_key, is_enabled, is_default, settings)
               VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, '{}'::jsonb)
               RETURNING id,
                         provider,
                         model,
                         display_name,
                         base_url,
                         api_key_prefix,
                         is_enabled,
                         is_default,
                         settings -> 'connection_test' ->> 'status' AS last_test_status,
                         settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                         settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                         settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(draft.provider)
        .bind(draft.model)
        .bind(draft.display_name)
        .bind(draft.base_url)
        .bind(draft.api_key_prefix)
        .bind(draft.encrypted_api_key)
        .bind(draft.is_default)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn update_config(
        &self,
        scope: &TenantScope,
        id: Uuid,
        draft: UpdateLlmProviderConfig,
    ) -> AppResult<LlmProviderConfigRow> {
        if let Some(encrypted_api_key) = draft.encrypted_api_key {
            sqlx::query_as::<_, LlmProviderConfigRow>(
                r#"UPDATE user_llm_configs
                      SET model = $1,
                          display_name = $2,
                          base_url = $3,
                          is_enabled = $4,
                          encrypted_api_key = $5,
                          api_key_prefix = $6,
                          settings = COALESCE(settings, '{}'::jsonb) - 'connection_test',
                          updated_at = now()
                    WHERE id = $7 AND user_id = $8
                RETURNING id,
                          provider,
                          model,
                          display_name,
                          base_url,
                          api_key_prefix,
                          is_enabled,
                          is_default,
                          settings -> 'connection_test' ->> 'status' AS last_test_status,
                          settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                          settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                          settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
            )
            .bind(draft.model)
            .bind(draft.display_name)
            .bind(draft.base_url)
            .bind(draft.is_enabled)
            .bind(encrypted_api_key)
            .bind(draft.api_key_prefix)
            .bind(id)
            .bind(scope.user_id().as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
        } else {
            sqlx::query_as::<_, LlmProviderConfigRow>(
                r#"UPDATE user_llm_configs
                      SET model = $1,
                          display_name = $2,
                          base_url = $3,
                          is_enabled = $4,
                          settings = COALESCE(settings, '{}'::jsonb) - 'connection_test',
                          updated_at = now()
                    WHERE id = $5 AND user_id = $6
                RETURNING id,
                          provider,
                          model,
                          display_name,
                          base_url,
                          api_key_prefix,
                          is_enabled,
                          is_default,
                          settings -> 'connection_test' ->> 'status' AS last_test_status,
                          settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                          settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                          settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
            )
            .bind(draft.model)
            .bind(draft.display_name)
            .bind(draft.base_url)
            .bind(draft.is_enabled)
            .bind(id)
            .bind(scope.user_id().as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
        }
    }

    pub async fn delete_config(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM user_llm_configs WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(scope.user_id().as_uuid())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("llm provider {id}")).into());
        }
        Ok(())
    }

    pub async fn set_default_config(
        &self,
        scope: &TenantScope,
        id: Uuid,
        provider: &str,
    ) -> AppResult<LlmProviderConfigRow> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"UPDATE user_llm_configs
                  SET is_default = false, updated_at = now()
                WHERE user_id = $1 AND provider = $2"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(provider)
        .execute(&mut *tx)
        .await?;

        let row = sqlx::query_as::<_, LlmProviderConfigRow>(
            r#"UPDATE user_llm_configs
                  SET is_default = true, updated_at = now()
                WHERE id = $1 AND user_id = $2
            RETURNING id,
                      provider,
                      model,
                      display_name,
                      base_url,
                      api_key_prefix,
                      is_enabled,
                      is_default,
                      settings -> 'connection_test' ->> 'status' AS last_test_status,
                      settings -> 'connection_test' ->> 'error_code' AS last_test_error_code,
                      settings -> 'connection_test' ->> 'error_message' AS last_test_error_message,
                      settings -> 'connection_test' ->> 'tested_at' AS last_tested_at"#,
        )
        .bind(id)
        .bind(scope.user_id().as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn record_test_result(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE user_llm_configs
                  SET settings = jsonb_set(
                        COALESCE(settings, '{}'::jsonb),
                        '{connection_test}',
                        jsonb_build_object(
                          'status', $3::text,
                          'tested_at', to_jsonb(now()),
                          'error_code', $4::text,
                          'error_message', $5::text
                        ),
                        true
                      ),
                      updated_at = now()
                WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(scope.user_id().as_uuid())
        .bind(status)
        .bind(error_code)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
