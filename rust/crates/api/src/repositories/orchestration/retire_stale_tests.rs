//! Runtime tests for batch retirement of stale tasks.

use super::*;
use crate::test_support::tenant_scope_for_ids;

#[sqlx::test(migrations = "../db/migrations")]
async fn retire_stale_only_touches_untouched_stale_tasks_in_the_group(pool: sqlx::PgPool) {
    let org_id = Uuid::new_v4();
    let other_org = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    for (org, slug) in [(org_id, "retire-org"), (other_org, "retire-other")] {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Retire Org', $2)")
            .bind(org)
            .bind(slug)
            .execute(&pool)
            .await
            .expect("seed org");
    }
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind("retire@example.com")
        .execute(&pool)
        .await
        .expect("seed user");

    let group = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO groups (id, organization_id, name, description, created_by) VALUES ($1, $2, 'G', 'G', $3)",
    )
    .bind(group)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed group");

    let stale = Uuid::new_v4();
    let fresh = Uuid::new_v4();
    let working = Uuid::new_v4();
    let other_group = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO groups (id, organization_id, name, description, created_by) VALUES ($1, $2, 'Other', 'Other', $3)",
    )
    .bind(other_group)
    .bind(other_org)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed other group");
    let other = Uuid::new_v4();
    for (id, status, progress, age_days, org, grp) in [
        (stale, "backlog", 0, 10, org_id, Some(group)),
        (fresh, "queued", 0, 0, org_id, Some(group)),
        (working, "working", 55, 10, org_id, Some(group)),
        (other, "backlog", 0, 10, other_org, Some(other_group)),
    ] {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, group_id, title, status, priority, \
             created_by, progress, created_at, updated_at) \
             VALUES ($1, $2, $3, 'T', $4, 'normal', $5, $6, \
                     NOW() - ($7 || ' days')::interval, NOW() - ($7 || ' days')::interval)",
        )
        .bind(id)
        .bind(org)
        .bind(grp)
        .bind(status)
        .bind(user_id)
        .bind(progress)
        .bind(age_days)
        .execute(&pool)
        .await
        .expect("seed task");
    }

    let repo = OrchestrationTaskRepository::new(pool.clone());
    let scope = tenant_scope_for_ids(org_id, user_id);
    let ids = repo.retire_stale_tasks(&scope, group, 7, 100).await.expect("retire");
    assert_eq!(ids, vec![stale], "only the untouched stale backlog task is retired");

    async fn read_status(pool: &PgPool, id: Uuid) -> String {
        sqlx::query_scalar::<_, String>("SELECT status FROM orchestration_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("read status")
    }
    assert_eq!(read_status(&pool, stale).await, "canceled");
    assert_eq!(read_status(&pool, fresh).await, "queued", "fresh tasks are kept");
    assert_eq!(read_status(&pool, working).await, "working", "started tasks are kept");
    assert_eq!(read_status(&pool, other).await, "backlog", "other-org tasks are untouched");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn retire_stale_respects_batch_limit(pool: sqlx::PgPool) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Retire Org', $2)")
        .bind(org_id)
        .bind("retire-batch")
        .execute(&pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind("retire2@example.com")
        .execute(&pool)
        .await
        .expect("seed user");
    let group = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO groups (id, organization_id, name, description, created_by) VALUES ($1, $2, 'G', 'G', $3)",
    )
    .bind(group)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed group");
    for _ in 0..5 {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, group_id, title, status, priority, \
             created_by, progress, created_at, updated_at) \
             VALUES ($1, $2, $3, 'T', 'backlog', 'normal', $4, 0, \
                     NOW() - interval '10 days', NOW() - interval '10 days')",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(group)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed backlog");
    }

    let repo = OrchestrationTaskRepository::new(pool.clone());
    let scope = tenant_scope_for_ids(org_id, user_id);
    let ids = repo.retire_stale_tasks(&scope, group, 7, 2).await.expect("retire capped");
    assert_eq!(ids.len(), 2, "batch limit caps the sweep");
}
