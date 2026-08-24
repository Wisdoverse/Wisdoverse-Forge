//! Task-comment repository — human updates (comments / blocker signals) on
//! orchestration tasks.
//!
//! Comments are first-class records, separate from task runs and lifecycle
//! state. Every query is tenant-scoped: `WHERE organization_id = $N` is
//! mandatory, and inserts only land when the referenced task belongs to the
//! same organization.

use agentforge_core::{AppResult, TenantScope, UserId};
use agentforge_db::entities::TaskComment;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// One comment joined with the author's display name for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskCommentWithAuthorRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub task_id: Uuid,
    pub author_user_id: UserId,
    pub kind: String,
    pub body: String,
    pub author_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Latest human blocker/unblock signal per task (board surfacing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct HumanMarkerRow {
    pub task_id: Uuid,
    pub kind: String,
    pub body: String,
    pub author_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Database access layer for human task updates.
pub struct TaskCommentRepository {
    pool: PgPool,
}

impl TaskCommentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    const SELECT_WITH_AUTHOR: &'static str = r#"SELECT c.id, c.organization_id, c.task_id,
                  c.author_user_id, c.kind, c.body, u.display_name AS author_name,
                  c.created_at, c.updated_at
           FROM task_comments c
           LEFT JOIN users u ON u.id = c.author_user_id"#;

    /// All comments for a task, oldest first.
    pub async fn list_by_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<Vec<TaskCommentWithAuthorRow>> {
        let rows = sqlx::query_as::<_, TaskCommentWithAuthorRow>(
            format!(
                "{} WHERE c.task_id = $1 AND c.organization_id = $2 ORDER BY c.created_at ASC, c.id ASC",
                Self::SELECT_WITH_AUTHOR
            )
            .as_str(),
        )
        .bind(task_id)
        .bind(scope.org_id().as_uuid())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// A single comment with author, tenant-scoped.
    pub async fn find_with_author(
        &self,
        scope: &TenantScope,
        comment_id: Uuid,
    ) -> AppResult<Option<TaskCommentWithAuthorRow>> {
        let row = sqlx::query_as::<_, TaskCommentWithAuthorRow>(
            format!("{} WHERE c.id = $1 AND c.organization_id = $2", Self::SELECT_WITH_AUTHOR).as_str(),
        )
        .bind(comment_id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Insert a comment only when the task exists inside this organization:
    /// an `EXISTS` guard inside the INSERT makes a cross-tenant task id a
    /// no-op instead of a foreign-key violation on the wrong tenant.
    /// Returns `None` when the task is missing.
    pub async fn create(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        author_user_id: UserId,
        kind: &str,
        body: &str,
    ) -> AppResult<Option<TaskCommentWithAuthorRow>> {
        let inserted = sqlx::query_as::<_, TaskComment>(
            r#"INSERT INTO task_comments (id, organization_id, task_id, author_user_id, kind, body)
               SELECT $1, $2, $3, $4, $5, $6
                 WHERE EXISTS (
                     SELECT 1 FROM orchestration_tasks t
                      WHERE t.id = $3 AND t.organization_id = $2
                 )
               RETURNING *"#,
        )
        .bind(Uuid::now_v7())
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .bind(author_user_id)
        .bind(kind)
        .bind(body)
        .fetch_optional(self.pool())
        .await?;

        match inserted {
            Some(comment) => Ok(self.find_with_author(scope, comment.id).await?),
            None => Ok(None),
        }
    }

    /// Delete a single comment (tenant-scoped). Returns `false` when the row
    /// was already gone or belongs to another organization.
    pub async fn delete(&self, scope: &TenantScope, comment_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_as::<_, (Uuid,)>(
            r#"DELETE FROM task_comments
               WHERE id = $1 AND organization_id = $2
               RETURNING id"#,
        )
        .bind(comment_id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(self.pool())
        .await?;
        Ok(result.is_some())
    }

    /// Latest blocker / unblock signal per task, ordered by time; a task with
    /// only ordinary comments is absent. Tenant-scoped on the org axis and
    /// capped at the caller's task list.
    pub async fn latest_marker_by_tasks(
        &self,
        scope: &TenantScope,
        task_ids: &[Uuid],
    ) -> AppResult<Vec<HumanMarkerRow>> {
        if task_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, HumanMarkerRow>(
            r#"SELECT per_task.task_id, per_task.kind, per_task.body, per_task.author_name, per_task.created_at
                 FROM (
                   SELECT DISTINCT ON (c.task_id)
                          c.task_id, c.kind, c.body, c.created_at, u.display_name AS author_name
                     FROM task_comments c
                     LEFT JOIN users u ON u.id = c.author_user_id
                    WHERE c.organization_id = $1
                      AND c.task_id = ANY($2)
                      AND c.kind IN ('blocker', 'unblock')
                    ORDER BY c.task_id, c.created_at DESC, c.id DESC
                 ) per_task
                 ORDER BY per_task.task_id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_ids)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod task_comment_tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;
    use sqlx::PgPool;
    use uuid::Uuid;

    /// Org / user / task fixture for a fresh sqlx test database.
    async fn seed_org_user_task(pool: &PgPool, org_id: Uuid, user_id: Uuid, task_id: Uuid) {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Comment Org', $2)")
            .bind(org_id)
            .bind(format!("comment-org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Dev')")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
             VALUES ($1, $2, 'Comment task', 'backlog', 'normal', $3)",
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed task");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn create_list_and_delete_are_tenant_scoped(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let other_org_id = Uuid::new_v4();
        seed_org_user_task(&pool, org_id, user_id, task_id).await;

        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org_id, user_id);
        let repo = TaskCommentRepository::new(pool.clone());

        let created = repo
            .create(&scope, task_id, scope.user_id(), "comment", "First note")
            .await
            .expect("create in own org")
            .expect("task exists");
        assert_eq!(created.body, "First note");
        assert_eq!(created.author_name.as_deref(), Some("Dev"));

        let list = repo.list_by_task(&scope, task_id).await.expect("list own org");
        assert_eq!(list.len(), 1);

        // Other organization sees nothing and cannot comment on the task.
        let list_cross = repo.list_by_task(&other_scope, task_id).await.expect("list cross");
        assert!(list_cross.is_empty());
        let cross =
            repo.create(&other_scope, task_id, other_scope.user_id(), "comment", "cross").await.expect("create cross");
        assert!(cross.is_none(), "cross-tenant create must be a no-op");

        let deleted = repo.delete(&scope, created.id).await.expect("delete");
        assert!(deleted);
        assert!(!repo.delete(&scope, created.id).await.expect("delete again"), "second delete must return false");
        assert!(repo.list_by_task(&scope, task_id).await.expect("list after").is_empty());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn service_blocks_deleting_someone_elses_comment(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let author_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        seed_org_user_task(&pool, org_id, author_id, task_id).await;
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'other@example.com')")
            .bind(other_user_id)
            .execute(&pool)
            .await
            .expect("seed other user");

        let author_scope = tenant_scope_for_ids(org_id, author_id);
        let other_scope = tenant_scope_for_ids(org_id, other_user_id);
        let service = crate::services::orchestration::OrchestrationService::new(
            crate::repositories::orchestration::OrchestrationTaskRepository::new(pool.clone()),
            crate::repositories::orchestration::ParticipantRepository::new(pool.clone()),
        );

        let comment = service
            .create_task_comment(&author_scope, task_id, Some("blocker"), "Waiting on review")
            .await
            .expect("author posts");
        assert_eq!(comment.kind, "blocker");

        // Anyone in the org can read it.
        assert_eq!(service.list_task_comments(&other_scope, task_id).await.expect("other reads").len(), 1);

        // Only the author can delete it.
        let blocked = service
            .delete_task_comment(&other_scope, task_id, comment.id)
            .await
            .expect_err("other user delete must fail");
        assert!(blocked.to_string().contains("forbidden"));

        service.delete_task_comment(&author_scope, task_id, comment.id).await.expect("author deletes");
        assert!(service.list_task_comments(&author_scope, task_id).await.expect("list after delete").is_empty());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn latest_marker_takes_last_signal_and_ignores_plain_comments(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let marked_task = Uuid::new_v4();
        let fresh_task = Uuid::new_v4();
        let other_org_id = Uuid::new_v4();
        seed_org_user_task(&pool, org_id, user_id, marked_task).await;
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
             VALUES ($1, $2, 'Fresh task', 'backlog', 'normal', $3)",
        )
        .bind(fresh_task)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed second task");

        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org_id, user_id);
        let repo = TaskCommentRepository::new(pool.clone());

        repo.create(&scope, marked_task, scope.user_id(), "comment", "Plan looks good").await.expect("plain comment");
        repo.create(&scope, marked_task, scope.user_id(), "blocker", "Blocked on review").await.expect("blocker");
        repo.create(&scope, marked_task, scope.user_id(), "unblock", "Review done").await.expect("unblock");
        // fresh_task keeps only an ordinary comment: it must NOT appear.

        let marks = repo.latest_marker_by_tasks(&scope, &[marked_task, fresh_task]).await.expect("latest marks");
        assert_eq!(marks.len(), 1, "fresh_task has no blocker/unblock signal");
        assert_eq!(marks[0].task_id, marked_task);
        assert_eq!(marks[0].kind, "unblock", "the latest signal wins");

        let cross = repo.latest_marker_by_tasks(&other_scope, &[marked_task]).await.expect("cross marks");
        assert!(cross.is_empty(), "cross-tenant reads are empty");
        assert_eq!(repo.latest_marker_by_tasks(&scope, &[]).await.expect("empty list"), Vec::<HumanMarkerRow>::new());
    }
}
