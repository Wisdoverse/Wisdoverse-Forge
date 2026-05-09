//! Team and project member management.

use agentforge_core::{AppResult, ErrorKind, ProjectId, TeamId, TenantScope};
use uuid::Uuid;

use crate::repositories::resource_member::{ResourceMember, ResourceMemberRepository};
use crate::repositories::resource_permission::ResourcePermissionRepository;
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
        ensure_current_org(scope, org_id)?;
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
        ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        let role = normalize_member_role(role)?;
        self.repo.add_team_member(scope, org_id, team_id, user_id, &role).await
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
        ensure_current_org(scope, org_id)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        let role = normalize_member_role(Some(role))?;
        self.repo.update_team_member(scope, org_id, team_id, user_id, &role).await
    }

    pub async fn remove_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
    ) -> AppResult<()> {
        ensure_current_org(scope, org_id)?;
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
        let role = normalize_member_role(role)?;
        self.repo.add_project_member(scope, project_id, user_id, &role).await
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
        let role = normalize_member_role(Some(role))?;
        self.repo.update_project_member(scope, project_id, user_id, &role).await
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

fn ensure_current_org(scope: &TenantScope, org_id: Uuid) -> AppResult<()> {
    if scope.org_id().as_uuid() == org_id {
        return Ok(());
    }
    Err(ErrorKind::Forbidden.into())
}

fn normalize_member_role(role: Option<&str>) -> AppResult<String> {
    let normalized = role.unwrap_or("member").trim().to_ascii_lowercase();
    match normalized.as_str() {
        "owner" | "admin" | "maintainer" | "member" => Ok(normalized),
        // Compatibility with older DOM modals that used editor/viewer labels.
        "editor" => Ok("maintainer".to_string()),
        "viewer" => Ok("member".to_string()),
        _ => Err(ErrorKind::Validation("role must be owner, admin, maintainer, or member".into()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_member_role;

    #[test]
    fn normalizes_legacy_member_roles() {
        assert_eq!(normalize_member_role(Some("editor")).unwrap(), "maintainer");
        assert_eq!(normalize_member_role(Some("viewer")).unwrap(), "member");
    }

    #[test]
    fn rejects_unknown_member_roles() {
        assert!(normalize_member_role(Some("root")).is_err());
    }
}
