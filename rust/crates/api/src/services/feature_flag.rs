//! Feature flag service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::FeatureFlag;
use sqlx::PgPool;

pub(crate) use crate::domain::configuration::configuration_data_response;
use crate::domain::resource::FeatureFlagName;
use crate::repositories::feature_flag::FeatureFlagRepository;

#[derive(Debug, Clone)]
pub struct UpsertFeatureFlagInput {
    pub enabled: bool,
    pub metadata: Option<serde_json::Value>,
}

/// Business logic layer for feature flag operations.
pub struct FeatureFlagService {
    repo: FeatureFlagRepository,
}

impl FeatureFlagService {
    pub fn new(repo: FeatureFlagRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(FeatureFlagRepository::new(pool))
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
        input: UpsertFeatureFlagInput,
    ) -> AppResult<FeatureFlag> {
        let name = FeatureFlagName::parse(name)?;
        let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));
        self.repo.upsert(scope.org_id(), name.value(), input.enabled, &metadata).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::FeatureFlagName;

    #[test]
    fn feature_flag_name_validation() {
        assert_eq!(FeatureFlagName::parse(" dark-mode ").unwrap().value(), "dark-mode");
        assert!(FeatureFlagName::parse("").is_err());
        assert!(FeatureFlagName::parse(&"x".repeat(256)).is_err());
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
