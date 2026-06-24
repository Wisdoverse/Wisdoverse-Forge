//! Billing write paths must fail closed until Stripe is wired end to end.

use agentforge_api::repositories::billing::BillingRepository;
use agentforge_api::services::billing::BillingService;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{ErrorKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_scope(pool: &PgPool) -> TenantScope {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind("Billing Test Org")
        .bind(format!("billing-test-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("billing-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");

    tenant_scope_for_ids(org_id, user_id)
}

async fn free_plan_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM billing_plans WHERE name = 'free'")
        .fetch_one(pool)
        .await
        .expect("seeded free billing plan")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn subscribe_refuses_to_create_local_active_subscription(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let plan_id = free_plan_id(&pool).await;
    let service = BillingService::new(BillingRepository::new(pool.clone()));

    let err =
        service.subscribe(&scope, plan_id, Some("pm_test_123")).await.expect_err("billing writes must fail closed");

    assert!(
        matches!(err.kind, ErrorKind::Unavailable(ref message) if message.contains("Stripe billing is not configured")),
        "unexpected error: {err:?}"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscriptions WHERE organization_id = $1")
        .bind(scope.org_id().as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count subscriptions");
    assert_eq!(count, 0, "subscribe must not create local-only active subscriptions");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cancel_refuses_to_mutate_local_only_subscription(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let plan_id = free_plan_id(&pool).await;
    let sub_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO subscriptions (id, organization_id, plan_id, status)
           VALUES ($1, $2, $3, 'active')"#,
    )
    .bind(sub_id)
    .bind(scope.org_id().as_uuid())
    .bind(plan_id)
    .execute(&pool)
    .await
    .expect("seed local-only active subscription");

    let service = BillingService::new(BillingRepository::new(pool.clone()));
    let err = service.cancel(&scope, false).await.expect_err("billing cancellation must fail closed");

    assert!(
        matches!(err.kind, ErrorKind::Unavailable(ref message) if message.contains("Stripe billing is not configured")),
        "unexpected error: {err:?}"
    );

    let row: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, canceled_at FROM subscriptions WHERE id = $1")
            .bind(sub_id)
            .fetch_one(&pool)
            .await
            .expect("reload subscription");
    assert_eq!(row.0, "active", "cancel must not locally mark subscription canceled");
    assert!(row.1.is_none(), "cancel must not set canceled_at without Stripe confirmation");
}
