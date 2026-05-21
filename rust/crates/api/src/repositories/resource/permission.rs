//! Resource permission checks for organization, team, and project management.

use agentforge_core::{AppResult, ErrorKind, ProjectId, ScopedRead, TeamId, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for resource permission predicates.
pub struct ResourcePermissionRepository {
    pool: PgPool,
}

impl ResourcePermissionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn validated_read_scope(&self, scope: &TenantScope) -> AppResult<ScopedRead> {
        let Some(workspace_id) = scope.workspace_id() else {
            return Ok(ScopedRead::from_validated_memberships(
                scope.org_id(),
                scope.user_id(),
                std::iter::empty(),
                std::iter::empty(),
                std::iter::empty(),
            ));
        };

        let workspace_exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM workspaces
                    WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_one(&self.pool)
        .await?;
        if !workspace_exists {
            return Err(ErrorKind::NotFound(format!("workspace {workspace_id}")).into());
        }

        let team_ids = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT tm.team_id
                 FROM team_members tm
                 JOIN teams t ON t.id = tm.team_id
                WHERE t.organization_id = $1
                  AND t.deleted_at IS NULL
                  AND tm.user_id = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(TeamId::from)
        .collect::<Vec<_>>();

        let project_ids = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT DISTINCT p.id
                 FROM projects p
                WHERE p.organization_id = $1
                  AND p.workspace_id = $2
                  AND p.deleted_at IS NULL
                  AND (
                      EXISTS (
                          SELECT 1 FROM project_members pm
                           WHERE pm.project_id = p.id AND pm.user_id = $3
                      )
                      OR EXISTS (
                          SELECT 1 FROM team_members tm
                           WHERE tm.team_id = p.team_id AND tm.user_id = $3
                      )
                  )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(ProjectId::from)
        .collect::<Vec<_>>();

        Ok(ScopedRead::from_validated_memberships(
            scope.org_id(),
            scope.user_id(),
            [workspace_id],
            team_ids,
            project_ids,
        ))
    }

    /// Organization owners/admins can manage all teams and projects.
    pub async fn can_manage_org(&self, scope: &TenantScope) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM organization_members om
                    WHERE om.organization_id = $1
                      AND om.user_id = $2
                      AND om.role IN ('owner', 'admin')
               )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Team owner/admin can manage that team; org owner/admin can manage every team.
    pub async fn can_manage_team(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM teams t
                    WHERE t.id = $3
                      AND t.organization_id = $1
                      AND t.deleted_at IS NULL
                      AND (
                           EXISTS (
                             SELECT 1
                               FROM organization_members om
                              WHERE om.organization_id = $1
                                AND om.user_id = $2
                                AND om.role IN ('owner', 'admin')
                           )
                           OR EXISTS (
                             SELECT 1
                               FROM team_members tm
                              WHERE tm.team_id = t.id
                                AND tm.user_id = $2
                                AND tm.role IN ('owner', 'admin')
                           )
                      )
               )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(team_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Project creation is allowed for org owner/admin or team owner/admin/maintainer.
    pub async fn can_create_project_in_team(&self, scope: &TenantScope, team_id: TeamId) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM teams t
                    WHERE t.id = $3
                      AND t.organization_id = $1
                      AND t.deleted_at IS NULL
                      AND (
                           EXISTS (
                             SELECT 1
                               FROM organization_members om
                              WHERE om.organization_id = $1
                                AND om.user_id = $2
                                AND om.role IN ('owner', 'admin')
                           )
                           OR EXISTS (
                             SELECT 1
                               FROM team_members tm
                              WHERE tm.team_id = t.id
                                AND tm.user_id = $2
                                AND tm.role IN ('owner', 'admin', 'maintainer')
                           )
                      )
               )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(team_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Project owner/admin/maintainer, parent team owner/admin/maintainer, or org owner/admin can manage a project.
    pub async fn can_manage_project(&self, scope: &TenantScope, project_id: ProjectId) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM projects p
                    WHERE p.id = $3
                      AND p.organization_id = $1
                      AND p.deleted_at IS NULL
                      AND (
                           EXISTS (
                             SELECT 1
                               FROM organization_members om
                              WHERE om.organization_id = $1
                                AND om.user_id = $2
                                AND om.role IN ('owner', 'admin')
                           )
                           OR EXISTS (
                             SELECT 1
                               FROM team_members tm
                              WHERE tm.team_id = p.team_id
                                AND tm.user_id = $2
                                AND tm.role IN ('owner', 'admin', 'maintainer')
                           )
                           OR EXISTS (
                             SELECT 1
                               FROM project_members pm
                              WHERE pm.project_id = p.id
                                AND pm.user_id = $2
                                AND pm.role IN ('owner', 'admin', 'maintainer')
                           )
                      )
               )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(project_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
