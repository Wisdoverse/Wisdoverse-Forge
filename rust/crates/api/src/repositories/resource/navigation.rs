//! Navigation read/write repository for the frontend tree-pane contract.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::resource::{
    NavigationProjectUpdateDraft, NavigationTeamCreateDraft, NavigationTeamUpdateDraft, ResourceRepositoryPolicy,
};

#[derive(Debug, Clone, FromRow)]
pub(crate) struct LegacyOrgRow {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) plan: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct LegacyTeamRow {
    pub(crate) id: Uuid,
    pub(crate) org_id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) visibility: String,
    pub(crate) description: String,
    pub(crate) can_manage: bool,
    pub(crate) can_delete: bool,
    pub(crate) can_create_project: bool,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct LegacyProjectRow {
    pub(crate) id: Uuid,
    pub(crate) team_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) color: String,
    pub(crate) description: String,
    pub(crate) can_manage: bool,
    pub(crate) can_delete: bool,
    /// Denormalized `projects.clone_status` summary, so the tree pane can render a
    /// clone badge without a per-project attempt read. The per-attempt detail
    /// (`CloneSummary`) is attached separately by the service.
    pub(crate) clone_status: String,
}

pub struct LegacyNavigationRepository {
    pool: PgPool,
}

impl LegacyNavigationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all organizations visible in the user's navigation switcher.
    pub(crate) async fn list_orgs(&self, scope: &TenantScope) -> AppResult<Vec<LegacyOrgRow>> {
        sqlx::query_as::<_, LegacyOrgRow>(
            r#"SELECT
                   o.id,
                   o.name,
                   o.slug,
                   COALESCE(o.plan, 'free') AS plan,
                   om.role
               FROM organizations o
               JOIN organization_members om
                 ON om.organization_id = o.id
               JOIN users u
                 ON u.id = om.user_id
              WHERE om.user_id = $1
                AND o.deleted_at IS NULL
              ORDER BY
                (o.email_domain IS DISTINCT FROM lower(split_part(u.email, '@', 2))),
                (o.email_domain IS NULL),
                om.created_at ASC,
                o.created_at ASC"#,
        )
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn fetch_org_for_user(&self, scope: &TenantScope, org_id: Uuid) -> AppResult<LegacyOrgRow> {
        sqlx::query_as::<_, LegacyOrgRow>(
            r#"SELECT
                   o.id,
                   o.name,
                   o.slug,
                   COALESCE(o.plan, 'free') AS plan,
                   om.role
               FROM organizations o
               JOIN organization_members om
                 ON om.organization_id = o.id
              WHERE o.id = $1
                AND om.user_id = $2
                AND o.deleted_at IS NULL
              LIMIT 1"#,
        )
        .bind(org_id)
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::organization_uuid_not_found(org_id))
    }

    pub(crate) async fn list_teams(&self, scope: &TenantScope, org_id: Uuid) -> AppResult<Vec<LegacyTeamRow>> {
        sqlx::query_as::<_, LegacyTeamRow>(
            r#"SELECT
                   t.id,
                   t.organization_id        AS org_id,
                   t.name,
                   t.slug,
                   COALESCE(t.visibility, 'private') AS visibility,
                   COALESCE(t.description, '')       AS description,
                   (
                       om.role IN ('owner', 'admin')
                       OR EXISTS (
                           SELECT 1
                             FROM public.team_members tm
                            WHERE tm.team_id = t.id
                              AND tm.user_id = $2
                              AND tm.role IN ('owner', 'admin')
                       )
                   ) AS can_manage,
                   (
                       om.role IN ('owner', 'admin')
                       OR EXISTS (
                           SELECT 1
                             FROM public.team_members tm
                            WHERE tm.team_id = t.id
                              AND tm.user_id = $2
                              AND tm.role IN ('owner', 'admin')
                       )
                   ) AS can_delete,
                   (
                       om.role IN ('owner', 'admin')
                       OR EXISTS (
                           SELECT 1
                             FROM public.team_members tm
                            WHERE tm.team_id = t.id
                              AND tm.user_id = $2
                              AND tm.role IN ('owner', 'admin', 'maintainer')
                       )
                   ) AS can_create_project
               FROM public.teams t
               JOIN organization_members om
                 ON om.organization_id = t.organization_id
              WHERE t.organization_id = $1
                AND t.organization_id = $3
                AND om.user_id = $2
                AND t.deleted_at IS NULL
              ORDER BY t.created_at ASC"#,
        )
        .bind(org_id)
        .bind(scope.user_id().as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn insert_team(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        draft: NavigationTeamCreateDraft,
    ) -> AppResult<LegacyTeamRow> {
        sqlx::query_as::<_, LegacyTeamRow>(
            r#"INSERT INTO public.teams (organization_id, name, slug, visibility, description)
               SELECT o.id, $4, $5, COALESCE($6::text, 'private'), COALESCE($7::text, '')
                 FROM public.organizations o
                 JOIN organization_members om
                   ON om.organization_id = o.id
                WHERE o.id = $1
                  AND o.id = $2
                  AND om.user_id = $3
                  AND o.deleted_at IS NULL
                RETURNING
                  id,
                  organization_id AS org_id,
                  name,
                  slug,
                  COALESCE(visibility, 'private') AS visibility,
                  COALESCE(description, '')       AS description,
                  TRUE AS can_manage,
                  TRUE AS can_delete,
                  TRUE AS can_create_project"#,
        )
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(draft.name)
        .bind(draft.slug)
        .bind(draft.visibility)
        .bind(draft.description)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::organization_uuid_not_found(org_id))
    }

    pub(crate) async fn update_team(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        draft: NavigationTeamUpdateDraft,
    ) -> AppResult<LegacyTeamRow> {
        sqlx::query_as::<_, LegacyTeamRow>(
            r#"UPDATE public.teams t
                  SET name = COALESCE($5, t.name),
                      slug = COALESCE($6, t.slug),
                      visibility = COALESCE($7, t.visibility),
                      description = COALESCE($8, t.description),
                      updated_at = NOW()
                 FROM organization_members om
                WHERE t.id = $1
                  AND t.organization_id = $2
                  AND t.organization_id = $3
                  AND om.organization_id = t.organization_id
                  AND om.user_id = $4
                  AND t.deleted_at IS NULL
                RETURNING
                  t.id,
                  t.organization_id AS org_id,
                  t.name,
                  t.slug,
                  COALESCE(t.visibility, 'private') AS visibility,
                  COALESCE(t.description, '')       AS description,
                  TRUE AS can_manage,
                  TRUE AS can_delete,
                  TRUE AS can_create_project"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(draft.name)
        .bind(draft.slug)
        .bind(draft.visibility)
        .bind(draft.description)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::team_not_found(team_id))
    }

    pub(crate) async fn delete_team(&self, scope: &TenantScope, org_id: Uuid, team_id: TeamId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE public.teams t
                  SET deleted_at = NOW(),
                      updated_at = NOW()
                 FROM organization_members om
                WHERE t.id = $1
                  AND t.organization_id = $2
                  AND t.organization_id = $3
                  AND om.organization_id = t.organization_id
                  AND om.user_id = $4
                  AND t.deleted_at IS NULL"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::team_not_found(team_id));
        }
        Ok(())
    }

    pub(crate) async fn list_projects(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<Vec<LegacyProjectRow>> {
        sqlx::query_as::<_, LegacyProjectRow>(
            r#"SELECT
                   p.id,
                   p.workspace_id,
                   p.team_id,
                   p.name,
                   p.slug,
                   COALESCE(p.color, '#007AFF')  AS color,
                   COALESCE(p.description, '')   AS description,
                   p.clone_status,
                   (
                       om.role IN ('owner', 'admin')
                       OR EXISTS (
                           SELECT 1
                             FROM public.team_members tm
                            WHERE tm.team_id = t.id
                              AND tm.user_id = $2
                              AND tm.role IN ('owner', 'admin', 'maintainer')
                       )
                       OR EXISTS (
                           SELECT 1
                             FROM public.project_members pm
                            WHERE pm.project_id = p.id
                              AND pm.user_id = $2
                              AND pm.role IN ('owner', 'admin', 'maintainer')
                       )
                   ) AS can_manage,
                   (
                       om.role IN ('owner', 'admin')
                       OR EXISTS (
                           SELECT 1
                             FROM public.team_members tm
                            WHERE tm.team_id = t.id
                              AND tm.user_id = $2
                              AND tm.role IN ('owner', 'admin', 'maintainer')
                       )
                       OR EXISTS (
                           SELECT 1
                             FROM public.project_members pm
                            WHERE pm.project_id = p.id
                              AND pm.user_id = $2
                              AND pm.role IN ('owner', 'admin', 'maintainer')
                       )
                   ) AS can_delete
               FROM public.projects p
               JOIN public.teams t
                 ON t.id = p.team_id
               JOIN organization_members om
                 ON om.organization_id = t.organization_id
              WHERE p.team_id = $1
                AND t.organization_id = $3
                AND om.user_id = $2
                AND p.deleted_at IS NULL
                AND t.deleted_at IS NULL
              ORDER BY p.created_at ASC"#,
        )
        .bind(team_id.as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn find_project_parent_org(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<Uuid> {
        sqlx::query_scalar(
            r#"SELECT t.organization_id
                 FROM public.teams t
                 JOIN organization_members om
                   ON om.organization_id = t.organization_id
                WHERE t.id = $1
                  AND t.organization_id = $2
                  AND om.user_id = $3
                  AND t.deleted_at IS NULL"#,
        )
        .bind(team_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::team_not_found(team_id))
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) async fn resolve_team_org_for_test(&self, user_id: Uuid, team_id: Uuid) -> AppResult<Uuid> {
        sqlx::query_scalar(
            r#"SELECT t.organization_id
                 FROM public.teams t
                 JOIN organization_members om
                   ON om.organization_id = t.organization_id
                WHERE t.id = $1
                  AND om.user_id = $2
                  AND t.deleted_at IS NULL"#,
        )
        .bind(team_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::team_uuid_not_found(team_id))
    }

    pub(crate) async fn default_workspace_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
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

    pub(crate) async fn update_project(
        &self,
        scope: &TenantScope,
        team_id: TeamId,
        project_id: ProjectId,
        draft: NavigationProjectUpdateDraft,
    ) -> AppResult<LegacyProjectRow> {
        sqlx::query_as::<_, LegacyProjectRow>(
            r#"UPDATE public.projects p
                  SET name = COALESCE($5, p.name),
                      slug = COALESCE($6, p.slug),
                      color = COALESCE($7, p.color),
                      description = COALESCE($8, p.description),
                      updated_at = NOW()
                 FROM public.teams t
                 JOIN organization_members om
                   ON om.organization_id = t.organization_id
                WHERE p.id = $1
                  AND p.team_id = $2
                  AND p.team_id = t.id
                  AND t.organization_id = $3
                  AND om.user_id = $4
                  AND p.deleted_at IS NULL
                  AND t.deleted_at IS NULL
                RETURNING
                  p.id,
                  p.workspace_id,
                  p.team_id,
                  p.name,
                  p.slug,
                  COALESCE(p.color, '#007AFF') AS color,
                  COALESCE(p.description, '')  AS description,
                  p.clone_status,
                  TRUE AS can_manage,
                  TRUE AS can_delete"#,
        )
        .bind(project_id.as_uuid())
        .bind(team_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(draft.name)
        .bind(draft.slug)
        .bind(draft.color)
        .bind(draft.description)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::project_not_found(project_id))
    }

    pub(crate) async fn delete_project(
        &self,
        scope: &TenantScope,
        team_id: TeamId,
        project_id: ProjectId,
    ) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE public.projects p
                  SET deleted_at = NOW(),
                      updated_at = NOW()
                 FROM public.teams t
                 JOIN organization_members om
                   ON om.organization_id = t.organization_id
                WHERE p.id = $1
                  AND p.team_id = $2
                  AND p.team_id = t.id
                  AND t.organization_id = $3
                  AND om.user_id = $4
                  AND p.deleted_at IS NULL
                  AND t.deleted_at IS NULL"#,
        )
        .bind(project_id.as_uuid())
        .bind(team_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::project_not_found(project_id));
        }
        Ok(())
    }
}
