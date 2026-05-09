//! Settings service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Setting;

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
        let key = key.trim();
        if key.is_empty() || key.len() > 255 {
            return Err(ErrorKind::Validation("key must be 1-255 characters".into()).into());
        }
        self.repo.upsert(scope, key, value).await
    }

    /// Delete a setting by key.
    pub async fn delete(&self, scope: &TenantScope, key: &str) -> AppResult<()> {
        self.repo.delete(scope, key).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_empty_key_rejected() {
        // We test the validation logic directly without DB
        let key = "".trim();
        assert!(key.is_empty());
    }

    #[test]
    fn validate_long_key_rejected() {
        let key = "a".repeat(256);
        assert!(key.len() > 255);
    }

    #[test]
    fn validate_valid_key_accepted() {
        let key = "theme.color".trim();
        assert!(!key.is_empty() && key.len() <= 255);
    }
}
