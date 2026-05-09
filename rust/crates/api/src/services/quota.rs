//! Quota service — usage tracking and limits.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::QuotaUsage;

use crate::repositories::quota::QuotaRepository;

/// Valid resource types for quota tracking.
const VALID_RESOURCE_TYPES: &[&str] = &["agents", "storage", "events"];

/// Business logic layer for quota operations.
pub struct QuotaService {
    repo: QuotaRepository,
}

impl QuotaService {
    pub fn new(repo: QuotaRepository) -> Self {
        Self { repo }
    }

    /// Get all quota usage for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<QuotaUsage>> {
        self.repo.list(scope).await
    }

    /// Get quota usage for a specific resource type.
    pub async fn get_by_type(&self, scope: &TenantScope, resource_type: &str) -> AppResult<QuotaUsage> {
        if !VALID_RESOURCE_TYPES.contains(&resource_type) {
            return Err(
                ErrorKind::Validation(format!("resource_type must be one of: {:?}", VALID_RESOURCE_TYPES)).into()
            );
        }
        self.repo.get_by_type(scope, resource_type).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_resource_types() {
        assert!(VALID_RESOURCE_TYPES.contains(&"agents"));
        assert!(VALID_RESOURCE_TYPES.contains(&"storage"));
        assert!(VALID_RESOURCE_TYPES.contains(&"events"));
    }

    #[test]
    fn invalid_resource_type() {
        assert!(!VALID_RESOURCE_TYPES.contains(&"cpu"));
        assert!(!VALID_RESOURCE_TYPES.contains(&""));
    }
}
