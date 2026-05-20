//! Team and project member management.

use agentforge_core::{AppResult, ErrorKind, ProjectId, TeamId, TenantScope};
use uuid::Uuid;

use crate::domain::resource::{ResourceMemberRole, ResourceOrganizationPolicy};
pub(crate) use crate::domain::resource::{
    resource_delete_response, resource_member_response, resource_members_response,
};
use crate::repositories::resource::member::{ResourceMember, ResourceMemberRepository};
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::resource_permission::ResourcePermissionService;

pub struct ResourceMemberService {
    repo: ResourceMemberRepository,
    permissions: ResourcePermissionService,
}

impl ResourceMemberService {
    pub fn new(repo: ResourceMemberRepository, permission_repo: ResourcePermissionRepository) -> Self {
        Self { repo, permissions: ResourcePermissionService::new(permission_repo) }
    }

    pub async fn list_team_members(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
    ) -> AppResult<Vec<ResourceMember>> {
        ResourceOrganizationPolicy::ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        self.repo.list_team_members(scope, org_id, team_id).await
    }

    pub async fn add_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
        role: Option<&str>,
    ) -> AppResult<ResourceMember> {
        ResourceOrganizationPolicy::ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        let role = ResourceMemberRole::normalize(role)?;
        self.repo.add_team_member(scope, org_id, team_id, user_id, role.as_str()).await
    }

    pub async fn add_team_member_by_email(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        email: &str,
        role: Option<&str>,
    ) -> AppResult<ResourceMember> {
        let user_id = self
            .repo
            .find_org_user_by_email(scope, email)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("org user {}", email.trim())))?;
        self.add_team_member(scope, org_id, team_id, user_id, role).await
    }

    pub async fn update_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        ResourceOrganizationPolicy::ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        let role = ResourceMemberRole::normalize(Some(role))?;
        self.repo.update_team_member(scope, org_id, team_id, user_id, role.as_str()).await
    }

    pub async fn remove_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
    ) -> AppResult<()> {
        ResourceOrganizationPolicy::ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        self.repo.remove_team_member(scope, org_id, team_id, user_id).await
    }

    pub async fn list_project_members(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
    ) -> AppResult<Vec<ResourceMember>> {
        self.permissions.require_project_manager(scope, project_id).await?;
        self.repo.list_project_members(scope, project_id).await
    }

    pub async fn add_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
        role: Option<&str>,
    ) -> AppResult<ResourceMember> {
        self.permissions.require_project_manager(scope, project_id).await?;
        let role = ResourceMemberRole::normalize(role)?;
        self.repo.add_project_member(scope, project_id, user_id, role.as_str()).await
    }

    pub async fn add_project_member_by_email(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        email: &str,
        role: Option<&str>,
    ) -> AppResult<ResourceMember> {
        let user_id = self
            .repo
            .find_org_user_by_email(scope, email)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("org user {}", email.trim())))?;
        self.add_project_member(scope, project_id, user_id, role).await
    }

    pub async fn update_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        self.permissions.require_project_manager(scope, project_id).await?;
        let role = ResourceMemberRole::normalize(Some(role))?;
        self.repo.update_project_member(scope, project_id, user_id, role.as_str()).await
    }

    pub async fn remove_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
    ) -> AppResult<()> {
        self.permissions.require_project_manager(scope, project_id).await?;
        self.repo.remove_project_member(scope, project_id, user_id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::ResourceMemberRole;

    #[test]
    fn normalizes_legacy_member_roles() {
        assert_eq!(ResourceMemberRole::normalize(Some("editor")).unwrap().as_str(), "maintainer");
        assert_eq!(ResourceMemberRole::normalize(Some("viewer")).unwrap().as_str(), "member");
    }

    #[test]
    fn rejects_unknown_member_roles() {
        assert!(ResourceMemberRole::normalize(Some("root")).is_err());
    }
}
