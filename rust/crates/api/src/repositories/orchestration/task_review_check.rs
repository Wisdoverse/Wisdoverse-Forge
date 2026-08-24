//! Task review check repository — a lightweight per-task human checklist.
//! Rows are review evidence: task-scoped, user-scoped, tenant-scoped.

use agentforge_core::{AppResult, TenantScope, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// One check row projected for the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskReviewCheckRow {
    pub check_key: String,
    pub done: bool,
    pub updated_at: DateTime<Utc>,
}

/// Database access layer for task review checks.
pub struct TaskReviewCheckRepository {
    pool: PgPool,
}

impl TaskReviewCheckRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// All checks for a task by the given user, tenant-scoped.
    pub async fn list_by_task(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        user_id: UserId,
    ) -> AppResult<Vec<TaskReviewCheckRow>> {
        let rows = sqlx::query_as::<_, TaskReviewCheckRow>(
            r#"SELECT check_key, done, updated_at
                 FROM task_review_checks
                WHERE task_id = $1 AND organization_id = $2 AND user_id = $3
                ORDER BY check_key ASC"#,
        )
        .bind(task_id)
        .bind(scope.org_id().as_uuid())
        .bind(user_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Upsert one check (idempotent). The guard keeps cross-tenant task ids a
    /// no-op; returns `None` when the task is missing.
    pub async fn set_check(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        user_id: UserId,
        check_key: &str,
        done: bool,
    ) -> AppResult<Option<TaskReviewCheckRow>> {
        let row = sqlx::query_as::<_, TaskReviewCheckRow>(
            r#"INSERT INTO task_review_checks (id, organization_id, task_id, user_id, check_key, done)
               SELECT gen_random_uuid(), $1, $2, $3, $4, $5
                 WHERE EXISTS (
                     SELECT 1 FROM orchestration_tasks t
                      WHERE t.id = $2 AND t.organization_id = $1
                 )
               ON CONFLICT (task_id, user_id, check_key) DO UPDATE
                  SET done = EXCLUDED.done, updated_at = NOW()
               RETURNING check_key, done, updated_at"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .bind(user_id)
        .bind(check_key)
        .bind(done)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Required gate keys still unchecked by ANY reviewer (a key ticked by
    /// one reviewer counts for the task). Tenant-scoped; empty when no
    /// required keys are configured.
    pub(crate) async fn undone_required_gates(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        required: &[String],
    ) -> AppResult<Vec<String>> {
        if required.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT DISTINCT rc.check_key
                 FROM task_review_checks rc
                WHERE rc.task_id = $1
                  AND rc.organization_id = $2
                  AND rc.done = TRUE
                  AND rc.check_key = ANY($3)"#,
        )
        .bind(task_id)
        .bind(scope.org_id().as_uuid())
        .bind(required)
        .fetch_all(self.pool())
        .await?;
        let done: std::collections::HashSet<String> = rows.into_iter().map(|(key,)| key).collect();
        Ok(required.iter().filter(|key| !done.contains(*key)).cloned().collect())
    }
}

#[cfg(test)]
mod task_review_check_tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn seed(pool: &PgPool, org_id: Uuid, user_id: Uuid, task_id: Uuid) {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Review Org', $2)")
            .bind(org_id)
            .bind(format!("review-org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, 'reviewer@example.com', 'Reviewer')")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
             VALUES ($1, $2, 'Review task', 'completed', 'normal', $3)",
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed task");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn set_and_list_are_tenant_scoped_and_idempotent(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        seed(&pool, org_id, user_id, task_id).await;

        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(Uuid::new_v4(), user_id);
        let repo = TaskReviewCheckRepository::new(pool.clone());

        let ticked = repo
            .set_check(&scope, task_id, scope.user_id(), "result_matches_brief", true)
            .await
            .expect("set")
            .expect("task exists");
        assert!(ticked.done);

        // Idempotent re-set flips the value.
        let unticked = repo
            .set_check(&scope, task_id, scope.user_id(), "result_matches_brief", false)
            .await
            .expect("re-set")
            .expect("task exists");
        assert!(!unticked.done);

        let rows = repo.list_by_task(&scope, task_id, scope.user_id()).await.expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].check_key, "result_matches_brief");

        let cross = repo.set_check(&other_scope, task_id, other_scope.user_id(), "x", true).await.expect("cross");
        assert!(cross.is_none(), "cross-tenant set must be a no-op");
        assert!(repo.list_by_task(&other_scope, task_id, other_scope.user_id()).await.expect("cross list").is_empty());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn undone_required_gates_counts_a_tick_from_any_reviewer(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        seed(&pool, org_id, user_id, task_id).await;
        let repo = TaskReviewCheckRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);

        // Another reviewer ticks one required key; the task then satisfies it.
        let reviewer_2 = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'reviewer2@example.com')")
            .bind(reviewer_2)
            .execute(&pool)
            .await
            .expect("seed reviewer2");
        sqlx::query(
            "INSERT INTO task_review_checks (organization_id, task_id, user_id, check_key, done) VALUES ($1, $2, $3, 'no_secrets', TRUE)",
        )
        .bind(org_id)
        .bind(task_id)
        .bind(reviewer_2)
        .execute(&pool)
        .await
        .expect("seed reviewer2 check");

        let required = vec!["no_secrets".to_string(), "result_matches_brief".to_string()];
        let undone = repo.undone_required_gates(&scope, task_id, &required).await.expect("undone");
        assert_eq!(undone, vec!["result_matches_brief".to_string()]);

        // No required keys configured -> no gate to enforce.
        let undone = repo.undone_required_gates(&scope, task_id, &[]).await.expect("empty");
        assert!(undone.is_empty());
    }
}
