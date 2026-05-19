//! Team and project member repository.

use agentforge_core::{AppResult, ErrorKind, ProjectId, TeamId, TenantScope};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMember {
    pub user_id: Uuid,
    pub email: String,
    pub username: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

pub struct ResourceMemberRepository {
    pool: PgPool,
}

impl ResourceMemberRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_team_members(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
    ) -> AppResult<Vec<ResourceMember>> {
        sqlx::query_as::<_, ResourceMember>(
            r#"SELECT
                   tm.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   tm.role,
                   tm.created_at AS joined_at
               FROM team_members tm
               JOIN teams t
                 ON t.id = tm.team_id
               JOIN users u
                 ON u.id = tm.user_id
               JOIN organization_members om
                 ON om.organization_id = t.organization_id
                AND om.user_id = u.id
              WHERE t.id = $1
                AND t.organization_id = $2
                AND t.organization_id = $3
                AND t.deleted_at IS NULL
                AND u.deleted_at IS NULL
              ORDER BY tm.created_at ASC"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn add_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        let member = sqlx::query_as::<_, ResourceMember>(
            r#"WITH upserted AS (
                   INSERT INTO team_members (team_id, user_id, role)
                   SELECT t.id, om.user_id, $5
                     FROM teams t
                     JOIN organization_members om
                       ON om.organization_id = t.organization_id
                      AND om.user_id = $4
                    WHERE t.id = $1
                      AND t.organization_id = $2
                      AND t.organization_id = $3
                      AND t.deleted_at IS NULL
                   ON CONFLICT (team_id, user_id)
                   DO UPDATE SET role = EXCLUDED.role
                   RETURNING user_id, role, created_at
               )
               SELECT
                   upserted.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   upserted.role,
                   upserted.created_at AS joined_at
                 FROM upserted
                 JOIN users u
                   ON u.id = upserted.user_id
                WHERE u.deleted_at IS NULL"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;

        member.ok_or_else(|| ErrorKind::NotFound(format!("team {team_id} or user {user_id}")).into())
    }

    pub async fn update_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        let member = sqlx::query_as::<_, ResourceMember>(
            r#"WITH updated AS (
                   UPDATE team_members tm
                      SET role = $5
                     FROM teams t
                    WHERE tm.team_id = t.id
                      AND tm.team_id = $1
                      AND t.organization_id = $2
                      AND t.organization_id = $3
                      AND tm.user_id = $4
                      AND t.deleted_at IS NULL
                    RETURNING tm.user_id, tm.role, tm.created_at
               )
               SELECT
                   updated.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   updated.role,
                   updated.created_at AS joined_at
                 FROM updated
                 JOIN users u
                   ON u.id = updated.user_id
                WHERE u.deleted_at IS NULL"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;

        member.ok_or_else(|| ErrorKind::NotFound(format!("team member {user_id}")).into())
    }

    pub async fn remove_team_member(
        &self,
        scope: &TenantScope,
        org_id: Uuid,
        team_id: TeamId,
        user_id: Uuid,
    ) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM team_members tm
               USING teams t
               WHERE tm.team_id = t.id
                 AND tm.team_id = $1
                 AND t.organization_id = $2
                 AND t.organization_id = $3
                 AND tm.user_id = $4
                 AND t.deleted_at IS NULL"#,
        )
        .bind(team_id.as_uuid())
        .bind(org_id)
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("team member {user_id}")).into());
        }
        Ok(())
    }

    pub async fn list_project_members(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
    ) -> AppResult<Vec<ResourceMember>> {
        sqlx::query_as::<_, ResourceMember>(
            r#"SELECT
                   pm.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   pm.role,
                   pm.created_at AS joined_at
               FROM project_members pm
               JOIN projects p
                 ON p.id = pm.project_id
               JOIN users u
                 ON u.id = pm.user_id
               JOIN organization_members om
                 ON om.organization_id = p.organization_id
                AND om.user_id = u.id
              WHERE p.id = $1
                AND p.organization_id = $2
                AND p.deleted_at IS NULL
                AND u.deleted_at IS NULL
              ORDER BY pm.created_at ASC"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn add_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        let member = sqlx::query_as::<_, ResourceMember>(
            r#"WITH upserted AS (
                   INSERT INTO project_members (project_id, user_id, role)
                   SELECT p.id, om.user_id, $4
                     FROM projects p
                     JOIN organization_members om
                       ON om.organization_id = p.organization_id
                      AND om.user_id = $3
                    WHERE p.id = $1
                      AND p.organization_id = $2
                      AND p.deleted_at IS NULL
                   ON CONFLICT (project_id, user_id)
                   DO UPDATE SET role = EXCLUDED.role
                   RETURNING user_id, role, created_at
               )
               SELECT
                   upserted.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   upserted.role,
                   upserted.created_at AS joined_at
                 FROM upserted
                 JOIN users u
                   ON u.id = upserted.user_id
                WHERE u.deleted_at IS NULL"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;

        member.ok_or_else(|| ErrorKind::NotFound(format!("project {project_id} or user {user_id}")).into())
    }

    pub async fn update_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<ResourceMember> {
        let member = sqlx::query_as::<_, ResourceMember>(
            r#"WITH updated AS (
                   UPDATE project_members pm
                      SET role = $4
                     FROM projects p
                    WHERE pm.project_id = p.id
                      AND pm.project_id = $1
                      AND p.organization_id = $2
                      AND pm.user_id = $3
                      AND p.deleted_at IS NULL
                    RETURNING pm.user_id, pm.role, pm.created_at
               )
               SELECT
                   updated.user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   updated.role,
                   updated.created_at AS joined_at
                 FROM updated
                 JOIN users u
                   ON u.id = updated.user_id
                WHERE u.deleted_at IS NULL"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .bind(role)
        .fetch_optional(&self.pool)
        .await?;

        member.ok_or_else(|| ErrorKind::NotFound(format!("project member {user_id}")).into())
    }

    pub async fn remove_project_member(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
        user_id: Uuid,
    ) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM project_members pm
               USING projects p
               WHERE pm.project_id = p.id
                 AND pm.project_id = $1
                 AND p.organization_id = $2
                 AND pm.user_id = $3
                 AND p.deleted_at IS NULL"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("project member {user_id}")).into());
        }
        Ok(())
    }

    pub async fn find_org_user_by_email(&self, scope: &TenantScope, email: &str) -> AppResult<Option<Uuid>> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT u.id
                 FROM users u
                 JOIN organization_members om
                   ON om.user_id = u.id
                WHERE om.organization_id = $1
                  AND lower(u.email) = lower($2)
                  AND u.deleted_at IS NULL
                LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(email.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }
}
