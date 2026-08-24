//! Runtime tests for the queued-time prediction queries on
//! [`OrchestrationTaskRepository`] (queue snapshot + median duration).
//!
//! `sqlx::test` creates a fresh database per test from the crate migrations,
//! so these are the authoritative checks for the exact SQL the service runs.

use super::*;
use crate::test_support::tenant_scope_for_ids;

/// The queue snapshot must match the auto-dispatcher drain order (urgent
/// first, then age) and the median must ignore zero-duration edge rows and
/// other-tenant rows.
#[sqlx::test(migrations = "../db/migrations")]
async fn queue_order_and_typical_duration_are_tenant_scoped(pool: sqlx::PgPool) {
    let org_id = Uuid::new_v4();
    let other_org = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let other_user = Uuid::new_v4();
    for (org, user, slug) in [(org_id, user_id, "wait-org"), (other_org, other_user, "wait-other")] {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Wait Org', $2)")
            .bind(org)
            .bind(slug)
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user)
            .bind(format!("u-{org}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");
    }

    // Completed history: 60s + 120s + 180s -> median 120s. A zero-duration
    // edge row (same org) and the other org's fast task must not skew it.
    for secs in [60, 120, 180, 0] {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, \
            created_by, started_at, completed_at, created_at, updated_at) \
            VALUES ($1, $2, 'Done', 'completed', 'normal', $3, \
            NOW() - ($4 || ' seconds')::interval, NOW(), NOW() - interval '3 days', NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(user_id)
        .bind(secs)
        .execute(&pool)
        .await
        .expect("seed completed");
    }
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, \
        created_by, started_at, completed_at, created_at, updated_at) \
        VALUES ($1, $2, 'Other fast', 'completed', 'normal', $3, \
        NOW() - interval '1 second', NOW(), NOW(), NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(other_org)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed other org completed");

    let repo = OrchestrationTaskRepository::new(pool.clone());
    let scope = tenant_scope_for_ids(org_id, user_id);
    let typical = repo.typical_wait_seconds(&scope).await.expect("median");
    assert_eq!(typical, Some(120), "median of 60/120/180 plus a 0s edge row");

    // Queued order: urgent (oldest) first, then normals by age; the other
    // org's urgent task is excluded entirely.
    let urgent = Uuid::new_v4();
    let normal_older = Uuid::new_v4();
    let normal_newer = Uuid::new_v4();
    for (id, priority, age) in [(urgent, "urgent", 2), (normal_older, "normal", 1), (normal_newer, "normal", 0)] {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, \
            created_by, created_at, updated_at) \
            VALUES ($1, $2, 'Waiting', 'queued', $3, $4, NOW() - ($5 || ' hours')::interval, NOW())",
        )
        .bind(id)
        .bind(org_id)
        .bind(priority)
        .bind(user_id)
        .bind(age)
        .execute(&pool)
        .await
        .expect("seed queued");
    }
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, \
        created_by, created_at, updated_at) \
        VALUES ($1, $2, 'Other queued', 'queued', 'urgent', $3, NOW() - interval '10 hours', NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(other_org)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed other queued");

    let queued = repo.queued_tasks_ordered(&scope).await.expect("queue");
    let ids: Vec<Uuid> = queued.iter().map(|k| k.id).collect();
    assert_eq!(ids, vec![urgent, normal_older, normal_newer], "urgent first, then age; other org excluded");
    assert_eq!(queued[0].priority, "urgent");
}
