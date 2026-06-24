//! Authentication service — session-context transitions.
//!
//! `UserService` owns login, register, and password reset (credential
//! lifecycle). `AuthService` owns context-switch flows: org/workspace/team/
//! project re-targeting that re-verifies the user's membership on each axis
//! and mints a new access+refresh token pair on success.

use std::sync::Arc;

use agentforge_auth::JwtManager;
use agentforge_core::{AppResult, UserId};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::auth::{
    AuthContextSwitchPolicy, SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS, SwitchContextAxes, SwitchContextResult,
    SwitchContextSuccessResponse,
};
use crate::repositories::identity::{OrganizationRepository, TeamRepository};
use crate::repositories::project::ProjectRepository;
use crate::repositories::workspace::WorkspaceRepository;

/// Cross-aggregate orchestration for session context switches.
pub struct AuthService {
    organizations: OrganizationRepository,
    teams: TeamRepository,
    workspaces: WorkspaceRepository,
    projects: ProjectRepository,
    jwt: Arc<JwtManager>,
}

impl AuthService {
    pub fn new(
        organizations: OrganizationRepository,
        teams: TeamRepository,
        workspaces: WorkspaceRepository,
        projects: ProjectRepository,
        jwt: Arc<JwtManager>,
    ) -> Self {
        Self { organizations, teams, workspaces, projects, jwt }
    }

    pub(crate) fn from_pool(pool: PgPool, jwt: Arc<JwtManager>) -> Self {
        Self::new(
            OrganizationRepository::new(pool.clone()),
            TeamRepository::new(pool.clone()),
            WorkspaceRepository::new(pool.clone()),
            ProjectRepository::new(pool),
            jwt,
        )
    }

    /// Re-targets the user's session into `org_id` along the validated axes
    /// and mints a fresh token pair. Returns `Forbidden` if any axis fails
    /// authorization.
    pub async fn switch_context(
        &self,
        user_id: UserId,
        org_id: Uuid,
        axes: SwitchContextAxes,
    ) -> AppResult<SwitchContextResult> {
        let role = self
            .organizations
            .find_member_role(user_id.as_uuid(), org_id)
            .await?
            .ok_or_else(AuthContextSwitchPolicy::missing_org_membership)?;

        self.authorize_axes(user_id, org_id, &axes).await?;

        let access_token = self
            .jwt
            .create_token_with_axes(
                user_id.as_uuid(),
                org_id,
                &role,
                axes.workspace_id(),
                axes.team_id(),
                axes.project_id(),
            )
            .map_err(AuthContextSwitchPolicy::token_creation_failed)?;

        let refresh_token = self
            .jwt
            .create_token_with_axes_and_expiry(
                user_id.as_uuid(),
                org_id,
                &role,
                axes.workspace_id(),
                axes.team_id(),
                axes.project_id(),
                SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS,
            )
            .map_err(AuthContextSwitchPolicy::refresh_token_creation_failed)?;

        Ok(SwitchContextResult {
            access_token,
            refresh_token,
            access_expires_in: self.jwt.expiry_seconds(),
            refresh_expires_in: SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS,
        })
    }

    async fn authorize_axes(&self, user_id: UserId, org_id: Uuid, axes: &SwitchContextAxes) -> AppResult<()> {
        if let Some(workspace_id) = axes.workspace_id() {
            let exists_in_org = self.workspaces.exists_in_org(workspace_id, org_id).await?;
            AuthContextSwitchPolicy::ensure_workspace_in_org(exists_in_org)?;
        }

        if let Some(team_id) = axes.team_id() {
            let can_read = self.teams.is_user_member(team_id, org_id, user_id.as_uuid()).await?;
            AuthContextSwitchPolicy::ensure_team_readable(can_read)?;
        }

        if let Some((project_id, workspace_id)) = axes.project_workspace_pair() {
            let can_read = self.projects.user_can_read(project_id, org_id, workspace_id, user_id.as_uuid()).await?;
            AuthContextSwitchPolicy::ensure_project_readable(can_read)?;
        }

        Ok(())
    }
}
