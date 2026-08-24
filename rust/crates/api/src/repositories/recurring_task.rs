//! Recurring task repository — scheduling rows and the due-run claim.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::RecurringTask;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for recurring tasks.
pub struct RecurringTaskRepository {
    pool: PgPool,
}

impl RecurringTaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List the team space schedules, newest first.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<RecurringTask>> {
        let rows = sqlx::query_as::<_, RecurringTask>(
            r#"SELECT * FROM recurring_tasks WHERE organization_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Create a schedule that starts with the next runner tick.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        title: &str,
        description: &str,
        priority: &str,
        requires_approval: bool,
        project_id: Uuid,
        group_id: Uuid,
        cadence_minutes: i32,
    ) -> AppResult<RecurringTask> {
        let row = sqlx::query_as::<_, RecurringTask>(
            r#"INSERT INTO recurring_tasks
               (organization_id, name, title, description, priority, requires_approval,
                project_id, group_id, cadence_minutes, created_by)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name.trim())
        .bind(title.trim())
        .bind(description)
        .bind(priority)
        .bind(requires_approval)
        .bind(project_id)
        .bind(group_id)
        .bind(cadence_minutes)
        .bind(scope.user_id().as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(agentforge_core::AppError::from)?;
        Ok(row)
    }

    /// Find one schedule by id, org-scoped.
    pub async fn find_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<Option<RecurringTask>> {
        let row = sqlx::query_as::<_, RecurringTask>(
            r#"SELECT * FROM recurring_tasks WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Delete one schedule by id, org-scoped.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<bool> {
        let removed = sqlx::query("DELETE FROM recurring_tasks WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(removed.rows_affected() > 0)
    }

    /// Enable or disable a schedule; returns the updated row or None.
    pub async fn set_enabled(&self, scope: &TenantScope, id: Uuid, enabled: bool) -> AppResult<Option<RecurringTask>> {
        let row = sqlx::query_as::<_, RecurringTask>(
            r#"UPDATE recurring_tasks SET enabled = $3, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(enabled)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Claim every due schedule and advance its next run by one cadence.
    /// At-most-once: a crash after claiming skips a run instead of creating
    /// a duplicate; the runner also caps each sweep so a backlog drains
    /// gradually.
    pub async fn claim_due(&self, limit: i64) -> AppResult<Vec<RecurringTask>> {
        let rows = sqlx::query_as::<_, RecurringTask>(
            r#"UPDATE recurring_tasks
               SET next_run_at = NOW() + (cadence_minutes || ' minutes')::interval,
                   updated_at = NOW()
               WHERE id IN (
                   SELECT id FROM recurring_tasks
                    WHERE enabled AND next_run_at <= NOW()
                    ORDER BY next_run_at ASC
                    LIMIT $1
               )
               RETURNING *"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;

    async fn seed_org_and_project(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        team_id: Uuid,
        project_id: Uuid,
        group_id: Uuid,
    ) {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Recur Org', $2)")
            .bind(org_id)
            .bind(format!("recur-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("recur-{org_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Team', 'team')")
            .bind(team_id)
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed team");
        sqlx::query("INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug) VALUES ($1, $2, $2, $3, 'P', 'p')")
            .bind(project_id)
            .bind(org_id)
            .bind(team_id)
            .execute(pool)
            .await
            .expect("seed project");
        sqlx::query(
            "INSERT INTO groups (id, organization_id, name, description, created_by) VALUES ($1, $2, 'G', 'G', $3)",
        )
        .bind(group_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed group");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn recurring_crud_and_due_claim_are_tenant_scoped(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let other_org = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        seed_org_and_project(&pool, org_id, user_id, Uuid::new_v4(), project_id, group_id).await;
        let other_project = Uuid::new_v4();
        let other_group = Uuid::new_v4();
        seed_org_and_project(&pool, other_org, other_user, Uuid::new_v4(), other_project, other_group).await;

        let repo = RecurringTaskRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org, other_user);
        let created = repo
            .create(&scope, "Daily", "Daily summary", "Brief", "normal", false, project_id, group_id, 1_440)
            .await
            .expect("create");
        repo.create(&other_scope, "Other", "Other", "x", "normal", false, other_project, other_group, 60)
            .await
            .expect("create other");

        let listed = repo.list(&scope).await.expect("list");
        assert_eq!(listed.len(), 1, "other org schedules are invisible");

        let disabled = repo.set_enabled(&scope, created.id, false).await.expect("disable").expect("row");
        assert!(!disabled.enabled);
        assert!(repo.delete(&scope, created.id).await.expect("delete"));
        assert_eq!(repo.list(&scope).await.expect("empty").len(), 0);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_due_advances_next_run_and_skips_disabled(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        seed_org_and_project(&pool, org_id, user_id, Uuid::new_v4(), project_id, group_id).await;
        let repo = RecurringTaskRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let due =
            repo.create(&scope, "Due", "Due task", "", "normal", false, project_id, group_id, 60).await.expect("due");
        let later = repo
            .create(&scope, "Later", "Later task", "", "normal", false, project_id, group_id, 60)
            .await
            .expect("later");
        repo.set_enabled(&scope, later.id, false).await.expect("disable later");
        // Move both to past due times; the enabled one is claimed.
        sqlx::query("UPDATE recurring_tasks SET next_run_at = NOW() - interval '10 minutes' WHERE id IN ($1, $2)")
            .bind(due.id)
            .bind(later.id)
            .execute(&pool)
            .await
            .expect("backdate");

        let claimed = repo.claim_due(10).await.expect("claim");
        assert_eq!(claimed.len(), 1, "disabled schedules are skipped");
        assert_eq!(claimed[0].id, due.id);
        let next_run_at: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT next_run_at FROM recurring_tasks WHERE id = $1")
                .bind(due.id)
                .fetch_one(&pool)
                .await
                .expect("next run");
        assert!(next_run_at > chrono::Utc::now(), "next run moved one cadence ahead");
    }
}
