//! Agent workspace persistence helpers.

use agentforge_core::{AppResult, ErrorKind};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::agent_workspace::{
    AgentWorkspacePaths, CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, host_path_for_container_cwd,
    resolve_agent_workspace_paths,
};

const DEFAULT_WORKSPACE_ROOT: &str = "/data/agentforge/workspaces";

pub(crate) fn workspace_root_from_env() -> String {
    std::env::var("AGENTFORGE_WORKSPACE_ROOT").unwrap_or_else(|_| DEFAULT_WORKSPACE_ROOT.to_string())
}

pub async fn resolve_workspace_mount_scope(
    pool: &PgPool,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> AppResult<WorkspaceMountScope> {
    if let Some(project_id) = project_id {
        let row = sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"SELECT organization_id, workspace_id
                 FROM public.projects
                WHERE id = $1
                  AND organization_id = $2
                  AND deleted_at IS NULL"#,
        )
        .bind(project_id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("project {project_id}")))?;

        return WorkspaceMountScope::for_project(row.0, workspace_id, row.1);
    }

    if let Some(workspace_id) = workspace_id {
        ensure_workspace_belongs_to_org(pool, org_id, workspace_id).await?;
        return Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id));
    }

    let workspace_id = default_workspace_for_org(pool, org_id).await?;
    Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id))
}

pub async fn ensure_workspace_belongs_to_org(pool: &PgPool, org_id: Uuid, workspace_id: Uuid) -> AppResult<()> {
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
    .fetch_one(pool)
    .await?;

    if exists { Ok(()) } else { Err(ErrorKind::NotFound(format!("workspace {workspace_id}")).into()) }
}

pub async fn default_workspace_for_org(pool: &PgPool, org_id: Uuid) -> AppResult<Uuid> {
    if let Some(workspace_id) = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id
             FROM public.workspaces
            WHERE organization_id = $1
              AND deleted_at IS NULL
            ORDER BY created_at ASC
            LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
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
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}
