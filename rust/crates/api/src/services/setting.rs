//! Settings service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Setting;
use serde_json::Value;

use crate::domain::configuration::{GatewaySettings, RuntimeSettings};
pub(crate) use crate::domain::configuration::{gateway_settings_response, runtime_settings_response};
use crate::domain::resource::SettingKey;
use crate::repositories::setting::SettingRepository;

const RUNTIME_KEY: &str = "runtime";
const GATEWAY_KEY: &str = "gateway";

/// Business logic layer for settings operations.
pub struct SettingService {
    repo: SettingRepository,
}

impl SettingService {
    pub fn new(repo: SettingRepository) -> Self {
        Self { repo }
    }

    /// List all settings for the user/org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Setting>> {
        self.repo.list(scope).await
    }

    /// Upsert a setting by key.
    pub async fn upsert(&self, scope: &TenantScope, key: &str, value: &Value) -> AppResult<Setting> {
        let key = SettingKey::parse(key)?;
        self.repo.upsert(scope, key.value(), value).await
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
        default_runtime: Option<&str>,
        default_cli_tool: Option<&str>,
    ) -> AppResult<RuntimeSettings> {
        let mut runtime = self.runtime_settings(scope).await?;
        runtime.apply_update(default_runtime, default_cli_tool)?;

        let value = serde_json::to_value(&runtime).map_err(|err| ErrorKind::Internal(err.into()))?;
        self.upsert(scope, RUNTIME_KEY, &value).await?;
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
        routing_strategy: Option<&str>,
        circuit_breaker_threshold: Option<u32>,
        circuit_breaker_reset_ms: Option<u32>,
    ) -> AppResult<GatewaySettings> {
        let mut gateway = self.gateway_settings(scope).await?;
        gateway.apply_update(routing_strategy, circuit_breaker_threshold, circuit_breaker_reset_ms)?;

        let value = serde_json::to_value(&gateway).map_err(|err| ErrorKind::Internal(err.into()))?;
        self.upsert(scope, GATEWAY_KEY, &value).await?;
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
