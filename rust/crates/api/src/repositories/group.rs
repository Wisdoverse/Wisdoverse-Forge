//! Group repository — tenant-scoped database queries for groups and group members.

use agentforge_core::{AppResult, ErrorKind, GroupId, ProjectId, TenantScope};
use agentforge_db::entities::{Group, GroupMember};
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for groups.
pub struct GroupRepository {
    pool: PgPool,
}

impl GroupRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List groups for the current tenant, ordered by most recent first.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Group>> {
        let groups = sqlx::query_as::<_, Group>(
            r#"SELECT * FROM groups
               WHERE organization_id = $1 AND deleted_at IS NULL
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(groups)
    }

    /// Get a single group by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: GroupId) -> AppResult<Group> {
        sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL")
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("group {id}")).into())
    }

    /// Create a new group. When `project_id` is provided, the project must
    /// belong to the current tenant.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        description: Option<&str>,
        project_id: Option<ProjectId>,
    ) -> AppResult<Group> {
        if let Some(project_id) = project_id {
            self.ensure_project_belongs_to_scope(scope, project_id).await?;
        }

        sqlx::query_as::<_, Group>(
            r#"INSERT INTO groups (organization_id, project_id, name, description, created_by)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(project_id.map(|id| id.as_uuid()))
        .bind(name)
        .bind(description)
        .bind(scope.user_id().as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Return the oldest project-scoped group or create a default one.
    pub async fn find_or_create_default_for_project(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
    ) -> AppResult<Group> {
        self.ensure_project_belongs_to_scope(scope, project_id).await?;

        if let Some(group) = sqlx::query_as::<_, Group>(
            r#"SELECT * FROM groups
               WHERE organization_id = $1
                 AND project_id = $2
                 AND deleted_at IS NULL
               ORDER BY created_at ASC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(project_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(group);
        }

        self.create(scope, "Tasks", Some("Default task group for this project."), Some(project_id)).await
    }

    async fn ensure_project_belongs_to_scope(&self, scope: &TenantScope, project_id: ProjectId) -> AppResult<()> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM projects
                    WHERE id = $1
                      AND organization_id = $2
                      AND deleted_at IS NULL
               )"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_one(&self.pool)
        .await?;

        if !exists {
            return Err(ErrorKind::NotFound(format!("project {project_id}")).into());
        }

        Ok(())
    }

    /// Update a group's name and/or description (tenant-scoped).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: GroupId,
        name: Option<&str>,
        description: Option<&str>,
    ) -> AppResult<Group> {
        sqlx::query_as::<_, Group>(
            r#"UPDATE groups SET
                   name = COALESCE($3, name),
                   description = COALESCE($4, description),
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("group {id}")).into())
    }

    /// Soft-delete a group (set deleted_at).
    pub async fn delete(&self, scope: &TenantScope, id: GroupId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE groups SET deleted_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("group {id}")).into());
        }
        Ok(())
    }

    /// List members of a group.
    pub async fn list_members(&self, scope: &TenantScope, group_id: GroupId) -> AppResult<Vec<GroupMember>> {
        // Verify group belongs to tenant
        self.find_by_id(scope, group_id).await?;

        let members = sqlx::query_as::<_, GroupMember>(
            r#"SELECT * FROM group_members
               WHERE group_id = $1
               ORDER BY created_at ASC"#,
        )
        .bind(group_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(members)
    }

    /// Add a member to a group.
    pub async fn add_member(
        &self,
        scope: &TenantScope,
        group_id: GroupId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<GroupMember> {
        // Verify group belongs to tenant
        self.find_by_id(scope, group_id).await?;

        sqlx::query_as::<_, GroupMember>(
            r#"INSERT INTO group_members (group_id, user_id, role)
               VALUES ($1, $2, $3)
               RETURNING *"#,
        )
        .bind(group_id.as_uuid())
        .bind(user_id)
        .bind(role)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                ErrorKind::Conflict("user is already a member of this group".into()).into()
            }
            _ => e.into(),
        })
    }

    /// Remove a member from a group.
    pub async fn remove_member(&self, scope: &TenantScope, group_id: GroupId, user_id: Uuid) -> AppResult<()> {
        // Verify group belongs to tenant
        self.find_by_id(scope, group_id).await?;

        let result = sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
            .bind(group_id.as_uuid())
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("member {user_id} in group {group_id}")).into());
        }
        Ok(())
    }
}
