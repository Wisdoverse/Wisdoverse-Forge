//! Favorite service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Favorite;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::resource::FavoriteTargetType;
pub(crate) use crate::domain::resource::{
    resource_data_response as favorite_data_response, resource_delete_response as favorite_delete_response,
};
use crate::repositories::favorite::FavoriteRepository;

/// Business logic layer for favorite operations.
pub struct FavoriteService {
    repo: FavoriteRepository,
}

impl FavoriteService {
    pub fn new(repo: FavoriteRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(FavoriteRepository::new(pool))
    }

    /// List all favorites for the user.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Favorite>> {
        self.repo.list(scope).await
    }

    /// Add a new favorite after validating target_type.
    pub async fn add(&self, scope: &TenantScope, target_type: &str, target_id: Uuid) -> AppResult<Favorite> {
        let target_type = FavoriteTargetType::parse(target_type)?;
        self.repo.create(scope, target_type.value(), target_id).await
    }

    /// Remove a favorite by ID.
    pub async fn remove(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::FavoriteTargetType;

    #[test]
    fn valid_target_types() {
        assert!(FavoriteTargetType::parse("agent").is_ok());
        assert!(FavoriteTargetType::parse("project").is_ok());
        assert!(FavoriteTargetType::parse("workspace").is_ok());
    }

    #[test]
    fn invalid_target_type_rejected() {
        assert!(FavoriteTargetType::parse("user").is_err());
        assert!(FavoriteTargetType::parse("team").is_err());
        assert!(FavoriteTargetType::parse("").is_err());
    }

    #[test]
    fn target_type_is_case_sensitive() {
        assert!(FavoriteTargetType::parse("Agent").is_err());
        assert!(FavoriteTargetType::parse("PROJECT").is_err());
    }
}
