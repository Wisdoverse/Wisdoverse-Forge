//! Authorization rules for organization, team, and project management.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope};
use sqlx::PgPool;

use crate::domain::resource::ResourcePermissionPolicy;
use crate::repositories::resource::permission::ResourcePermissionRepository;

/// Business rules for resource-scoped management permissions.
pub struct ResourcePermissionService {
    repo: ResourcePermissionRepository,
}

impl ResourcePermissionService {
    pub fn new(repo: ResourcePermissionRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(ResourcePermissionRepository::new(pool))
    }

    pub async fn require_org_manager(&self, scope: &TenantScope) -> AppResult<()> {
        ResourcePermissionPolicy::ensure_can_manage_org(self.repo.can_manage_org(scope).await?)
    }

    pub async fn require_team_manager(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<()> {
        ResourcePermissionPolicy::ensure_can_manage_team(self.repo.can_manage_team(scope, team_id).await?)
    }

    pub async fn require_project_creator(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<()> {
        ResourcePermissionPolicy::ensure_can_create_project(self.repo.can_create_project_in_team(scope, team_id).await?)
    }

    pub async fn require_project_manager(&self, scope: &TenantScope, project_id: ProjectId) -> AppResult<()> {
        ResourcePermissionPolicy::ensure_can_manage_project(self.repo.can_manage_project(scope, project_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> TenantScope {
        crate::test_support::tenant_scope()
    }

    #[test]
    fn forbidden_maps_to_permission_denial() {
        assert!(ResourcePermissionPolicy::ensure_can_manage_org(false).is_err());
    }

    #[test]
    fn tenant_scope_carries_org_and_user_boundaries() {
        let scope = scope();
        assert_ne!(scope.org_id().as_uuid(), scope.user_id().as_uuid());
    }
}
