//! Legacy navigation service.
//!
//! The URL surface is historical, but this service owns the active tree-pane
//! workflow over organizations, teams, and projects.

use agentforge_core::{AppResult, OrgId, ProjectId, TeamId, TenantScope, WorkspaceId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::navigation::{LegacyOrg, LegacyProject, LegacyTeam};
use crate::domain::resource::NavigationResourcePolicy;
use crate::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use crate::repositories::project_clone::ProjectCloneRepository;
use crate::repositories::resource::navigation::{
    LegacyNavigationRepository, LegacyOrgRow, LegacyProjectRow, LegacyTeamRow,
};
use crate::services::organization::{OrganizationService, UpdateOrganizationInput};
use crate::services::project::ProjectService;
use crate::services::resource_permission::ResourcePermissionService;

pub(crate) use crate::domain::navigation::{
    legacy_delete_response, legacy_org_response, legacy_orgs_response, legacy_project_response,
    legacy_projects_response, legacy_team_response, legacy_teams_response,
};

pub(crate) struct LegacyNavigationService {
    navigation: LegacyNavigationRepository,
    organizations: OrganizationService,
    permissions: ResourcePermissionService,
    projects: ProjectRepository,
    clones: ProjectCloneRepository,
}

impl LegacyNavigationService {
    pub(crate) fn new(
        navigation: LegacyNavigationRepository,
        organizations: OrganizationService,
        permissions: ResourcePermissionService,
        projects: ProjectRepository,
        clones: ProjectCloneRepository,
    ) -> Self {
        Self { navigation, organizations, permissions, projects, clones }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(
            LegacyNavigationRepository::new(pool.clone()),
            OrganizationService::from_pool(pool.clone()),
            ResourcePermissionService::from_pool(pool.clone()),
            ProjectRepository::new(pool.clone()),
            ProjectCloneRepository::new(pool),
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
        let name = NavigationResourcePolicy::org_update_name(name)?;

        self.organizations.update(scope, OrgId::from(org_id), UpdateOrganizationInput { name }).await?;
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
        let mut projects: Vec<LegacyProject> =
            self.navigation.list_projects(scope, TeamId::from(team_id)).await?.into_iter().map(Into::into).collect();

        // Attach each project's latest clone-attempt detail in ONE batched read
        // (no N+1), so the tree pane shows branch/head_sha on success and the
        // redacted error + class on failure alongside the `clone_status` badge.
        let ids: Vec<Uuid> = projects.iter().map(|p| p.id).collect();
        let mut summaries = self.clones.latest_attempt_summaries_for_projects(scope, &ids).await?;
        for project in &mut projects {
            project.clone = summaries.remove(&project.id).as_ref().map(ProjectService::clone_summary_of);
        }
        Ok(projects)
    }

    pub(crate) async fn create_project(
        &self,
        scope: &TenantScope,
        team_id: Uuid,
        name: String,
        slug: Option<String>,
        color: Option<String>,
        description: Option<String>,
        repository_url: Option<String>,
    ) -> AppResult<LegacyProject> {
        let team_id = TeamId::from(team_id);
        // The draft still validates the name + carries color/description, but the
        // caller-supplied `slug` is intentionally DISCARDED for the on-disk
        // identity: `workspace_dir_name` (and the `slug` column) are derived by
        // the filesystem-safe policy inside the transaction, so a raw caller slug
        // can never become a directory name.
        let draft = NavigationResourcePolicy::project_create_draft(name, slug, color, description)?;
        let clone = match repository_url.as_deref() {
            Some(url) => Some(CloneRequest::parse(url)?),
            None => None,
        };
        let org_id = self.navigation.find_project_parent_org(scope, team_id).await?;
        self.permissions.require_project_creator(scope, team_id).await?;
        let workspace_id = self.navigation.default_workspace_for_org(org_id).await?;

        // The response mirrors the column defaults the tx applies (the INSERT
        // COALESCEs a missing color to '#007AFF' and a missing description to '').
        let resolved_color = draft.color.clone().unwrap_or_else(|| "#007AFF".to_string());
        let resolved_description = draft.description.clone().unwrap_or_default();

        // The SAME transactional create path as the flat `ProjectService`:
        // workspace-ownership validation, dir-name allocation, project + default
        // group + (when a repo is present) clone attempt + outbox, all in one tx.
        let project = self
            .projects
            .create_with_clone(
                scope,
                ProjectCreateTx {
                    workspace_id: WorkspaceId::from(workspace_id),
                    team_id: team_id.as_uuid(),
                    name: draft.name,
                    color: draft.color,
                    description: draft.description,
                    clone,
                },
            )
            .await?;

        // The freshly-created clone attempt (queued) so the create response shows
        // clone status immediately; `None` when no repo was supplied.
        let clone_summary =
            self.clones.latest_attempt_summary(scope, project.id).await?.as_ref().map(ProjectService::clone_summary_of);

        // Project a fresh-create `LegacyProject` response. The creator always has
        // manage/delete on the project they just created; `slug` mirrors the
        // derived `workspace_dir_name` (raw caller slugs are no longer persisted).
        Ok(LegacyProject {
            id: project.id.as_uuid(),
            team_id: team_id.as_uuid(),
            workspace_id: project.workspace_id.as_uuid(),
            name: project.name.clone(),
            slug: project.workspace_dir_name.clone(),
            color: resolved_color,
            description: resolved_description,
            can_manage: true,
            can_delete: true,
            clone_status: project.clone_status.clone(),
            clone: clone_summary,
        })
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
            clone_status: row.clone_status,
            // The per-attempt detail is attached by the service after listing (it
            // needs a separate, batched attempt read); the row adapter alone has
            // only the denormalized summary column.
            clone: None,
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
            clone_status: "queued".to_string(),
        });
        assert_eq!(project.team_id, team_id);
        assert_eq!(project.workspace_id, workspace_id);
        assert_eq!(project.clone_status, "queued");
        // The row adapter alone has only the summary column; the per-attempt
        // detail is attached by the service after listing.
        assert!(project.clone.is_none());
    }
}
