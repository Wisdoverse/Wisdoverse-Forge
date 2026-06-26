//! Agent workspace persistence helpers.

use agentforge_core::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::agent_workspace::{
    AgentWorkspacePaths, CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, host_path_for_container_cwd,
    resolve_agent_workspace_paths,
};
use crate::repositories::agent::AgentWorkspaceRepository;

const DEFAULT_WORKSPACE_ROOT: &str = "/data/agentforge/workspaces";

pub(crate) fn workspace_root_from_env() -> String {
    std::env::var("AGENTFORGE_WORKSPACE_ROOT").unwrap_or_else(|_| DEFAULT_WORKSPACE_ROOT.to_string())
}

#[derive(Clone)]
pub(crate) struct AgentWorkspaceService {
    repo: AgentWorkspaceRepository,
}

impl AgentWorkspaceService {
    pub(crate) fn new(repo: AgentWorkspaceRepository) -> Self {
        Self { repo }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(AgentWorkspaceRepository::new(pool))
    }

    pub(crate) async fn resolve_workspace_mount_scope(
        &self,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        project_id: Option<Uuid>,
    ) -> AppResult<WorkspaceMountScope> {
        if let Some(project_id) = project_id {
            let row = self.repo.project_workspace(org_id, project_id).await?;
            return WorkspaceMountScope::for_project(row.organization_id, workspace_id, row.workspace_id);
        }

        if let Some(workspace_id) = workspace_id {
            self.ensure_workspace_belongs_to_org(org_id, workspace_id).await?;
            return Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id));
        }

        let workspace_id = self.default_workspace_for_org(org_id).await?;
        Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id))
    }

    pub(crate) async fn ensure_workspace_belongs_to_org(&self, org_id: Uuid, workspace_id: Uuid) -> AppResult<()> {
        self.repo.ensure_workspace_belongs_to_org(org_id, workspace_id).await
    }

    pub(crate) async fn default_workspace_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
        self.repo.default_workspace_for_org(org_id).await
    }
}
