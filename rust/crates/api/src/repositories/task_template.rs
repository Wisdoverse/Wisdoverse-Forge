//! Task template repository — org-scoped queries for reusable task briefs.

use agentforge_core::{AppResult, TenantScope, UserId};
use agentforge_db::entities::TaskTemplate;
use sqlx::PgPool;
use uuid::Uuid;

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
    pub async fn list(&self, scope: &TenantScope, project_id: Option<Uuid>) -> AppResult<Vec<TaskTemplate>> {
        let rows = sqlx::query_as::<_, TaskTemplate>(
            r#"SELECT * FROM task_templates
               WHERE organization_id = $1
                 AND ($2::uuid IS NULL OR project_id = $2 OR project_id IS NULL)
               ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
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
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
        .fetch_one(&self.pool)
        .await
        .map_err(agentforge_core::AppError::from)?;
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

    /// Delete one template by id, org-scoped. Returns whether a row was removed.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<bool> {
        let removed = sqlx::query(r#"DELETE FROM task_templates WHERE id = $1 AND organization_id = $2"#)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(removed.rows_affected() > 0)
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
        }

        let repo = TaskTemplateRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org, other_user);

        let created = repo
            .create(&scope, "Release", "Cut a release", "Steps...", "high", true, None, UserId::from(user_id))
            .await
            .expect("create");
        repo.create(&other_scope, "Other", "Other title", "", "normal", false, None, UserId::from(other_user))
            .await
            .expect("create other");

        let listed = repo.list(&scope, None).await.expect("list");
        assert_eq!(listed.len(), 1, "other org templates are invisible");
        assert_eq!(listed[0].name, "Release");
        // Project filter still returns team-wide (NULL project) templates.
        let filtered = repo.list(&scope, Some(Uuid::new_v4())).await.expect("filtered");
        assert_eq!(filtered.len(), 1, "team-wide templates remain visible for any project");
        assert_eq!(listed[0].priority, "high");
        assert!(listed[0].requires_approval);

        let found = repo.find_by_id(&scope, created.id).await.expect("find").expect("template");
        assert_eq!(found.title, "Cut a release");
        assert!(repo.find_by_id(&other_scope, created.id).await.expect("find other").is_none());

        assert!(repo.delete(&scope, created.id).await.expect("delete"));
        assert!(!repo.delete(&scope, created.id).await.expect("delete again"));
    }
}
