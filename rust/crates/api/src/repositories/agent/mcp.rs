//! Agent aggregate persistence for the internal MCP bridge.

use agentforge_core::{AgentStatus, AppResult};
use agentforge_db::entities::Agent;
use chrono::Utc;
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
    pub(crate) container_image_identity: Option<serde_json::Value>,
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
            Some(user_id) => {
                let is_member: bool = sqlx::query_scalar(
                    r#"SELECT EXISTS (
                           SELECT 1
                             FROM organization_members
                            WHERE organization_id = $1
                              AND user_id = $2
                       )"#,
                )
                .bind(organization_id)
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
                if !is_member {
                    return Err(AgentRepositoryPolicy::organization_member_not_found(organization_id));
                }
                user_id
            }
            None => self.default_member_for_org(organization_id).await?,
        };

        Ok(McpProjectRuntimeContextRow { project_id, organization_id, workspace_id, user_id })
    }

    pub(crate) async fn insert_agent(&self, record: McpAgentInsertRecord) -> AppResult<()> {
        let runtime_kind =
            AgentCreateRuntimePolicy::for_mcp_insert(record.cli_tool.as_deref(), record.container_id.as_deref())?;
        sqlx::query(
            r#"INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, status, container_id, container_image_identity, cli_tool, model, provider, runtime_kind)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        )
        .bind(record.agent_id)
        .bind(record.organization_id)
        .bind(record.workspace_id)
        .bind(record.project_id)
        .bind(record.user_id)
        .bind(record.name)
        .bind(record.status)
        .bind(record.container_id)
        .bind(record.container_image_identity)
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
        let result = sqlx::query(
            "UPDATE agents
             SET status = $2,
                 interactive_lease_expires_at = CASE
                     WHEN $2::agent_status = 'working'
                     THEN NOW() + INTERVAL '60 seconds'
                     ELSE NULL
                 END,
                 interactive_owner_session_id = CASE
                     WHEN $2::agent_status = 'working' THEN NULL
                     ELSE interactive_owner_session_id
                 END,
                 updated_at = CASE
                     WHEN status IS DISTINCT FROM $2::agent_status THEN NOW()
                     ELSE updated_at
                 END
             WHERE id = $1",
        )
        .bind(agent_id)
        .bind(status)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AgentRepositoryPolicy::agent_uuid_not_found(agent_id));
        }
        Ok(())
    }

    pub(crate) async fn begin_agent_work(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
    ) -> AppResult<chrono::DateTime<Utc>> {
        sqlx::query_scalar(
            r#"UPDATE agents
                  SET status = 'working',
                      interactive_lease_expires_at = clock_timestamp() + INTERVAL '60 seconds',
                      interactive_owner_session_id = NULL,
                      updated_at = CASE
                          WHEN status IS DISTINCT FROM 'working'::agent_status THEN NOW()
                          ELSE updated_at
                      END
                WHERE id = $1
                  AND container_id = $2
            RETURNING interactive_lease_expires_at"#,
        )
        .bind(agent_id)
        .bind(expected_container_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_uuid_not_found(agent_id))
    }

    pub(crate) async fn renew_agent_work_lease(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: chrono::DateTime<Utc>,
    ) -> AppResult<Option<chrono::DateTime<Utc>>> {
        Ok(sqlx::query_scalar(
            r#"UPDATE agents
                  SET interactive_lease_expires_at = clock_timestamp() + INTERVAL '60 seconds'
                WHERE id = $1
                  AND container_id = $2
                  AND interactive_lease_expires_at = $3
                  AND interactive_lease_expires_at > clock_timestamp()
            RETURNING interactive_lease_expires_at"#,
        )
        .bind(agent_id)
        .bind(expected_container_id)
        .bind(expected_lease)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn finish_agent_work(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: chrono::DateTime<Utc>,
        status: AgentStatus,
    ) -> AppResult<bool> {
        Ok(sqlx::query_scalar(
            r#"UPDATE agents
                  SET status = $3,
                      interactive_lease_expires_at = NULL,
                      interactive_owner_session_id = NULL,
                      updated_at = NOW()
                WHERE id = $1
                  AND container_id = $2
                  AND interactive_lease_expires_at = $4
                  AND interactive_lease_expires_at > clock_timestamp()
            RETURNING TRUE"#,
        )
        .bind(agent_id)
        .bind(expected_container_id)
        .bind(status)
        .bind(expected_lease)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(false))
    }

    pub(crate) async fn delete_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM agents WHERE id = $1 AND container_id IS NOT DISTINCT FROM $2")
            .bind(agent_id)
            .bind(expected_container_id)
            .execute(&self.pool)
            .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_org(pool: &PgPool, label: &str) -> (Uuid, Uuid) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(label)
            .bind(format!("{label}-{org_id}"))
            .execute(pool)
            .await
            .expect("seed organization");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("{label}-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(org_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed membership");
        (org_id, user_id)
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn supplied_mcp_user_must_belong_to_resolved_organization(pool: PgPool) {
        let (org_a, user_a) = seed_org(&pool, "mcp-a").await;
        let (_org_b, user_b) = seed_org(&pool, "mcp-b").await;
        let repo = McpAgentRepository::new(pool);

        repo.resolve_project_context(None, Some(org_a), Some(user_b))
            .await
            .expect_err("foreign organization member must be rejected");
        let context =
            repo.resolve_project_context(None, Some(org_a), Some(user_a)).await.expect("same-organization member");
        assert_eq!(context.organization_id, org_a);
        assert_eq!(context.user_id, user_a);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn mcp_delete_is_bound_to_the_expected_container(pool: PgPool) {
        let (org_id, user_id) = seed_org(&pool, "mcp-delete").await;
        let agent_id = Uuid::new_v4();
        let image = serde_json::json!({
            "source": "agentforge-agent:claude",
            "imageId": format!("sha256:{}", "d".repeat(64)),
            "versionSource": "not-reported",
            "trust": "host-local"
        });
        sqlx::query(
            "INSERT INTO agents
                (id, organization_id, workspace_id, user_id, runtime_kind, cli_tool, container_id,
                 container_image_identity)
             VALUES ($1, $2, $2, $3, 'container', 'claude', 'container-current', $4)",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .bind(image)
        .execute(&pool)
        .await
        .expect("seed Agent");
        let repo = McpAgentRepository::new(pool.clone());

        repo.delete_agent(agent_id, Some("container-stale"))
            .await
            .expect_err("stale container must not authorize deletion");
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1)")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .unwrap()
        );
        repo.delete_agent(agent_id, Some("container-current")).await.expect("delete current container");
    }
}
