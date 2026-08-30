use std::collections::HashMap;

use agentforge_core::{AgentStatus, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::agent::{AgentContainerRuntimePolicy, AgentRepositoryPolicy, McpAgentPrompt, McpAgentRuntimePolicy};
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
    pub org_id: Uuid,
    pub workspace_id: Uuid,
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
    pub container_image_identity: Option<serde_json::Value>,
    pub cli_tool: Option<String>,
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
    pub image_identity: serde_json::Value,
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

    async fn begin_agent_work(&self, agent_id: Uuid, expected_container_id: &str) -> AppResult<DateTime<Utc>>;

    async fn renew_agent_work_lease(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: DateTime<Utc>,
    ) -> AppResult<Option<DateTime<Utc>>>;

    async fn finish_agent_work(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: DateTime<Utc>,
        status: AgentStatus,
    ) -> AppResult<bool>;

    async fn delete_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()>;
}

#[async_trait]
pub trait McpAgentRuntime: Send + Sync {
    async fn create_agent(&self, req: McpAgentRuntimeCreate) -> AppResult<McpAgentRuntimeCreateResult>;
    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()>;
    async fn destroy_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()>;
    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus>;
}

#[async_trait]
pub trait McpAgentTools: Send + Sync {
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult>;
    async fn send_prompt(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid, prompt: &str) -> AppResult<()>;
    async fn destroy_session(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<()>;
    async fn session_status(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<SessionStatus>;
}

pub struct McpAgentService<S, R> {
    store: S,
    runtime: R,
    config: McpAgentRuntimeConfig,
    lifecycle_pool: Option<PgPool>,
}

impl<S, R> McpAgentService<S, R> {
    pub fn new(store: S, runtime: R, config: McpAgentRuntimeConfig, lifecycle_pool: PgPool) -> Self {
        Self { store, runtime, config, lifecycle_pool: Some(lifecycle_pool) }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(store: S, runtime: R, config: McpAgentRuntimeConfig) -> Self {
        Self { store, runtime, config, lifecycle_pool: None }
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
            container_image_identity: Some(created.image_identity),
            cli_tool: Some(cli_tool.clone()),
            model: Some(image),
            provider: Some(ContainerCliCredentialPolicy::provider_for_tool(&cli_tool).to_string()),
            updated_at: None,
        };

        if let Err(err) = self.store.insert_agent(record).await {
            let _ = self.runtime.destroy_agent(agent_id, Some(&created.container_id)).await;
            return Err(err);
        }

        Ok(CreateSessionResult {
            agent_id,
            status: "idle".to_string(),
            name,
            org_id: context.org_id,
            workspace_id: context.workspace_id,
        })
    }

    /// Tenant-isolation gate (#885): the bridge operates on a global `agent_id`, so
    /// every prompt/status/destroy must confirm the agent belongs to the caller's
    /// authoritative org AND workspace. Per the runtime contract `agents.workspace_id`
    /// is the execution/access boundary, so both must match. A mismatch returns the
    /// same not-found error as a missing agent so cross-scope existence is not leaked.
    async fn require_agent_in_scope(
        &self,
        org_id: Uuid,
        workspace_id: Uuid,
        agent_id: Uuid,
    ) -> AppResult<McpAgentRecord> {
        let record = self.store.get_agent(agent_id).await?;
        if record.organization_id != org_id || record.workspace_id != workspace_id {
            return Err(AgentRepositoryPolicy::agent_uuid_not_found(agent_id));
        }
        Ok(record)
    }

    pub async fn send_prompt(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        let record = self.require_agent_in_scope(org_id, workspace_id, agent_id).await?;
        let prompt = McpAgentPrompt::parse(prompt)?;
        #[cfg(test)]
        if self.lifecycle_pool.is_none() {
            return self.runtime.send_prompt(agent_id, prompt.content()).await;
        }
        let Some(tx) = self.admit_idle_lifecycle(&record).await? else {
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        };
        let result = self.runtime.send_prompt(agent_id, prompt.content()).await;
        finish_mcp_lifecycle(tx, result).await
    }

    pub async fn destroy_session(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<()> {
        let record = self.require_agent_in_scope(org_id, workspace_id, agent_id).await?;
        #[cfg(test)]
        if self.lifecycle_pool.is_none() {
            self.runtime.destroy_agent(agent_id, record.container_id.as_deref()).await?;
            return self.store.delete_agent(agent_id, record.container_id.as_deref()).await;
        }
        let Some(tx) = self.admit_idle_lifecycle(&record).await? else {
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        };
        // The container may have been replaced between the scope check above
        // and lifecycle-lock acquisition. Re-read under the lock and CAS the
        // final delete to the same authoritative container.
        let current = self.require_agent_in_scope(org_id, workspace_id, agent_id).await?;
        let result = match self.runtime.destroy_agent(agent_id, current.container_id.as_deref()).await {
            Ok(()) => self.store.delete_agent(agent_id, current.container_id.as_deref()).await,
            Err(err) => Err(err),
        };
        finish_mcp_lifecycle(tx, result).await
    }

    pub async fn session_status(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<SessionStatus> {
        let record = self.require_agent_in_scope(org_id, workspace_id, agent_id).await?;
        #[cfg(test)]
        if self.lifecycle_pool.is_none() {
            return self.runtime.session_status(agent_id).await;
        }
        let tx = self.lock_lifecycle(&record).await?;
        let result = self.runtime.session_status(agent_id).await;
        finish_mcp_lifecycle(tx, result).await
    }

    async fn admit_idle_lifecycle(&self, record: &McpAgentRecord) -> AppResult<Option<Transaction<'_, Postgres>>> {
        let mut tx = self.lock_lifecycle(record).await?;
        let idle = agentforge_db::agent_work_admission_is_idle_in_tx(&mut tx, record.organization_id, record.agent_id)
            .await?
            .unwrap_or(false);
        if !idle {
            tx.commit().await?;
            return Ok(None);
        }
        Ok(Some(tx))
    }

    async fn lock_lifecycle(&self, record: &McpAgentRecord) -> AppResult<Transaction<'_, Postgres>> {
        let Some(pool) = self.lifecycle_pool.as_ref() else {
            #[cfg(test)]
            unreachable!("test callers bypass lifecycle locking when no pool is configured");
            #[cfg(not(test))]
            unreachable!("production MCP service always has a lifecycle pool");
        };
        let mut tx = pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, record.agent_id).await?;
        Ok(tx)
    }
}

async fn finish_mcp_lifecycle<T>(tx: Transaction<'_, Postgres>, result: AppResult<T>) -> AppResult<T> {
    match (tx.commit().await, result) {
        (Ok(()), result) => result,
        (Err(commit_err), Ok(_)) => Err(commit_err.into()),
        (Err(commit_err), Err(operation_err)) => {
            tracing::warn!(error = %commit_err, "failed to release MCP Agent lifecycle transaction after operation error");
            Err(operation_err)
        }
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

    async fn send_prompt(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        McpAgentService::send_prompt(self, org_id, workspace_id, agent_id, prompt).await
    }

    async fn destroy_session(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<()> {
        McpAgentService::destroy_session(self, org_id, workspace_id, agent_id).await
    }

    async fn session_status(&self, org_id: Uuid, workspace_id: Uuid, agent_id: Uuid) -> AppResult<SessionStatus> {
        McpAgentService::session_status(self, org_id, workspace_id, agent_id).await
    }
}
