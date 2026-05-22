//! LLM provider configuration service.
//!
//! Owns user-provider orchestration over the `user_llm_configs` repository:
//! validation, secret encryption/decryption, default selection, and connection
//! test result persistence.

use std::sync::Arc;
use std::time::Duration;

use agentforge_core::{AppResult, ErrorKind, TenantScope, crypto};
use agentforge_llm::{ChatMessage, ChatRequest, LlmProviderBuildConfig, LlmProviderFactory};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::credential::{
    LlmProviderConfigResponse, LlmProviderPolicy, llm_provider_delete_response, llm_provider_list_response,
    llm_provider_response, llm_provider_test_disabled_response, llm_provider_test_error_parts,
    llm_provider_test_error_payload, llm_provider_test_success_response, llm_provider_test_timeout_response,
    supported_providers_response,
};
use crate::repositories::user::llm_config::{
    InsertLlmProviderConfig, LlmProviderConfigRow, UpdateLlmProviderConfig, UserLlmConfigRepository,
};

pub(crate) struct LlmProviderService {
    repo: UserLlmConfigRepository,
    encryption_key: Option<[u8; 32]>,
    llm_factory: Arc<LlmProviderFactory>,
}

impl LlmProviderService {
    pub(crate) fn from_pool(
        pool: PgPool,
        encryption_key: Option<[u8; 32]>,
        llm_factory: Arc<LlmProviderFactory>,
    ) -> Self {
        Self::new(UserLlmConfigRepository::new(pool), encryption_key, llm_factory)
    }

    pub(crate) fn new(
        repo: UserLlmConfigRepository,
        encryption_key: Option<[u8; 32]>,
        llm_factory: Arc<LlmProviderFactory>,
    ) -> Self {
        Self { repo, encryption_key, llm_factory }
    }

    pub(crate) fn supported_providers(&self) -> Value {
        supported_providers_response()
    }

    pub(crate) async fn list_providers(&self, scope: &TenantScope) -> AppResult<Value> {
        let providers: Vec<_> = self
            .repo
            .list_configs(scope)
            .await?
            .into_iter()
            .enumerate()
            .map(|(idx, row)| provider_response_from_row(row, i32::try_from(idx + 1).unwrap_or(i32::MAX)))
            .collect();
        Ok(llm_provider_list_response(&providers))
    }

    pub(crate) async fn get_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<Value> {
        let row = self.repo.get_config(scope, id).await?;
        let provider = provider_response_from_row(row, 1);
        Ok(llm_provider_response(&provider))
    }

    pub(crate) async fn create_provider(
        &self,
        scope: &TenantScope,
        provider: String,
        model: String,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> AppResult<Value> {
        let draft = LlmProviderPolicy::create_draft(provider, model, display_name, api_key, base_url)?;
        let (encrypted_api_key, api_key_prefix) = self.encrypted_api_key_and_prefix(draft.api_key.as_deref())?;

        if self.repo.provider_model_exists(scope, &draft.provider, &draft.model).await? {
            return Err(LlmProviderPolicy::provider_model_conflict().into());
        }

        let is_default = self.repo.should_insert_as_default(scope, &draft.provider).await?;
        let row = self
            .repo
            .insert_config(
                scope,
                InsertLlmProviderConfig {
                    provider: draft.provider,
                    model: draft.model,
                    display_name: draft.display_name,
                    base_url: draft.base_url,
                    api_key_prefix,
                    encrypted_api_key,
                    is_default,
                },
            )
            .await?;
        let provider = provider_response_from_row(row, 1);
        Ok(llm_provider_response(&provider))
    }

    pub(crate) async fn update_provider(
        &self,
        scope: &TenantScope,
        id: Uuid,
        model: Option<String>,
        display_name: Option<String>,
        api_key: Option<String>,
        base_url: Option<String>,
        is_enabled: Option<bool>,
    ) -> AppResult<Value> {
        let current = self.repo.get_config(scope, id).await?;
        let draft = LlmProviderPolicy::update_draft(
            &current.provider,
            current.model,
            current.display_name,
            current.base_url,
            current.is_enabled,
            model,
            display_name,
            api_key,
            base_url,
            is_enabled,
        )?;
        let encrypted =
            if let Some(api_key) = draft.api_key.as_deref() { Some(self.encrypt_api_key(api_key)?) } else { None };
        let row = self
            .repo
            .update_config(
                scope,
                id,
                UpdateLlmProviderConfig {
                    model: draft.model,
                    display_name: draft.display_name,
                    base_url: draft.base_url,
                    is_enabled: draft.is_enabled,
                    encrypted_api_key: encrypted.as_ref().map(|(encrypted, _)| encrypted.clone()),
                    api_key_prefix: encrypted.map(|(_, prefix)| prefix),
                },
            )
            .await?;
        let provider = provider_response_from_row(row, 1);
        Ok(llm_provider_response(&provider))
    }

    pub(crate) async fn delete_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<Value> {
        self.repo.delete_config(scope, id).await?;
        Ok(llm_provider_delete_response())
    }

    pub(crate) async fn set_default_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<Value> {
        let current = self.repo.get_config(scope, id).await?;
        let row = self.repo.set_default_config(scope, id, &current.provider).await?;
        let provider = provider_response_from_row(row, 1);
        Ok(llm_provider_response(&provider))
    }

    pub(crate) async fn test_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<Value> {
        let provider = self.repo.get_test_config(scope, id).await?;
        if !provider.is_enabled.unwrap_or(true) {
            self.repo.record_test_result(scope, id, "failed", Some("disabled"), Some("Provider is disabled.")).await?;
            return Ok(llm_provider_test_disabled_response());
        }

        let model = LlmProviderPolicy::required_test_model(provider.model.as_deref())?;

        let api_key = if provider.provider == "ollama" {
            String::new()
        } else {
            let key = self.encryption_key.ok_or_else(LlmProviderPolicy::missing_test_api_key)?;
            crypto::decrypt_base64(&key, &provider.encrypted_api_key)
                .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("decrypt llm provider api key failed: {err}")))?
        };

