//! Agent application service — coordinates domain rules with persistence.

use agentforge_core::{AgentId, AgentStatus, AppResult, TenantScope};
use agentforge_db::entities::{Agent, AgentCollaborator};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentCliToolSelection, AgentCollaboratorPermission, AgentCommandSubject, AgentLifecycle, AgentListPage, AgentName,
    AgentPermissionProjection, AgentStatusTransition, agent_permission_projection,
};
pub(crate) use crate::domain::agent::{
    agent_container_status_response, agent_data_response, agent_delete_response, agent_git_status_response,
    agent_list_response, agent_messages_deleted_response, agent_messages_response, agent_permission_response,
    agent_prompt_sent_response, agent_response, agent_status_response,
};
pub(crate) use crate::repositories::agent::CreateAgentParams;
use crate::repositories::agent::{AgentListItem, AgentRepository};
use crate::services::agent_workspace::{AgentWorkspaceService, resolve_agent_workspace_paths, workspace_root_from_env};

/// Application service for agent operations.
pub struct AgentService {
    repo: AgentRepository,
    workspace_resolver: Option<AgentWorkspaceResolver>,
}

struct AgentWorkspaceResolver {
    workspaces: AgentWorkspaceService,
    workspace_root: String,
}

impl AgentService {
    pub fn new(repo: AgentRepository) -> Self {
        Self { repo, workspace_resolver: None }
    }

    pub(crate) fn from_pool_with_workspace(pool: PgPool) -> Self {
        Self::new(AgentRepository::new(pool.clone()))
            .with_workspace_resolver(AgentWorkspaceService::from_pool(pool), workspace_root_from_env())
    }

    pub(crate) fn with_workspace_resolver(mut self, workspaces: AgentWorkspaceService, workspace_root: String) -> Self {
        self.workspace_resolver = Some(AgentWorkspaceResolver { workspaces, workspace_root });
        self
    }

    /// Build the NATS subject for agent sidecar commands.
    pub(crate) fn command_subject(agent_id: &str) -> String {
        AgentCommandSubject::for_agent_id(agent_id)
    }

    /// List agents with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Agent>> {
        let page = AgentListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
    }

    /// List agents enriched with owner + project names, for frontend display.
    pub async fn list_with_owner(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<AgentListItem>> {
        let page = AgentListPage::new(limit, offset);
        self.repo.list_with_owner(scope, page.limit(), page.offset()).await
    }

    /// Get a single agent by ID.
    pub async fn get(&self, scope: &TenantScope, id: AgentId) -> AppResult<Agent> {
        self.repo.find_by_id(scope, id).await
    }

    /// Get a single agent by ID enriched with owner + project names.
    pub async fn get_with_owner(&self, scope: &TenantScope, id: AgentId) -> AppResult<AgentListItem> {
        self.repo.find_with_owner_by_id(scope, id).await
    }

    /// Create a new agent with optional metadata. `cli_tool` distinguishes
    /// Container CLI agents (`claude`/`codex`/`gemini`/`opencode`) from pure
    /// provider+prompt agents (cli_tool = None).
    pub async fn create(&self, scope: &TenantScope, params: CreateAgentParams<'_>) -> AppResult<Agent> {
        AgentName::validate(params.name)?;
        let mut params = params;
        params.cli_tool = AgentCliToolSelection::normalize(params.cli_tool)?;
        if self.workspace_resolver.is_none() {
            return self.repo.create(scope, params).await;
        }
        let resolved_cwd = self.resolve_container_workspace(scope, &mut params).await?;
        params.cwd = resolved_cwd.as_deref();
        self.repo.create(scope, params).await
    }

    async fn resolve_container_workspace(
        &self,
        scope: &TenantScope,
        params: &mut CreateAgentParams<'_>,
    ) -> AppResult<Option<String>> {
        let Some(resolver) = &self.workspace_resolver else {
            return Ok(None);
        };

        let workspace_scope = resolver
            .workspaces
            .resolve_workspace_mount_scope(scope.org_id().as_uuid(), params.workspace_id, params.project_id)
            .await?;
        params.workspace_id = Some(workspace_scope.workspace_id);

        if params.cli_tool.is_none() {
            return Ok(None);
        }

        let paths = resolve_agent_workspace_paths(&resolver.workspace_root, workspace_scope, params.cwd)?;
        Ok(Some(paths.container_cwd))
    }

    /// Update agent fields (name, model, provider, system_prompt).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: AgentId,
        name: Option<&str>,
        model: Option<&str>,
        provider: Option<&str>,
        system_prompt: Option<&str>,
    ) -> AppResult<Agent> {
        AgentName::validate(name)?;
        self.repo.update(scope, id, name, model, provider, system_prompt).await
    }

    /// Update agent status with state machine validation.
    pub async fn update_status(&self, scope: &TenantScope, id: AgentId, new_status: AgentStatus) -> AppResult<Agent> {
        let agent = self.repo.find_by_id(scope, id).await?;

        match AgentLifecycle::transition(agent.status, new_status)? {
            AgentStatusTransition::Noop => Ok(agent),
            AgentStatusTransition::Change(status) => self.repo.update_status(scope, id, status).await,
        }
    }

    /// Delete an agent.
    pub async fn delete(&self, scope: &TenantScope, id: AgentId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    pub(crate) async fn set_container(
        &self,
        scope: &TenantScope,
        id: AgentId,
        container_id: &str,
        hmac_secret: &str,
        nats_connect_password: &str,
    ) -> AppResult<Agent> {
        self.repo.set_container(scope, id, container_id, hmac_secret, nats_connect_password).await
    }

    pub(crate) async fn clear_container(&self, scope: &TenantScope, id: AgentId) -> AppResult<Agent> {
        self.repo.clear_container(scope, id).await
    }

    // --- Collaborator operations ---

    /// List collaborators for an agent.
    pub async fn list_collaborators(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Vec<AgentCollaborator>> {
        self.repo.list_collaborators(scope, agent_id).await
    }

    /// Add a collaborator with validated permission.
    pub async fn add_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        let permission = AgentCollaboratorPermission::parse(permission)?;
        self.repo.add_collaborator(scope, agent_id, user_id, permission.as_str()).await
    }

    /// Update a collaborator's permission.
    pub async fn update_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        let permission = AgentCollaboratorPermission::parse(permission)?;
        self.repo.update_collaborator(scope, agent_id, user_id, permission.as_str()).await
    }

    /// Remove a collaborator from an agent.
    pub async fn remove_collaborator(&self, scope: &TenantScope, agent_id: AgentId, user_id: Uuid) -> AppResult<()> {
        self.repo.remove_collaborator(scope, agent_id, user_id).await
    }

    /// Check whether a user can perform an agent action.
    pub(crate) async fn check_permission(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        action: &str,
    ) -> AppResult<AgentPermissionProjection> {
        let agent = self.repo.find_by_id(scope, agent_id).await?;
        let is_owner = agent.user_id.as_uuid() == user_id;
        let collaborators = self.repo.list_collaborators(scope, agent_id).await?;
        let collaborator_permission = collaborators
            .iter()
            .find(|collaborator| collaborator.user_id.as_uuid() == user_id)
            .map(|collaborator| collaborator.permission.as_str());

        Ok(agent_permission_projection(is_owner, collaborator_permission, action))
    }
}
