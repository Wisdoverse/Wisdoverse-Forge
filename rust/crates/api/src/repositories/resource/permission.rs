//! Resource permission checks for organization, team, and project management.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope};
use sqlx::PgPool;

/// Database access layer for resource permission predicates.
pub struct ResourcePermissionRepository {
    pool: PgPool,
}

impl ResourcePermissionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
