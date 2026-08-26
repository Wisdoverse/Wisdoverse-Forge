//! Task template repository — org-scoped queries for reusable task briefs.

use agentforge_core::{AppResult, TenantScope, UserId};
use agentforge_db::entities::TaskTemplate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::task_template::template_project_invalid;

/// Database access layer for task templates.
pub struct TaskTemplateRepository {
    pool: PgPool,
}

impl TaskTemplateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List all templates for the organization, newest first.
    /// List templates; optional `project_id` filters to one project while
    /// keeping team-wide (NULL) templates included.
    pub async fn list(
        &self,
        scope: &TenantScope,
        project_id: Option<Uuid>,
        readable_project_ids: &[Uuid],
    ) -> AppResult<Vec<TaskTemplate>> {
        let rows = sqlx::query_as::<_, TaskTemplate>(
            r#"SELECT * FROM task_templates
               WHERE organization_id = $1
                 AND (project_id IS NULL OR project_id = ANY($2))
                 AND ($3::uuid IS NULL OR project_id = $3 OR project_id IS NULL)
               ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(readable_project_ids)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Create a template row and return it.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        title: &str,
        description: &str,
        priority: &str,
        requires_approval: bool,
        project_id: Option<Uuid>,
        created_by: UserId,
    ) -> AppResult<TaskTemplate> {
        let row = sqlx::query_as::<_, TaskTemplate>(
            r#"INSERT INTO task_templates
               (organization_id, name, title, description, priority, requires_approval, project_id, created_by)
               SELECT $1, $2, $3, $4, $5, $6, $7, $8
                WHERE $7::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM projects p
                   WHERE p.id = $7 AND p.organization_id = $1 AND p.deleted_at IS NULL
                )
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name.trim())
        .bind(title.trim())
        .bind(description)
        .bind(priority)
        .bind(requires_approval)
        .bind(project_id)
        .bind(created_by.as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(template_project_invalid)?;
        Ok(row)
    }

    /// Find one template by id, org-scoped.
    pub async fn find_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<Option<TaskTemplate>> {
        let row =
            sqlx::query_as::<_, TaskTemplate>(r#"SELECT * FROM task_templates WHERE id = $1 AND organization_id = $2"#)
                .bind(id)
                .bind(scope.org_id().as_uuid())
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    /// Delete and return one template when the live caller may manage it.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<Option<TaskTemplate>> {
        sqlx::query_as::<_, TaskTemplate>(
            r#"DELETE FROM task_templates tt
                WHERE tt.id = $1
                  AND tt.organization_id = $2
                  AND EXISTS (
                    SELECT 1 FROM organization_members om
                     WHERE om.organization_id = tt.organization_id
                       AND om.user_id = $3
                       AND (
                         om.role IN ('owner', 'admin')
                         OR (
                           tt.created_by = $3
                           AND (
                             tt.project_id IS NULL
                             OR EXISTS (
                               SELECT 1 FROM projects p
                                WHERE p.id = tt.project_id
                                  AND p.organization_id = tt.organization_id
                                  AND p.deleted_at IS NULL
                                  AND (
                                    EXISTS (
                                      SELECT 1 FROM project_members pm
                                       WHERE pm.project_id = p.id AND pm.user_id = $3
                                    )
                                    OR EXISTS (
                                      SELECT 1 FROM team_members tm
                                      JOIN teams t ON t.id = tm.team_id
                                       WHERE tm.team_id = p.team_id
                                         AND tm.user_id = $3
                                         AND t.deleted_at IS NULL
                                    )
                                  )
                             )
                           )
                         )
                       )
                  )
               RETURNING tt.*"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn templates_are_org_scoped_and_deletable(pool: sqlx::PgPool) {
        let org_id = Uuid::new_v4();
        let other_org = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let other_project = Uuid::new_v4();
        for (org, user, slug) in [(org_id, user_id, "tpl-org"), (other_org, other_user, "tpl-other")] {
            sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Tpl Org', $2)")
                .bind(org)
                .bind(slug)
                .execute(&pool)
                .await
                .expect("seed org");
            sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
                .bind(user)
                .bind(format!("tpl-{slug}@example.com"))
                .execute(&pool)
                .await
                .expect("seed user");
            sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
                .bind(org)
                .bind(user)
                .execute(&pool)
                .await
                .expect("seed org membership");
        }
        for (org, user, project, slug) in
            [(org_id, user_id, project_id, "tpl-project"), (other_org, other_user, other_project, "other-project")]
        {
            let team = Uuid::new_v4();
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
                .bind(org)
                .execute(&pool)
                .await
                .expect("seed workspace");
            sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Team', 'team')")
                .bind(team)
                .bind(org)
                .execute(&pool)
                .await
                .expect("seed team");
            sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
                .bind(team)
                .bind(user)
                .execute(&pool)
                .await
                .expect("seed team membership");
            sqlx::query("INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug) VALUES ($1, $2, $2, $3, 'Project', $4)")
                .bind(project)
                .bind(org)
                .bind(team)
                .bind(slug)
                .execute(&pool)
                .await
                .expect("seed project");
        }

        let repo = TaskTemplateRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org, other_user);

        let created = repo
            .create(&scope, "Release", "Cut a release", "Steps...", "high", true, None, UserId::from(user_id))
            .await
            .expect("create");
        let project_template = repo
            .create(
                &scope,
                "Project release",
                "Cut project release",
                "",
                "normal",
                false,
                Some(project_id),
                UserId::from(user_id),
            )
            .await
            .expect("create project template");
        assert!(
            repo.create(
                &scope,
                "Foreign",
                "Foreign project",
                "",
                "normal",
                false,
                Some(other_project),
                UserId::from(user_id),
            )
            .await
            .is_err(),
            "a foreign-org project cannot be referenced"
        );
        repo.create(&other_scope, "Other", "Other title", "", "normal", false, None, UserId::from(other_user))
            .await
            .expect("create other");

        let listed = repo.list(&scope, None, &[]).await.expect("list");
        assert_eq!(listed.len(), 1, "other org templates are invisible");
        assert_eq!(listed[0].name, "Release");
        let with_project = repo.list(&scope, None, &[project_id]).await.expect("list readable project");
        assert_eq!(with_project.len(), 2, "readable project templates are included");
        // Project filter still returns team-wide (NULL project) templates.
        let filtered = repo.list(&scope, Some(Uuid::new_v4()), &[]).await.expect("filtered");
        assert_eq!(filtered.len(), 1, "team-wide templates remain visible for any project");
        assert_eq!(listed[0].priority, "high");
        assert!(listed[0].requires_approval);

        let found = repo.find_by_id(&scope, created.id).await.expect("find").expect("template");
        assert_eq!(found.title, "Cut a release");
        assert!(repo.find_by_id(&other_scope, created.id).await.expect("find other").is_none());

        let project_team: Uuid = sqlx::query_scalar("SELECT team_id FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("project team");
        sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(project_team)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("revoke project access");
        assert!(repo.delete(&scope, project_template.id).await.expect("delete revoked project template").is_none());
        assert!(repo.delete(&scope, created.id).await.expect("delete").is_some());
        assert!(repo.delete(&scope, created.id).await.expect("delete again").is_none());
    }
}
