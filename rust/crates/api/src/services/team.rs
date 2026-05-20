//! Team service — business logic and validation.

use agentforge_core::{AppResult, TeamId, TenantScope};
use agentforge_db::entities::Team;

use crate::domain::resource::{ResourceListPage, ResourceName};
pub(crate) use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::repositories::identity::team::TeamRepository;
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::resource_permission::ResourcePermissionService;

#[derive(Debug, Clone)]
pub struct CreateTeamInput {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct UpdateTeamInput {
    pub name: String,
}

/// Business logic layer for team operations.
pub struct TeamService {
    repo: TeamRepository,
    permissions: ResourcePermissionService,
}

impl TeamService {
    pub fn new(repo: TeamRepository, permission_repo: ResourcePermissionRepository) -> Self {
        Self { repo, permissions: ResourcePermissionService::new(permission_repo) }
    }

    /// List teams with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Team>> {
        let page = ResourceListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
    }

    /// Get a single team by ID.
    pub async fn get(&self, scope: &TenantScope, id: TeamId) -> AppResult<Team> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new team with validated name.
    pub async fn create(&self, scope: &TenantScope, input: CreateTeamInput) -> AppResult<Team> {
        self.permissions.require_org_manager(scope).await?;
        let name = ResourceName::parse(&input.name)?;
        self.repo.create(scope, name.value()).await
    }

    /// Update a team's name.
    pub async fn update(&self, scope: &TenantScope, id: TeamId, input: UpdateTeamInput) -> AppResult<Team> {
        self.permissions.require_team_manager(scope, id).await?;
        let name = ResourceName::parse(&input.name)?;
        self.repo.update(scope, id, name.value()).await
    }

    /// Soft-delete a team.
    pub async fn delete(&self, scope: &TenantScope, id: TeamId) -> AppResult<()> {
        self.permissions.require_team_manager(scope, id).await?;
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::{ResourceListPage, ResourceName};

    #[test]
    fn valid_names() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse("Engineering").is_ok());
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
        assert_eq!(ResourceListPage::new(200, 0).limit(), 100);
    }

    #[test]
    fn offset_floor() {
        assert_eq!(ResourceListPage::new(10, -10).offset(), 0);
        assert_eq!(ResourceListPage::new(10, 50).offset(), 50);
    }
}