        let provider_instance = match self.llm_factory.build_with_config(LlmProviderBuildConfig {
            provider_key: provider.provider.clone(),
            api_key,
            base_url: provider.base_url.clone(),
        }) {
            Ok(instance) => instance,
            Err(error) => {
                let (code, message, _) = llm_provider_test_error_parts(&error);
                self.repo.record_test_result(scope, id, "failed", Some(code), Some(message)).await?;
                return Ok(llm_provider_test_error_payload(&error));
            }
        };

        let request = ChatRequest {
            model: model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Reply with a short connection check acknowledgement.".to_string(),
            }],
            max_tokens: Some(16),
            temperature: Some(0.0),
        };

        match tokio::time::timeout(Duration::from_secs(30), provider_instance.chat(request)).await {
            Ok(Ok(response)) => {
                self.repo.record_test_result(scope, id, "passed", None, None).await?;
                Ok(llm_provider_test_success_response(
                    provider.id,
                    &provider.provider,
                    &model,
                    &response.content,
                    response.usage.as_ref(),
                ))
            }
            Ok(Err(error)) => {
                let (code, message, _) = llm_provider_test_error_parts(&error);
                self.repo.record_test_result(scope, id, "failed", Some(code), Some(message)).await?;
                Ok(llm_provider_test_error_payload(&error))
            }
            Err(_) => {
                self.repo
                    .record_test_result(
                        scope,
                        id,
                        "failed",
                        Some("timeout"),
                        Some("Provider connection test timed out."),
                    )
                    .await?;
                Ok(llm_provider_test_timeout_response())
            }
        }
    }

    fn encrypted_api_key_and_prefix(&self, api_key: Option<&str>) -> AppResult<(String, Option<String>)> {
        if let Some(api_key) = api_key {
            let (encrypted_api_key, prefix) = self.encrypt_api_key(api_key)?;
            Ok((encrypted_api_key, Some(prefix)))
        } else {
            Ok((String::new(), None))
        }
    }

    fn encrypt_api_key(&self, api_key: &str) -> AppResult<(String, String)> {
        let key = self.encryption_key.ok_or_else(LlmProviderPolicy::missing_storage_key)?;
        let encrypted_api_key = crypto::encrypt_base64(&key, api_key)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encrypt llm provider api key failed: {err}")))?;
        Ok((encrypted_api_key, LlmProviderPolicy::api_key_prefix(api_key)))
    }
}

fn provider_response_from_row(row: LlmProviderConfigRow, priority: i32) -> LlmProviderConfigResponse {
    let provider = row.provider;
    let display_name = row.display_name.unwrap_or_else(|| LlmProviderPolicy::display_name(&provider).to_string());

    LlmProviderConfigResponse {
        id: row.id,
        provider,
        display_name,
        model: row.model.unwrap_or_default(),
        base_url: row.base_url,
        api_key_prefix: row.api_key_prefix,
        priority,
        is_enabled: row.is_enabled.unwrap_or(true),
        is_default: row.is_default.unwrap_or(false),
        last_test_status: row.last_test_status,
        last_test_error_code: row.last_test_error_code,
        last_test_error_message: row.last_test_error_message,
        last_tested_at: row.last_tested_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_response_from_row_defaults_display_name_and_flags() {
        let id = Uuid::new_v4();
        let response = provider_response_from_row(
            LlmProviderConfigRow {
                id,
                provider: "openai".to_string(),
                model: Some("gpt-5.5".to_string()),
                display_name: None,
                base_url: None,
                api_key_prefix: Some("sk-12345".to_string()),
                is_enabled: None,
                is_default: None,
                last_test_status: Some("passed".to_string()),
                last_test_error_code: None,
                last_test_error_message: None,
                last_tested_at: Some("2026-05-20T13:00:00Z".to_string()),
            },
            2,
        );

        assert_eq!(response.id, id);
        assert_eq!(response.display_name, "OpenAI");
        assert_eq!(response.model, "gpt-5.5");
        assert!(response.is_enabled);
        assert!(!response.is_default);
        assert_eq!(response.priority, 2);
    }
}
