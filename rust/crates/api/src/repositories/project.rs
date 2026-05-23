//! Project repository — tenant-scoped database queries for projects.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::Project;
use sqlx::PgPool;

use crate::domain::resource::ResourceRepositoryPolicy;

/// Database access layer for projects.
pub struct ProjectRepository {
    pool: PgPool,
}

impl ProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List projects for the current tenant, with optional workspace filter.
    pub async fn list(
        &self,
        scope: &TenantScope,
        workspace_id: Option<WorkspaceId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Project>> {
        let projects = match workspace_id {
            Some(ws_id) => {
                sqlx::query_as::<_, Project>(
                    r#"SELECT * FROM projects
                       WHERE organization_id = $1 AND workspace_id = $2 AND deleted_at IS NULL
                       ORDER BY created_at DESC
                       LIMIT $3 OFFSET $4"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(ws_id.as_uuid())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Project>(
                    r#"SELECT * FROM projects
                       WHERE organization_id = $1 AND deleted_at IS NULL
                       ORDER BY created_at DESC
                       LIMIT $2 OFFSET $3"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(projects)
    }

    /// Get a single project by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: ProjectId) -> AppResult<Project> {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::project_not_found(id))
    }

    /// Create a new project. Persists the domain-resolved `slug` and
    /// `team_id` so migration 026's NOT NULL constraints hold. When the
    /// caller does not supply `team_id`, defaults to the org's oldest
    /// surviving team — matching the migration's backfill rule so existing
    /// rows and new rows have a consistent resolution story. Returns
    /// `Validation` if the org has zero teams; the caller must create one
    /// first.
    pub async fn create(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        team_id: Option<TeamId>,
        name: &str,
        slug: &str,
        repository_url: Option<&str>,
    ) -> AppResult<Project> {
        let resolved_team_id = match team_id {
            Some(id) => id.as_uuid(),
            None => self.default_team_for_org(scope).await?,
        };
        sqlx::query_as::<_, Project>(
            r#"INSERT INTO projects (organization_id, workspace_id, team_id, name, slug, repository_url)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(resolved_team_id)
        .bind(name)
        .bind(slug)
        .bind(repository_url)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Returns the org's oldest surviving team id, or a `Validation`
    /// error if the org has no teams. Used as the default parent when
    /// `create` is called without an explicit `team_id`.
    async fn default_team_for_org(&self, scope: &TenantScope) -> AppResult<uuid::Uuid> {
        let row: Option<(uuid::Uuid,)> = sqlx::query_as(
            r#"SELECT id FROM public.teams
               WHERE organization_id = $1 AND deleted_at IS NULL
               ORDER BY created_at ASC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.0).ok_or_else(ResourceRepositoryPolicy::default_project_team_required)
    }

    /// Update a project (tenant-scoped).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: ProjectId,
        name: Option<&str>,
        repository_url: Option<Option<&str>>,
    ) -> AppResult<Project> {
        // Build update dynamically based on provided fields.
        // For simplicity, we fetch then update only changed fields.
        let existing = self.find_by_id(scope, id).await?;

        let new_name = name.unwrap_or(&existing.name);
        let new_url = match repository_url {
            Some(url) => url,
            None => existing.repository_url.as_deref(),
        };

        sqlx::query_as::<_, Project>(
            r#"UPDATE projects SET name = $3, repository_url = $4, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(new_name)
        .bind(new_url)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::project_not_found(id))
    }

    /// Soft-delete a project (set deleted_at).
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE projects SET deleted_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::project_not_found(id));
        }
        Ok(())
    }
}
