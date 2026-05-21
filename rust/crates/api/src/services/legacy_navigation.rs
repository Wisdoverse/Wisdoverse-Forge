//! Legacy navigation service.
//!
//! The URL surface is historical, but this service owns the active tree-pane
//! workflow over organizations, teams, and projects.

use agentforge_core::{AppResult, ErrorKind, OrgId, ProjectId, TeamId, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::navigation::{LegacyOrg, LegacyProject, LegacyTeam};
use crate::domain::resource::NavigationResourcePolicy;
use crate::repositories::resource::navigation::{
    LegacyNavigationRepository, LegacyOrgRow, LegacyProjectRow, LegacyTeamRow,
};
use crate::services::group::GroupService;
use crate::services::organization::{OrganizationService, UpdateOrganizationInput};
use crate::services::resource_permission::ResourcePermissionService;

pub(crate) use crate::domain::navigation::{
    legacy_delete_response, legacy_org_response, legacy_orgs_response, legacy_project_response,
    legacy_projects_response, legacy_team_response, legacy_teams_response,
};

pub(crate) struct LegacyNavigationService {
    navigation: LegacyNavigationRepository,
    organizations: OrganizationService,
    permissions: ResourcePermissionService,
    groups: GroupService,
}

impl LegacyNavigationService {
    pub(crate) fn new(
        navigation: LegacyNavigationRepository,
        organizations: OrganizationService,
        permissions: ResourcePermissionService,
        groups: GroupService,
    ) -> Self {
        Self { navigation, organizations, permissions, groups }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(
            LegacyNavigationRepository::new(pool.clone()),
            OrganizationService::from_pool(pool.clone()),
            ResourcePermissionService::from_pool(pool.clone()),
            GroupService::from_pool(pool),
        )
    }

    pub(crate) async fn list_orgs(&self, scope: &TenantScope) -> AppResult<Vec<LegacyOrg>> {
        Ok(self.navigation.list_orgs(scope).await?.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn get_org(&self, scope: &TenantScope, org_id: Uuid) -> AppResult<LegacyOrg> {
        self.navigation.fetch_org_for_user(scope, org_id).await.map(Into::into)
    }

    pub(crate) async fn update_org(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        name: Option<String>,
    ) -> AppResult<LegacyOrg> {
        let Some(name) = name.as_deref() else {
            return Err(ErrorKind::Validation("name is required".into()).into());
        };

        self.organizations
            .update(scope, OrgId::from(org_id), UpdateOrganizationInput { name: name.to_string() })
            .await?;
        self.get_org(scope, org_id).await
    }

    pub(crate) async fn list_teams(&self, scope: &TenantScope, org_id: Uuid) -> AppResult<Vec<LegacyTeam>> {
        Ok(self.navigation.list_teams(scope, org_id).await?.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn create_team(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        name: String,
        slug: Option<String>,
        visibility: Option<String>,
        description: Option<String>,
    ) -> AppResult<LegacyTeam> {
        let draft = NavigationResourcePolicy::team_create_draft(name, slug, visibility, description)?;
        self.permissions.require_org_manager(scope).await?;
        self.navigation.insert_team(scope, org_id, draft).await.map(Into::into)
    }

    pub(crate) async fn update_team(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: Uuid,
        name: Option<String>,
        slug: Option<String>,
        visibility: Option<String>,
        description: Option<String>,
    ) -> AppResult<LegacyTeam> {
        let team_id = TeamId::from(team_id);
        let draft = NavigationResourcePolicy::team_update_draft(name, slug, visibility, description)?;
        self.permissions.require_team_manager(scope, team_id).await?;
        self.navigation.update_team(scope, org_id, team_id, draft).await.map(Into::into)
    }

    pub(crate) async fn delete_team(&self, scope: &TenantScope, org_id: Uuid, team_id: Uuid) -> AppResult<()> {
        let team_id = TeamId::from(team_id);
        self.permissions.require_team_manager(scope, team_id).await?;
        self.navigation.delete_team(scope, org_id, team_id).await
    }

    pub(crate) async fn list_projects(&self, scope: &TenantScope, team_id: Uuid) -> AppResult<Vec<LegacyProject>> {
        Ok(self.navigation.list_projects(scope, TeamId::from(team_id)).await?.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn create_project(
        &self,
        scope: &TenantScope,
        team_id: Uuid,
        name: String,
        slug: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> AppResult<LegacyProject> {
        let team_id = TeamId::from(team_id);
        let draft = NavigationResourcePolicy::project_create_draft(name, slug, color, description)?;
        let org_id = self.navigation.find_project_parent_org(scope, team_id).await?;
        self.permissions.require_project_creator(scope, team_id).await?;
        let workspace_id = self.navigation.default_workspace_for_org(org_id).await?;
        let project: LegacyProject = self.navigation.insert_project(org_id, workspace_id, team_id, draft).await?.into();
        self.groups.find_or_create_default_for_project(scope, ProjectId::from(project.id)).await?;
        Ok(project)
    }

    pub(crate) async fn update_project(
        &self,
        scope: &TenantScope,
        team_id: Uuid,
        project_id: Uuid,
        name: Option<String>,
        slug: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> AppResult<LegacyProject> {
        let team_id = TeamId::from(team_id);
        let project_id = ProjectId::from(project_id);
        let draft = NavigationResourcePolicy::project_update_draft(name, slug, color, description)?;
        self.permissions.require_project_manager(scope, project_id).await?;
        self.navigation.update_project(scope, team_id, project_id, draft).await.map(Into::into)
    }

    pub(crate) async fn delete_project(&self, scope: &TenantScope, team_id: Uuid, project_id: Uuid) -> AppResult<()> {
        let team_id = TeamId::from(team_id);
        let project_id = ProjectId::from(project_id);
        self.permissions.require_project_manager(scope, project_id).await?;
        self.navigation.delete_project(scope, team_id, project_id).await
    }
}

impl From<LegacyOrgRow> for LegacyOrg {
    fn from(row: LegacyOrgRow) -> Self {
        Self { id: row.id, name: row.name, slug: row.slug, plan: row.plan, role: row.role }
    }
}

impl From<LegacyTeamRow> for LegacyTeam {
    fn from(row: LegacyTeamRow) -> Self {
        Self {
            id: row.id,
            org_id: row.org_id,
            name: row.name,
            slug: row.slug,
            visibility: row.visibility,
            description: row.description,
            can_manage: row.can_manage,
            can_delete: row.can_delete,
            can_create_project: row.can_create_project,
        }
    }
}

impl From<LegacyProjectRow> for LegacyProject {
    fn from(row: LegacyProjectRow) -> Self {
        Self {
            id: row.id,
            team_id: row.team_id,
            workspace_id: row.workspace_id,
            name: row.name,
            slug: row.slug,
            color: row.color,
            description: row.description,
            can_manage: row.can_manage,
            can_delete: row.can_delete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_adapters_preserve_legacy_navigation_fields() {
        let org_id = Uuid::nil();
        let team_id = Uuid::nil();
        let workspace_id = Uuid::nil();

        let team = LegacyTeam::from(LegacyTeamRow {
            id: team_id,
            org_id,
            name: "Engineering".to_string(),
            slug: "engineering".to_string(),
            visibility: "private".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
            can_create_project: true,
        });
        assert_eq!(team.org_id, org_id);
        assert!(team.can_create_project);

        let project = LegacyProject::from(LegacyProjectRow {
            id: Uuid::nil(),
            team_id,
            workspace_id,
            name: "Forge".to_string(),
            slug: "forge".to_string(),
            color: "#007AFF".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
        });
        assert_eq!(project.team_id, team_id);
        assert_eq!(project.workspace_id, workspace_id);
    }
}
