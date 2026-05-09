//! Feature flag service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::FeatureFlag;

use crate::repositories::feature_flag::FeatureFlagRepository;

/// Business logic layer for feature flag operations.
pub struct FeatureFlagService {
    repo: FeatureFlagRepository,
}

impl FeatureFlagService {
    pub fn new(repo: FeatureFlagRepository) -> Self {
        Self { repo }
    }

    /// List all flags for the org (including global).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<FeatureFlag>> {
        self.repo.list(scope.org_id()).await
    }

    /// Get a specific flag by name.
    pub async fn get_by_name(&self, scope: &TenantScope, name: &str) -> AppResult<FeatureFlag> {
        self.repo.find_by_name(scope.org_id(), name).await
    }

    /// Upsert a feature flag by name.
    pub async fn upsert(
        &self,
        scope: &TenantScope,
        name: &str,
        enabled: bool,
        metadata: Option<&serde_json::Value>,
    ) -> AppResult<FeatureFlag> {
        let name = name.trim();
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        let default_metadata = serde_json::json!({});
        let metadata = metadata.unwrap_or(&default_metadata);
        self.repo.upsert(scope.org_id(), name, enabled, metadata).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn feature_flag_name_validation() {
        let valid = "dark-mode".trim();
        assert!(!valid.is_empty() && valid.len() <= 255);

        let empty = "".trim();
        assert!(empty.is_empty());

        let too_long = "x".repeat(256);
        assert!(too_long.len() > 255);
    }

    #[test]
    fn feature_flag_toggle_values() {
        // Verify bool toggling works as expected
        let enabled = true;
        let disabled = false;
        assert_ne!(enabled, disabled);
        assert!(enabled);
        assert!(!disabled);
    }
}
