//! Workspace service — business logic and validation.

use agentforge_core::{AppResult, TenantScope, WorkspaceId};
use agentforge_db::entities::Workspace;

use crate::domain::resource::{ResourceListPage, ResourceName};
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
        let page = ResourceListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
    }

    /// Get a single workspace by ID.
    pub async fn get(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<Workspace> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new workspace with validated name.
    pub async fn create(&self, scope: &TenantScope, name: &str) -> AppResult<Workspace> {
        let name = ResourceName::parse(name)?;
        self.repo.create(scope, name.value()).await
    }

    /// Update a workspace's name.
    pub async fn update(&self, scope: &TenantScope, id: WorkspaceId, name: &str) -> AppResult<Workspace> {
        let name = ResourceName::parse(name)?;
        self.repo.update(scope, id, name.value()).await
    }

    /// Soft-delete a workspace.
    pub async fn delete(&self, scope: &TenantScope, id: WorkspaceId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::{ResourceListPage, ResourceName};

    #[test]
    fn valid_names() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse("My Workspace").is_ok());
        assert!(ResourceName::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(ResourceName::parse("").is_err());
        assert!(ResourceName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(ResourceListPage::new(0, 0).limit(), 1);
        assert_eq!(ResourceListPage::new(1, 0).limit(), 1);
        assert_eq!(ResourceListPage::new(50, 0).limit(), 50);
        assert_eq!(ResourceListPage::new(100, 0).limit(), 100);
        assert_eq!(ResourceListPage::new(200, 0).limit(), 100);
        assert_eq!(ResourceListPage::new(-5, 0).limit(), 1);
    }

    #[test]
    fn offset_floor() {
        assert_eq!(ResourceListPage::new(10, -10).offset(), 0);
        assert_eq!(ResourceListPage::new(10, 0).offset(), 0);
        assert_eq!(ResourceListPage::new(10, 50).offset(), 50);
    }
}
