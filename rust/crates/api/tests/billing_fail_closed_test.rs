//! Billing write paths must fail closed until Stripe is wired end to end.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agentforge_api::repositories::billing::BillingRepository;
use agentforge_api::services::billing::{
    BillingGateway, BillingService, CheckoutSession, CheckoutSessionInput, DirectSubscriptionInput, PortalSession,
    StripeEvent, StripeSubscriptionSnapshot,
};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{AppResult, ErrorKind, TenantScope};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Default)]
struct MockGateway {
    calls: AtomicUsize,
}

#[async_trait]
impl BillingGateway for MockGateway {
    fn is_configured(&self) -> bool {
        true
    }

    async fn create_checkout_session(&self, _input: CheckoutSessionInput) -> AppResult<CheckoutSession> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CheckoutSession { id: "cs_test".into(), url: "https://staging.example.com/checkout".into() })
    }

    async fn create_direct_subscription(
        &self,
        _input: DirectSubscriptionInput,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ErrorKind::Unavailable("unexpected Stripe call".into()).into())
    }

    async fn create_portal_session(&self, _customer_id: &str, _return_url: &str) -> AppResult<PortalSession> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ErrorKind::Unavailable("unexpected Stripe call".into()).into())
    }

    async fn cancel_subscription(
        &self,
        _subscription_id: &str,
        _immediately: bool,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ErrorKind::Unavailable("unexpected Stripe call".into()).into())
    }

    async fn resume_subscription(&self, _subscription_id: &str) -> AppResult<StripeSubscriptionSnapshot> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ErrorKind::Unavailable("unexpected Stripe call".into()).into())
    }

    fn verify_webhook_payload(&self, _payload: &str, _signature: &str) -> AppResult<StripeEvent> {
        Err(ErrorKind::Unavailable("unexpected webhook call".into()).into())
    }
}

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

fn assert_forbidden<T: std::fmt::Debug>(result: AppResult<T>) {
    let error = result.expect_err("ordinary org members must not manage billing");
    assert!(matches!(error.kind, ErrorKind::Forbidden(_)), "expected Forbidden, got {error:?}");
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

#[sqlx::test(migrations = "../db/migrations")]
async fn billing_management_requires_org_manager_before_gateway_calls(pool: PgPool) {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind("Billing Permission Test Org")
        .bind(format!("billing-permission-{org_id}"))
        .execute(&pool)
        .await
        .expect("seed org");

    let mut scopes = Vec::new();
    for (role, label) in [("member", "member"), ("owner", "owner")] {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("billing-{label}-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("seed org membership");
        scopes.push(tenant_scope_for_ids(org_id, user_id));
    }
    let member = &scopes[0];
    let owner = &scopes[1];

    let plan_id = free_plan_id(&pool).await;
    sqlx::query("UPDATE billing_plans SET stripe_price_id = $1 WHERE id = $2")
        .bind("price_test_billing_permission")
        .bind(plan_id)
        .execute(&pool)
        .await
        .expect("map free plan to Stripe");

    let gateway = Arc::new(MockGateway::default());
    let service = BillingService::with_gateway(BillingRepository::new(pool), gateway.clone());

    assert_forbidden(
        service
            .create_checkout_session(
                member,
                plan_id,
                "monthly",
                "https://staging.example.com/success",
                "https://staging.example.com/cancel",
                None,
            )
            .await,
    );
    assert_forbidden(service.subscribe(member, plan_id, Some("pm_test_123")).await);
    assert_forbidden(service.cancel(member, false).await);
    assert_forbidden(service.resume(member).await);
    assert_forbidden(service.create_portal_session(member, "https://staging.example.com/account").await);
    assert_eq!(gateway.calls.load(Ordering::SeqCst), 0, "forbidden calls must not reach Stripe");

    let checkout = service
        .create_checkout_session(
            owner,
            plan_id,
            "monthly",
            "https://staging.example.com/success",
            "https://staging.example.com/cancel",
            None,
        )
        .await
        .expect("org owner can create a checkout session");
    assert_eq!(checkout.id, "cs_test");
    assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
}
