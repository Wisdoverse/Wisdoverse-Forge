//! Favorite service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Favorite;
use uuid::Uuid;

use crate::repositories::favorite::FavoriteRepository;

/// Valid target types for favorites.
const VALID_TARGET_TYPES: &[&str] = &["agent", "project", "workspace"];

/// Business logic layer for favorite operations.
pub struct FavoriteService {
    repo: FavoriteRepository,
}

impl FavoriteService {
    pub fn new(repo: FavoriteRepository) -> Self {
        Self { repo }
    }

    /// List all favorites for the user.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Favorite>> {
        self.repo.list(scope).await
    }

    /// Add a new favorite after validating target_type.
    pub async fn add(&self, scope: &TenantScope, target_type: &str, target_id: Uuid) -> AppResult<Favorite> {
        if !VALID_TARGET_TYPES.contains(&target_type) {
            return Err(ErrorKind::Validation(format!("target_type must be one of: {:?}", VALID_TARGET_TYPES)).into());
        }
        self.repo.create(scope, target_type, target_id).await
    }

    /// Remove a favorite by ID.
    pub async fn remove(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_target_types() {
        assert!(VALID_TARGET_TYPES.contains(&"agent"));
        assert!(VALID_TARGET_TYPES.contains(&"project"));
        assert!(VALID_TARGET_TYPES.contains(&"workspace"));
    }

    #[test]
    fn invalid_target_type_rejected() {
        assert!(!VALID_TARGET_TYPES.contains(&"user"));
        assert!(!VALID_TARGET_TYPES.contains(&"team"));
        assert!(!VALID_TARGET_TYPES.contains(&""));
    }

    #[test]
    fn target_type_is_case_sensitive() {
        assert!(!VALID_TARGET_TYPES.contains(&"Agent"));
        assert!(!VALID_TARGET_TYPES.contains(&"PROJECT"));
    }
}
