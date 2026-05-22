//! Settings service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Setting;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::configuration::{
    GatewaySettings, RuntimeSettings, gateway_settings_persistence_value, runtime_settings_persistence_value,
};
pub(crate) use crate::domain::configuration::{
    configuration_data_response, configuration_delete_response, gateway_settings_response, runtime_settings_response,
};
use crate::domain::resource::SettingKey;
use crate::repositories::setting::SettingRepository;

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
}

impl SettingService {
    pub fn new(repo: SettingRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(SettingRepository::new(pool))
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
}

fn setting_value<'a>(scope: &TenantScope, settings: &'a [Setting], key: &str) -> Option<&'a Value> {
    settings
        .iter()
        .find(|setting| setting.key == key && setting.user_id == Some(scope.user_id()))
        .or_else(|| settings.iter().find(|setting| setting.key == key))
        .map(|setting| &setting.value)
}

#[cfg(test)]
mod tests {
    use agentforge_core::{OrgId, SettingId, UserId};
    use agentforge_db::entities::Setting;
    use chrono::Utc;
    use uuid::Uuid;

    use super::{RUNTIME_KEY, setting_value};
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
}
