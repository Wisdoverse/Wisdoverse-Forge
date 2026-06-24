//! Agent aggregate persistence for the internal MCP bridge.

use agentforge_core::{AgentStatus, AppResult};
use agentforge_db::entities::Agent;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{AgentCreateRuntimePolicy, AgentRepositoryPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpProjectRuntimeContextRow {
    pub(crate) project_id: Option<Uuid>,
    pub(crate) organization_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) user_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpAgentInsertRecord {
    pub(crate) agent_id: Uuid,
    pub(crate) organization_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) project_id: Option<Uuid>,
    pub(crate) user_id: Uuid,
    pub(crate) name: String,
    pub(crate) status: AgentStatus,
    pub(crate) container_id: Option<String>,
    pub(crate) cli_tool: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) provider: Option<String>,
}

#[derive(Clone)]
pub(crate) struct McpAgentRepository {
    pool: PgPool,
}

impl McpAgentRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn resolve_project_context(
        &self,
        project_id: Option<Uuid>,
        org_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> AppResult<McpProjectRuntimeContextRow> {
        let (organization_id, workspace_id) = match project_id {
            Some(project_id) => {
                let (project_org_id, workspace_id) = self.project_workspace(project_id).await?;
                if let Some(org_id) = org_id
                    && org_id != project_org_id
                {
                    return Err(AgentRepositoryPolicy::project_not_found(project_id));
                }
                (project_org_id, workspace_id)
            }
            None => {
                let organization_id = org_id.ok_or_else(AgentRepositoryPolicy::tenant_context_required)?;
                let workspace_id = self.default_workspace_for_org(organization_id).await?;
                (organization_id, workspace_id)
            }
        };

        let user_id = match user_id {
            Some(user_id) => user_id,
            None => self.default_member_for_org(organization_id).await?,
        };

        Ok(McpProjectRuntimeContextRow { project_id, organization_id, workspace_id, user_id })
    }

    pub(crate) async fn insert_agent(&self, record: McpAgentInsertRecord) -> AppResult<()> {
        let runtime_kind =
            AgentCreateRuntimePolicy::for_mcp_insert(record.cli_tool.as_deref(), record.container_id.as_deref())?;
        sqlx::query(
            r#"INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, status, container_id, cli_tool, model, provider, runtime_kind)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        )
        .bind(record.agent_id)
        .bind(record.organization_id)
        .bind(record.workspace_id)
        .bind(record.project_id)
        .bind(record.user_id)
        .bind(record.name)
        .bind(record.status)
        .bind(record.container_id)
        .bind(record.cli_tool)
        .bind(record.model)
        .bind(record.provider)
        .bind(runtime_kind.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn get_agent(&self, agent_id: Uuid) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AgentRepositoryPolicy::agent_uuid_not_found(agent_id))
    }

    pub(crate) async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()> {
        let result = sqlx::query("UPDATE agents SET status = $2, updated_at = NOW() WHERE id = $1")
            .bind(agent_id)
            .bind(status)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AgentRepositoryPolicy::agent_uuid_not_found(agent_id));
        }
        Ok(())
    }

    pub(crate) async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM agents WHERE id = $1").bind(agent_id).execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(AgentRepositoryPolicy::agent_uuid_not_found(agent_id));
        }
        Ok(())
    }

    async fn project_workspace(&self, project_id: Uuid) -> AppResult<(Uuid, Uuid)> {
        sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"SELECT organization_id, workspace_id
                 FROM projects
                WHERE id = $1
                  AND deleted_at IS NULL"#,
        )
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::project_not_found(project_id))
    }

    async fn default_workspace_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
        if let Some(workspace_id) = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id
                 FROM workspaces
                WHERE organization_id = $1
                  AND deleted_at IS NULL
                ORDER BY created_at ASC
                LIMIT 1"#,
        )
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(workspace_id);
        }

        sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO workspaces (organization_id, name)
               VALUES ($1, 'Default Workspace')
               RETURNING id"#,
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn default_member_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT user_id
                 FROM organization_members
                WHERE organization_id = $1
                ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, created_at ASC
                LIMIT 1"#,
        )
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::organization_member_not_found(org_id))
    }
}
