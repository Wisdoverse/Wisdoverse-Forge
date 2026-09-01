//! Settings service — validation and management.

use std::sync::Arc;

use agentforge_core::{AppResult, CliToolKind, TenantScope};
use agentforge_db::entities::Setting;
use agentforge_platform::DockerClient;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::configuration::{
    GatewaySettings, RuntimeCliToolDetail, RuntimeSettings, RuntimeSettingsWithCliTools,
    gateway_settings_persistence_value, runtime_settings_persistence_value,
};
pub(crate) use crate::domain::configuration::{
    configuration_data_response, configuration_delete_response, gateway_settings_response,
    runtime_settings_with_cli_tools_response,
};
use crate::domain::resource::SettingKey;
use crate::repositories::setting::SettingRepository;
use crate::services::container_image_config::{
    capture_container_image_identity, configured_cli_image, image_verification_failure,
};

const RUNTIME_KEY: &str = "runtime";
const GATEWAY_KEY: &str = "gateway";

#[derive(Debug, Clone)]
pub struct UpsertSettingInput {
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateRuntimeSettingsInput {
    pub default_runtime: Option<String>,
    pub default_cli_tool: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateGatewaySettingsInput {
    pub routing_strategy: Option<String>,
    pub circuit_breaker_threshold: Option<u32>,
    pub circuit_breaker_reset_ms: Option<u32>,
}

/// Business logic layer for settings operations.
pub struct SettingService {
    repo: SettingRepository,
    docker: Option<Arc<DockerClient>>,
}

impl SettingService {
    pub fn new(repo: SettingRepository) -> Self {
        Self { repo, docker: None }
    }

    pub fn new_with_runtime(repo: SettingRepository, docker: Option<Arc<DockerClient>>) -> Self {
        Self { repo, docker }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(SettingRepository::new(pool))
    }

    pub fn from_runtime(pool: PgPool, docker: Option<Arc<DockerClient>>) -> Self {
        Self::new_with_runtime(SettingRepository::new(pool), docker)
    }

    /// List all settings for the user/org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Setting>> {
        self.repo.list(scope).await
    }

    /// Upsert a setting by key.
    pub async fn upsert(&self, scope: &TenantScope, key: &str, input: UpsertSettingInput) -> AppResult<Setting> {
        let key = SettingKey::parse(key)?;
        self.repo.upsert(scope, key.value(), &input.value).await
    }

    /// Delete a setting by key.
    pub async fn delete(&self, scope: &TenantScope, key: &str) -> AppResult<()> {
        self.repo.delete(scope, key).await
    }

    /// Read runtime settings, preferring user-scoped values over organization defaults.
    pub(crate) async fn runtime_settings(&self, scope: &TenantScope) -> AppResult<RuntimeSettings> {
        let settings = self.list(scope).await?;
        Ok(RuntimeSettings::from_stored(setting_value(scope, &settings, RUNTIME_KEY)))
    }

    pub(crate) async fn runtime_settings_with_cli_tools(
        &self,
        scope: &TenantScope,
    ) -> AppResult<RuntimeSettingsWithCliTools> {
        let runtime = self.runtime_settings(scope).await?;
        let cli_tool_details = self.runtime_cli_tool_details(&runtime).await;
        Ok(RuntimeSettingsWithCliTools { runtime, cli_tool_details })
    }

    /// Validate and persist runtime settings.
    pub(crate) async fn update_runtime_settings(
        &self,
        scope: &TenantScope,
        input: UpdateRuntimeSettingsInput,
    ) -> AppResult<RuntimeSettings> {
        let mut runtime = self.runtime_settings(scope).await?;
        runtime.apply_update(input.default_runtime.as_deref(), input.default_cli_tool.as_deref())?;

        let value = runtime_settings_persistence_value(&runtime)?;
        self.upsert(scope, RUNTIME_KEY, UpsertSettingInput { value }).await?;
        Ok(runtime)
    }

    pub(crate) async fn update_runtime_settings_with_cli_tools(
        &self,
        scope: &TenantScope,
        input: UpdateRuntimeSettingsInput,
    ) -> AppResult<RuntimeSettingsWithCliTools> {
        let runtime = self.update_runtime_settings(scope, input).await?;
        let cli_tool_details = self.runtime_cli_tool_details(&runtime).await;
        Ok(RuntimeSettingsWithCliTools { runtime, cli_tool_details })
    }

    /// Read gateway settings, preferring user-scoped values over organization defaults.
    pub(crate) async fn gateway_settings(&self, scope: &TenantScope) -> AppResult<GatewaySettings> {
        let settings = self.list(scope).await?;
        Ok(GatewaySettings::from_stored(setting_value(scope, &settings, GATEWAY_KEY)))
    }

    /// Validate and persist gateway settings.
    pub(crate) async fn update_gateway_settings(
        &self,
        scope: &TenantScope,
        input: UpdateGatewaySettingsInput,
    ) -> AppResult<GatewaySettings> {
        let mut gateway = self.gateway_settings(scope).await?;
        gateway.apply_update(
            input.routing_strategy.as_deref(),
            input.circuit_breaker_threshold,
            input.circuit_breaker_reset_ms,
        )?;

        let value = gateway_settings_persistence_value(&gateway)?;
        self.upsert(scope, GATEWAY_KEY, UpsertSettingInput { value }).await?;
        Ok(gateway)
    }

    async fn runtime_cli_tool_details(&self, runtime: &RuntimeSettings) -> Vec<RuntimeCliToolDetail> {
        futures::future::join_all(runtime.available_cli_tools.iter().filter_map(|cli_tool| {
            let tool = CliToolKind::parse_legacy(cli_tool).ok()?;
            Some(self.inspect_cli_image(tool, cli_tool.clone(), configured_cli_image(tool)))
        }))
        .await
    }

    async fn inspect_cli_image(&self, tool: CliToolKind, cli_tool: String, image: String) -> RuntimeCliToolDetail {
        if let Some(docker) = &self.docker {
            match docker.local_image_identity(&image).await {
                Ok(Some(identity)) => {
                    let label_version = identity
                        .labels
                        .get("org.agentforge.cli-version")
                        .or_else(|| identity.labels.get("org.wisdoverse.cli-version"))
                        .cloned()
                        .and_then(clean_version);
                    let (version, version_source) = match label_version {
                        Some(version) => (Some(version), "docker-label".to_string()),
                        None => (image_tag_version(&image), "image-tag".to_string()),
                    };
                    return match capture_container_image_identity(tool, &image, &identity).await {
                        Ok(evidence) => RuntimeCliToolDetail {
                            cli_tool,
                            image,
                            image_id: Some(identity.id),
                            image_digest: identity.manifest_digest,
                            version,
                            image_present: true,
                            image_ready: true,
                            image_trust: Some(evidence.trust),
                            image_error: None,
                            image_error_code: None,
                            version_source,
                        },
                        Err(err) => {
                            let (code, recovery) = image_verification_failure(&err);
                            tracing::warn!(error = %err, image, "Container CLI image is present but not trusted");
                            RuntimeCliToolDetail {
                                cli_tool,
                                image,
                                image_id: Some(identity.id),
                                image_digest: identity.manifest_digest,
                                version,
                                image_present: true,
                                image_ready: false,
                                image_trust: None,
                                image_error: Some(recovery.to_string()),
                                image_error_code: Some(code.to_string()),
                                version_source,
                            }
                        }
                    };
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::debug!(error = %err, image, "failed to inspect Container CLI image");
                    return RuntimeCliToolDetail {
                        cli_tool,
                        image,
                        image_id: None,
                        image_digest: None,
                        version: None,
                        image_present: false,
                        image_ready: false,
                        image_trust: None,
                        image_error: Some("Could not inspect this image".to_string()),
                        image_error_code: Some("image_inspection_failed".to_string()),
                        version_source: "not-reported".to_string(),
                    };
                }
            }
        }

        let tag_version = image_tag_version(&image);
        let source = if tag_version.is_some() { "image-tag" } else { "not-reported" };
        RuntimeCliToolDetail {
            cli_tool,
            image,
            image_id: None,
            image_digest: None,
            version: tag_version,
            image_present: false,
            image_ready: false,
            image_trust: None,
            image_error: None,
            image_error_code: None,
            version_source: source.to_string(),
        }
    }
}

fn setting_value<'a>(scope: &TenantScope, settings: &'a [Setting], key: &str) -> Option<&'a Value> {
    settings
        .iter()
        .find(|setting| setting.key == key && setting.user_id == Some(scope.user_id()))
        .or_else(|| settings.iter().find(|setting| setting.key == key))
        .map(|setting| &setting.value)
}

