use std::sync::Arc;

use agentforge_api::repositories::billing::BillingRepository;
use agentforge_api::services::billing::{
    BillingGateway, BillingService, CheckoutSession, CheckoutSessionInput, DirectSubscriptionInput, PortalSession,
    StripeSubscriptionSnapshot,
};
use agentforge_core::{AppResult, ErrorKind};
use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Default)]
struct WebhookOnlyGateway;

#[async_trait]
impl BillingGateway for WebhookOnlyGateway {
    fn is_configured(&self) -> bool {
        true
    }

    async fn create_checkout_session(&self, _input: CheckoutSessionInput) -> AppResult<CheckoutSession> {
        Err(ErrorKind::Unavailable("not used".to_string()).into())
    }

    async fn create_direct_subscription(
        &self,
        _input: DirectSubscriptionInput,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        Err(ErrorKind::Unavailable("not used".to_string()).into())
    }

    async fn create_portal_session(&self, _customer_id: &str, _return_url: &str) -> AppResult<PortalSession> {
        Err(ErrorKind::Unavailable("not used".to_string()).into())
    }

    async fn cancel_subscription(
        &self,
        _subscription_id: &str,
        _immediately: bool,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        Err(ErrorKind::Unavailable("not used".to_string()).into())
    }

    async fn resume_subscription(&self, _subscription_id: &str) -> AppResult<StripeSubscriptionSnapshot> {
        Err(ErrorKind::Unavailable("not used".to_string()).into())
    }

    fn verify_webhook_payload(&self, payload: &str, _signature: &str) -> AppResult<serde_json::Value> {
        serde_json::from_str(payload).map_err(|err| ErrorKind::Validation(err.to_string()).into())
    }
}

async fn seed_org(pool: &PgPool) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind("Stripe Webhook Test Org")
        .bind(format!("stripe-webhook-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    org_id
}

async fn stripe_mapped_plan(pool: &PgPool, stripe_price_id: &str) -> Uuid {
    let plan_id: Uuid = sqlx::query_scalar("SELECT id FROM billing_plans WHERE name = 'free'")
        .fetch_one(pool)
        .await
        .expect("seeded free billing plan");
    sqlx::query("UPDATE billing_plans SET stripe_price_id = $1 WHERE id = $2")
        .bind(stripe_price_id)
        .bind(plan_id)
        .execute(pool)
        .await
        .expect("map plan to Stripe price");
    plan_id
}

#[sqlx::test(migrations = "../db/migrations")]
async fn subscription_webhook_upserts_subscription_and_invoice_idempotently(pool: PgPool) {
    let org_id = seed_org(&pool).await;
    let plan_id = stripe_mapped_plan(&pool, "price_test_webhook").await;
    let service = BillingService::with_gateway(BillingRepository::new(pool.clone()), Arc::new(WebhookOnlyGateway));

    let subscription_event = json!({
        "id": "evt_sub_created",
        "type": "customer.subscription.updated",
        "data": {
            "object": {
                "id": "sub_test_webhook",
                "customer": "cus_test_webhook",
                "status": "active",
                "current_period_start": 1_700_000_000,
                "current_period_end": 1_700_086_400,
                "cancel_at_period_end": true,
                "metadata": {
                    "org_id": org_id.to_string(),
                    "plan_id": plan_id.to_string()
                },
                "items": {
                    "data": [
                        { "price": { "id": "price_test_webhook" } }
                    ]
                }
            }
        }
    });

    service
        .handle_webhook(&subscription_event.to_string(), "test-signature")
        .await
        .expect("subscription webhook should persist");

    let sub: (Uuid, String, String, String, bool) = sqlx::query_as(
        r#"SELECT plan_id, stripe_subscription_id, stripe_customer_id, status, cancel_at_period_end
           FROM subscriptions
           WHERE organization_id = $1"#,
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("subscription persisted");

    assert_eq!(sub.0, plan_id);
    assert_eq!(sub.1, "sub_test_webhook");
    assert_eq!(sub.2, "cus_test_webhook");
    assert_eq!(sub.3, "active");
    assert!(sub.4);

    let invoice_paid = json!({
        "id": "evt_invoice_paid",
        "type": "invoice.paid",
        "data": {
            "object": {
                "id": "in_test_webhook",
                "customer": "cus_test_webhook",
                "subscription": "sub_test_webhook",
                "amount_due": 2500,
                "amount_paid": 2500,
                "total": 2500,
                "currency": "usd",
                "status": "paid",
                "status_transitions": { "paid_at": 1_700_000_010 },
                "created": 1_700_000_000
            }
        }
    });
    service.handle_webhook(&invoice_paid.to_string(), "test-signature").await.expect("invoice webhook should persist");
    service
        .handle_webhook(&invoice_paid.to_string(), "test-signature")
        .await
        .expect("invoice webhook should be idempotent");

    let invoice_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE stripe_invoice_id = $1")
        .bind("in_test_webhook")
        .fetch_one(&pool)
        .await
        .expect("count invoice");
    assert_eq!(invoice_count, 1, "Stripe invoice webhook must upsert by invoice ID");
}
