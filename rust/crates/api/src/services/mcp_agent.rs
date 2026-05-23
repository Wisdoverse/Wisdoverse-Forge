use std::collections::HashMap;

use agentforge_core::{AgentStatus, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::agent::{McpAgentPrompt, McpAgentRuntimePolicy};
use crate::domain::credential::ContainerCliCredentialPolicy;
use crate::services::agent_workspace::{WorkspaceMountScope, resolve_agent_workspace_paths};

#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub project_id: Option<Uuid>,
    pub cli_tool: String,
    pub name: Option<String>,
    pub org_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct CreateSessionResult {
    pub agent_id: Uuid,
    pub status: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    pub agent_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRuntimeContext {
    pub project_id: Option<Uuid>,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAgentRecord {
    pub agent_id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub status: AgentStatus,
    pub container_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAgentRuntimeConfig {
    pub workspace_root: String,
    pub default_image: String,
    pub tool_images: HashMap<String, String>,
    pub system_api_keys: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAgentRuntimeCreate {
    pub agent_id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub image: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAgentRuntimeCreateResult {
    pub container_id: String,
}

#[async_trait]
pub trait McpAgentStore: Send + Sync {
    async fn resolve_project_context(
        &self,
        project_id: Option<Uuid>,
        org_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> AppResult<ProjectRuntimeContext>;

    async fn insert_agent(&self, record: McpAgentRecord) -> AppResult<()>;

    async fn get_agent(&self, agent_id: Uuid) -> AppResult<McpAgentRecord>;

    async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()>;

    async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()>;
}

#[async_trait]
pub trait McpAgentRuntime: Send + Sync {
    async fn create_agent(&self, req: McpAgentRuntimeCreate) -> AppResult<McpAgentRuntimeCreateResult>;
    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()>;
    async fn destroy_agent(&self, agent_id: Uuid) -> AppResult<()>;
    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus>;
}

#[async_trait]
pub trait McpAgentTools: Send + Sync {
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult>;
    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()>;
    async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()>;
    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus>;
}

pub struct McpAgentService<S, R> {
    store: S,
    runtime: R,
    config: McpAgentRuntimeConfig,
}

impl<S, R> McpAgentService<S, R> {
    pub fn new(store: S, runtime: R, config: McpAgentRuntimeConfig) -> Self {
        Self { store, runtime, config }
    }
}

impl<S, R> McpAgentService<S, R>
where
    S: McpAgentStore,
    R: McpAgentRuntime,
{
    pub async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult> {
        let cli_tool = ContainerCliCredentialPolicy::canonical_tool(&request.cli_tool)?.to_string();
        let context = self.store.resolve_project_context(request.project_id, request.org_id, request.user_id).await?;

        let agent_id = Uuid::now_v7();
        let name = request
            .name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Agent {}", &agent_id.to_string()[..8]));
        let image =
            McpAgentRuntimePolicy::image_for_tool(&cli_tool, &self.config.default_image, &self.config.tool_images);
        let workspace_paths = resolve_agent_workspace_paths(
            &self.config.workspace_root,
            WorkspaceMountScope { org_id: context.org_id, workspace_id: context.workspace_id },
            None,
        )?;
        let cwd = workspace_paths.host_projects_root.to_string_lossy().into_owned();
        let env = McpAgentRuntimePolicy::system_env_for_tool(&cli_tool, &self.config.system_api_keys);

        let runtime = McpAgentRuntimeCreate {
            agent_id,
            org_id: context.org_id,
            user_id: context.user_id,
            project_id: context.project_id,
            name: name.clone(),
            image: image.clone(),
            cwd,
            env,
        };
        let created = self.runtime.create_agent(runtime).await?;

        let record = McpAgentRecord {
            agent_id,
            organization_id: context.org_id,
            workspace_id: context.workspace_id,
            user_id: context.user_id,
            project_id: context.project_id,
            name: name.clone(),
            status: AgentStatus::Idle,
            container_id: Some(created.container_id.clone()),
            model: Some(image),
            provider: Some(ContainerCliCredentialPolicy::provider_for_tool(&cli_tool).to_string()),
            updated_at: None,
        };

        if let Err(err) = self.store.insert_agent(record).await {
            let _ = self.runtime.destroy_agent(agent_id).await;
            return Err(err);
        }

        Ok(CreateSessionResult { agent_id, status: "idle".to_string(), name })
    }

    pub async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        let prompt = McpAgentPrompt::parse(prompt)?;
        self.runtime.send_prompt(agent_id, prompt.content()).await
    }

    pub async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()> {
        self.runtime.destroy_agent(agent_id).await?;
        self.store.delete_agent(agent_id).await
    }

    pub async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        self.runtime.session_status(agent_id).await
    }
}

#[async_trait]
impl<S, R> McpAgentTools for McpAgentService<S, R>
where
    S: McpAgentStore,
    R: McpAgentRuntime,
{
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult> {
        McpAgentService::create_session(self, request).await
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        McpAgentService::send_prompt(self, agent_id, prompt).await
    }

    async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()> {
        McpAgentService::destroy_session(self, agent_id).await
    }

    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        McpAgentService::session_status(self, agent_id).await
    }
}