fn image_tag_version(image: &str) -> Option<String> {
    let image_name = image.rsplit('/').next()?;
    let (_, tag) = image_name.rsplit_once(':')?;
    clean_version(tag.to_string())
}

fn clean_version(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "<no value>" { None } else { Some(value.to_string()) }
}

#[cfg(test)]
mod tests {
    use agentforge_core::{OrgId, SettingId, UserId};
    use agentforge_db::entities::Setting;
    use chrono::Utc;
    use uuid::Uuid;

    use super::{RUNTIME_KEY, image_tag_version, setting_value};
    use crate::domain::resource::SettingKey;

    #[test]
    fn validate_empty_key_rejected() {
        assert!(SettingKey::parse("").is_err());
    }

    #[test]
    fn validate_long_key_rejected() {
        assert!(SettingKey::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn validate_valid_key_accepted() {
        assert_eq!(SettingKey::parse(" theme.color ").unwrap().value(), "theme.color");
    }

    #[test]
    fn setting_value_prefers_user_setting_over_org_default() {
        let org_uuid = Uuid::new_v4();
        let user_uuid = Uuid::new_v4();
        let org_id = OrgId::from(org_uuid);
        let user_id = UserId::from(user_uuid);
        let scope = crate::test_support::tenant_scope_for_ids(org_uuid, user_uuid);
        let now = Utc::now();
        let settings = vec![
            Setting {
                id: SettingId::new(),
                organization_id: org_id,
                user_id: None,
                key: RUNTIME_KEY.to_string(),
                value: serde_json::json!({ "defaultRuntime": "container" }),
                created_at: now,
                updated_at: now,
            },
            Setting {
                id: SettingId::new(),
                organization_id: org_id,
                user_id: Some(user_id),
                key: RUNTIME_KEY.to_string(),
                value: serde_json::json!({ "defaultRuntime": "api" }),
                created_at: now,
                updated_at: now,
            },
        ];

        let value = setting_value(&scope, &settings, RUNTIME_KEY).unwrap();
        assert_eq!(value["defaultRuntime"], "api");
    }

    #[test]
    fn image_tag_version_ignores_registry_port() {
        assert_eq!(
            image_tag_version("registry.local:5000/team/agentforge-agent:codex-1.2.3").as_deref(),
            Some("codex-1.2.3")
        );
        assert!(image_tag_version("registry.local:5000/team/agentforge-agent").is_none());
    }
}
