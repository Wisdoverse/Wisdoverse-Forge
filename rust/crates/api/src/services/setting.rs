//! Settings service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Setting;

use crate::domain::resource::SettingKey;
use crate::repositories::setting::SettingRepository;

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
    pub async fn upsert(&self, scope: &TenantScope, key: &str, value: &serde_json::Value) -> AppResult<Setting> {
        let key = SettingKey::parse(key)?;
        self.repo.upsert(scope, key.value(), value).await
    }

    /// Delete a setting by key.
    pub async fn delete(&self, scope: &TenantScope, key: &str) -> AppResult<()> {
        self.repo.delete(scope, key).await
    }
}

#[cfg(test)]
mod tests {
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
}
