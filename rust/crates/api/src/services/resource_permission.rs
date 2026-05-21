//! Authorization rules for organization, team, and project management.

use agentforge_core::{AppResult, ErrorKind, ProjectId, TeamId, TenantScope};
use sqlx::PgPool;

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
        if self.repo.can_manage_org(scope).await? {
            return Ok(());
        }
        Err(ErrorKind::Forbidden.into())
    }

    pub async fn require_team_manager(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<()> {
        if self.repo.can_manage_team(scope, team_id).await? {
            return Ok(());
        }
        Err(ErrorKind::Forbidden.into())
    }

    pub async fn require_project_creator(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<()> {
        if self.repo.can_create_project_in_team(scope, team_id).await? {
            return Ok(());
        }
        Err(ErrorKind::Forbidden.into())
    }

    pub async fn require_project_manager(&self, scope: &TenantScope, project_id: ProjectId) -> AppResult<()> {
        if self.repo.can_manage_project(scope, project_id).await? {
            return Ok(());
        }
        Err(ErrorKind::Forbidden.into())
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
        let err: agentforge_core::AppError = ErrorKind::Forbidden.into();
        assert!(matches!(err.kind, ErrorKind::Forbidden));
    }

    #[test]
    fn tenant_scope_carries_org_and_user_boundaries() {
        let scope = scope();
        assert_ne!(scope.org_id().as_uuid(), scope.user_id().as_uuid());
    }
}
