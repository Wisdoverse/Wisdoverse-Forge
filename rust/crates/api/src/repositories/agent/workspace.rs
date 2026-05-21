//! Agent workspace mount-scope persistence helpers.

use agentforge_core::{AppResult, ErrorKind};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentProjectWorkspaceRow {
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Clone)]
pub struct AgentWorkspaceRepository {
    pool: PgPool,
}

impl AgentWorkspaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn project_workspace(&self, org_id: Uuid, project_id: Uuid) -> AppResult<AgentProjectWorkspaceRow> {
        let row = sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"SELECT organization_id, workspace_id
                 FROM public.projects
                WHERE id = $1
                  AND organization_id = $2
                  AND deleted_at IS NULL"#,
        )
        .bind(project_id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("project {project_id}")))?;

        Ok(AgentProjectWorkspaceRow { organization_id: row.0, workspace_id: row.1 })
    }

    pub async fn ensure_workspace_belongs_to_org(&self, org_id: Uuid, workspace_id: Uuid) -> AppResult<()> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                 SELECT 1
                   FROM public.workspaces
                  WHERE id = $1
                    AND organization_id = $2
                    AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(&self.pool)
        .await?;

        if exists { Ok(()) } else { Err(ErrorKind::NotFound(format!("workspace {workspace_id}")).into()) }
    }

    pub async fn default_workspace_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
        if let Some(workspace_id) = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id
                 FROM public.workspaces
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
            r#"INSERT INTO public.workspaces (organization_id, name)
               VALUES ($1, 'Default Workspace')
               RETURNING id"#,
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
