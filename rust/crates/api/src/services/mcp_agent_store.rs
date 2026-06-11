use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use agentforge_core::{AgentStatus, AppResult};

use crate::repositories::agent::{McpAgentInsertRecord, McpAgentRepository};
use crate::services::mcp_agent::{McpAgentRecord, McpAgentStore, ProjectRuntimeContext};

#[derive(Clone)]
pub(crate) struct SqlxMcpAgentStore {
    repo: McpAgentRepository,
}

impl SqlxMcpAgentStore {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { repo: McpAgentRepository::new(pool) }
    }
}

#[async_trait]
impl McpAgentStore for SqlxMcpAgentStore {
    async fn resolve_project_context(
        &self,
        project_id: Option<Uuid>,
        org_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> AppResult<ProjectRuntimeContext> {
        let row = self.repo.resolve_project_context(project_id, org_id, user_id).await?;
        Ok(ProjectRuntimeContext {
            project_id: row.project_id,
            org_id: row.organization_id,
            user_id: row.user_id,
            workspace_id: row.workspace_id,
        })
    }

    async fn insert_agent(&self, record: McpAgentRecord) -> AppResult<()> {
        self.repo
            .insert_agent(McpAgentInsertRecord {
                agent_id: record.agent_id,
                organization_id: record.organization_id,
                workspace_id: record.workspace_id,
                project_id: record.project_id,
                user_id: record.user_id,
                name: record.name,
                status: record.status,
                container_id: record.container_id,
                cli_tool: record.cli_tool,
                model: record.model,
                provider: record.provider,
            })
            .await
    }

    async fn get_agent(&self, agent_id: Uuid) -> AppResult<McpAgentRecord> {
        let agent = self.repo.get_agent(agent_id).await?;

        Ok(McpAgentRecord {
            agent_id: agent.id.as_uuid(),
            organization_id: agent.organization_id.as_uuid(),
            workspace_id: agent.workspace_id.as_uuid(),
            user_id: agent.user_id.as_uuid(),
            project_id: agent.project_id.map(|id| id.as_uuid()),
            name: agent.name.unwrap_or_else(|| format!("Agent {}", &agent.id.to_string()[..8])),
            status: agent.status,
            container_id: agent.container_id,
            cli_tool: agent.cli_tool,
            model: agent.model,
            provider: agent.provider,
            updated_at: Some(agent.updated_at),
        })
    }

    async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()> {
        self.repo.update_agent_status(agent_id, status).await
    }

    async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.repo.delete_agent(agent_id).await
    }
}
