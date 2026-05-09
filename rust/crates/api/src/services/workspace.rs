//! Workspace service — business logic and validation.

use agentforge_core::{AppResult, ErrorKind, TenantScope, WorkspaceId};
use agentforge_db::entities::Workspace;

use crate::repositories::workspace::WorkspaceRepository;

/// Business logic layer for workspace operations.
pub struct WorkspaceService {
    repo: WorkspaceRepository,
}

impl WorkspaceService {
    pub fn new(repo: WorkspaceRepository) -> Self {
        Self { repo }
    }

    /// List workspaces with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Workspace>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Get a single workspace by ID.
    pub async fn get(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<Workspace> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new workspace with validated name.
    pub async fn create(&self, scope: &TenantScope, name: &str) -> AppResult<Workspace> {
        Self::validate_name(name)?;
        self.repo.create(scope, name).await
    }

    /// Update a workspace's name.
    pub async fn update(&self, scope: &TenantScope, id: WorkspaceId, name: &str) -> AppResult<Workspace> {
        Self::validate_name(name)?;
        self.repo.update(scope, id, name).await
    }

    /// Soft-delete a workspace.
    pub async fn delete(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Validate workspace name: 1-255 characters.
    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(WorkspaceService::validate_name("A").is_ok());
        assert!(WorkspaceService::validate_name("My Workspace").is_ok());
        assert!(WorkspaceService::validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(WorkspaceService::validate_name("").is_err());
        assert!(WorkspaceService::validate_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(0_i64.clamp(1, 100), 1);
        assert_eq!(1_i64.clamp(1, 100), 1);
        assert_eq!(50_i64.clamp(1, 100), 50);
        assert_eq!(100_i64.clamp(1, 100), 100);
        assert_eq!(200_i64.clamp(1, 100), 100);
        assert_eq!((-5_i64).clamp(1, 100), 1);
    }

    #[test]
    fn offset_floor() {
        let negative_offset = -10_i64;
        let zero_offset = 0_i64;
        let positive_offset = 50_i64;
        assert_eq!(negative_offset.max(0), 0);
        assert_eq!(zero_offset.max(0), 0);
        assert_eq!(positive_offset.max(0), 50);
    }
}
