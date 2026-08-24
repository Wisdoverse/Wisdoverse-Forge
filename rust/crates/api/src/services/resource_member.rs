//! Team and project member management.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::resource::{ResourceMemberPolicy, ResourceMemberRole, ResourceOrganizationPolicy, TeamInvitePolicy};
pub(crate) use crate::domain::resource::{
    resource_delete_response, resource_member_response, resource_members_response,
};
use crate::repositories::resource::invite::TeamInviteRepository;
use crate::repositories::resource::member::{ResourceMember, ResourceMemberRepository};
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::resource_permission::ResourcePermissionService;

/// Outcome of inviting a person to a team by email.
pub enum InviteOutcome {
    /// The person already has an account in this org — membership added now.
    Added(ResourceMember),
    /// No account for that email yet — a pending invite was created; share the link.
    Invited { invite_url: String },
}

pub struct ResourceMemberService {
    repo: ResourceMemberRepository,
    permissions: ResourcePermissionService,
    invites: TeamInviteRepository,
}

impl ResourceMemberService {
    pub fn new(
        repo: ResourceMemberRepository,
        permission_repo: ResourcePermissionRepository,
        invite_repo: TeamInviteRepository,
    ) -> Self {
        Self { repo, permissions: ResourcePermissionService::new(permission_repo), invites: invite_repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(
            ResourceMemberRepository::new(pool.clone()),
            ResourcePermissionRepository::new(pool.clone()),
            TeamInviteRepository::new(pool),
        )
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

    /// Invite a person to a team by email: existing org members are added
    /// directly; everyone else gets a one-time invite link (72 h).
    pub async fn invite_team_member_by_email(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        email: &str,
        role: Option<&str>,
        app_url: Option<&str>,
    ) -> AppResult<InviteOutcome> {
        if let Some(user_id) = self.repo.find_org_user_by_email(scope, email).await? {
            let member = self.add_team_member(scope, org_id, team_id, user_id, role).await?;
            return Ok(InviteOutcome::Added(member));
        }
        let role = ResourceMemberRole::normalize(role)?.as_str().to_string();
        let normalized = email.trim().to_lowercase();
        let token = TeamInvitePolicy::generate_token();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(TeamInvitePolicy::TTL_HOURS);
        self.invites
            .upsert_pending(
                scope,
                team_id.as_uuid(),
                &normalized,
                &role,
                &TeamInvitePolicy::hash_token(&token),
                expires_at,
            )
            .await?;
        let base = app_url
            .map(|value| value.trim_end_matches('/'))
            .filter(|value| !value.is_empty())
            .unwrap_or("http://localhost:4002");
        Ok(InviteOutcome::Invited { invite_url: format!("{base}/login?invite={token}") })
    }

    /// Redeem a one-time invite with the CURRENT user's account: the invite only
    /// matches the email it was sent to, and grants org + team memberships.
    pub async fn redeem_team_invite(
        &self,
        token: &str,
        user: &agentforge_db::entities::User,
    ) -> AppResult<serde_json::Value> {
        let invite = self
            .invites
            .find_active_by_token_hash(&TeamInvitePolicy::hash_token(token))
            .await?
            .ok_or_else(TeamInvitePolicy::invalid_or_expired)?;
        if !invite.email.eq_ignore_ascii_case(&user.email) {
            return Err(TeamInvitePolicy::email_mismatch().into());
        }
        self.invites
            .grant_memberships(invite.organization_id.as_uuid(), invite.team_id, user.id.as_uuid(), &invite.role)
            .await?;
        self.invites.mark_accepted(invite.id).await?;
        Ok(crate::domain::resource::redeem_team_invite_response(invite.organization_id.as_uuid(), invite.team_id))
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
            .ok_or_else(|| ResourceMemberPolicy::missing_org_user(email))?;
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
