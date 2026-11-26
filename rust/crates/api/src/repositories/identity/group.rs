//! Group repository — tenant-scoped database queries for groups and group members.

use agentforge_core::{AppResult, GroupId, ProjectId, TenantScope};
use agentforge_db::entities::{Group, GroupMember};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::resource::ResourceRepositoryPolicy;

/// Project-scoped group row for the legacy tree-pane projection.
#[derive(Debug, Clone, FromRow)]
pub struct ProjectGroupSummaryRow {
    pub id: Uuid,
    pub name: String,
    pub project_id: Uuid,
}

/// Database access layer for groups.
pub struct GroupRepository {
    pool: PgPool,
}

impl GroupRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Read and hold an active group's project/workspace rows until caller commit.
    pub(crate) async fn lock_active_destination_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        group_id: Uuid,
    ) -> AppResult<Option<Uuid>> {
        Ok(sqlx::query_scalar::<_, Uuid>(
            r#"SELECT g.id FROM groups g
               JOIN projects p ON p.id = g.project_id AND p.organization_id = g.organization_id
               JOIN workspaces w ON w.id = p.workspace_id AND w.organization_id = p.organization_id
               WHERE g.id = $1 AND g.organization_id = $2
                 AND g.deleted_at IS NULL AND p.deleted_at IS NULL AND w.deleted_at IS NULL
               FOR SHARE OF g, p, w"#,
        )
        .bind(group_id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&mut **tx)
        .await?)
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

    /// List project-scoped canonical groups visible to the current user.
    pub async fn list_project_group_summaries(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
    ) -> AppResult<Vec<ProjectGroupSummaryRow>> {
        sqlx::query_as::<_, ProjectGroupSummaryRow>(
            r#"SELECT
                   g.id,
                   g.name,
                   g.project_id
               FROM public.groups g
               JOIN public.projects p
                 ON p.id = g.project_id
               JOIN organization_members om
                 ON om.organization_id = p.organization_id
              WHERE g.project_id = $1
                AND om.user_id = $2
                AND p.organization_id = $3
                AND g.deleted_at IS NULL
                AND p.deleted_at IS NULL
              ORDER BY g.created_at ASC"#,
        )
        .bind(project_id.as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Get a single group by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: GroupId) -> AppResult<Group> {
        sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL")
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ResourceRepositoryPolicy::group_not_found(id))
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
            return Err(ResourceRepositoryPolicy::project_not_found(project_id));
        }

        Ok(())
    }

    /// Test-support query matching the historical project-scoped navigation
    /// contract, where project membership is proven by user organization
    /// membership rather than the active tenant scope.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn list_canonical_for_project_for_test(
        &self,
        user_id: Uuid,
        project_id: Uuid,
    ) -> AppResult<Vec<ProjectGroupSummaryRow>> {
        sqlx::query_as::<_, ProjectGroupSummaryRow>(
            r#"SELECT
                   g.id,
                   g.name,
                   g.project_id
               FROM public.groups g
               JOIN public.projects p
                 ON p.id = g.project_id
               JOIN organization_members om
                 ON om.organization_id = p.organization_id
              WHERE g.project_id = $1
                AND om.user_id = $2
                AND g.deleted_at IS NULL
                AND p.deleted_at IS NULL
              ORDER BY g.created_at ASC"#,
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
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
        .ok_or_else(|| ResourceRepositoryPolicy::group_not_found(id))
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
            return Err(ResourceRepositoryPolicy::group_not_found(id));
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
                ResourceRepositoryPolicy::group_member_already_exists()
            }
            _ => e.into(),
        })
    }

    /// Remove a member from a group.
    pub async fn remove_member(&self, scope: &TenantScope, group_id: GroupId, user_id: Uuid) -> AppResult<()> {
        // Verify group belongs to tenant
        self.find_by_id(scope, group_id).await?;
        self.remove_member_row(scope, group_id, user_id).await
    }

    /// The delete statement itself; it constrains the group's organization even
    /// without the caller's `find_by_id` pre-check.
    async fn remove_member_row(&self, scope: &TenantScope, group_id: GroupId, user_id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            "DELETE FROM group_members WHERE group_id = $1 AND user_id = $2
               AND EXISTS (SELECT 1 FROM groups g WHERE g.id = $1 AND g.organization_id = $3)",
        )
        .bind(group_id.as_uuid())
        .bind(user_id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::group_member_not_found(group_id, user_id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_group(pool: &PgPool) -> (TenantScope, GroupId, Uuid) {
        let (org, user, group) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org)
            .bind(format!("Org {org}"))
            .bind(format!("org-{org}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user)
            .bind(format!("u-{user}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO groups (id, organization_id, name, created_by) VALUES ($1, $2, 'group', $3)")
            .bind(group)
            .bind(org)
            .bind(user)
            .execute(pool)
            .await
            .expect("seed group");
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
            .bind(group)
            .bind(user)
            .execute(pool)
            .await
            .expect("seed group member");
        (crate::test_support::tenant_scope_for_ids(org, user), GroupId::from(group), user)
    }

    async fn member_row(pool: &PgPool, group: GroupId) -> Option<String> {
        sqlx::query_scalar("SELECT to_jsonb(m)::text FROM group_members m WHERE m.group_id = $1")
            .bind(group.as_uuid())
            .fetch_optional(pool)
            .await
            .expect("read group member row")
    }

    fn masked(result: &AppResult<()>, id: Uuid) -> String {
        match result {
            Ok(()) => "Ok".to_string(),
            Err(err) => format!("Err({})", err.to_string().replace(&id.to_string(), "<id>")),
        }
    }

    /// #1324: the member delete constrains the group's organization itself,
    /// beneath `remove_member`'s `find_by_id` pre-check. A foreign group id
    /// returns exactly the unknown-id result and changes no row.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn member_removal_statement_is_constrained_by_organization(pool: PgPool) {
        let (scope_a, a_group, user_a) = seed_group(&pool).await;
        let (scope_b, b_group, _) = seed_group(&pool).await;
        let unknown = GroupId::from(Uuid::new_v4());
        let repo = GroupRepository::new(pool.clone());
        let before = (member_row(&pool, a_group).await, member_row(&pool, b_group).await);

        let foreign = repo.remove_member_row(&scope_b, a_group, user_a).await;
        let missing = repo.remove_member_row(&scope_b, unknown, user_a).await;
        assert!(foreign.is_err(), "foreign member removal must fail");
        assert_eq!(masked(&foreign, a_group.as_uuid()), masked(&missing, unknown.as_uuid()));
        assert_eq!((member_row(&pool, a_group).await, member_row(&pool, b_group).await), before);

        repo.remove_member_row(&scope_a, a_group, user_a).await.expect("own member removal");
        assert_eq!(member_row(&pool, a_group).await, None);
        assert_eq!(member_row(&pool, b_group).await, before.1);
    }
}
