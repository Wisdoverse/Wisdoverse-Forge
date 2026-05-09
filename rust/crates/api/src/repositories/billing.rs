//! Billing repository — database queries for plans, subscriptions, and invoices.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{BillingPlan, Invoice, Subscription};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for billing.
pub struct BillingRepository {
    pool: PgPool,
}

impl BillingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── Plans ──────────────────────────────────────────────────────────

    /// List all available billing plans.
    pub async fn list_plans(&self) -> AppResult<Vec<BillingPlan>> {
        let plans = sqlx::query_as::<_, BillingPlan>(r#"SELECT * FROM billing_plans ORDER BY max_agents ASC"#)
            .fetch_all(&self.pool)
            .await?;
        Ok(plans)
    }

    /// Find a billing plan by ID.
    pub async fn find_plan_by_id(&self, id: Uuid) -> AppResult<BillingPlan> {
        sqlx::query_as::<_, BillingPlan>(r#"SELECT * FROM billing_plans WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("billing plan {id}")).into())
    }

    /// Find a billing plan by Stripe price ID.
    pub async fn find_plan_by_stripe_price_id(&self, stripe_price_id: &str) -> AppResult<BillingPlan> {
        sqlx::query_as::<_, BillingPlan>(r#"SELECT * FROM billing_plans WHERE stripe_price_id = $1"#)
            .bind(stripe_price_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("billing plan for Stripe price {stripe_price_id}")).into())
    }

    /// Load the current user's email for Stripe customer prefill.
    pub async fn get_user_email(&self, scope: &TenantScope) -> AppResult<String> {
        sqlx::query_scalar::<_, String>(r#"SELECT email FROM users WHERE id = $1 AND deleted_at IS NULL"#)
            .bind(scope.user_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("user {}", scope.user_id())).into())
    }

    // ── Subscriptions (tenant-scoped) ─────────────────────────────────

    /// Get the current service-entitling subscription for the organization.
    pub async fn get_subscription(&self, scope: &TenantScope) -> AppResult<Option<Subscription>> {
        let sub = sqlx::query_as::<_, Subscription>(
            r#"SELECT * FROM subscriptions
               WHERE organization_id = $1 AND status IN ('active', 'trialing', 'past_due')
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(sub)
    }

    /// Get a Stripe-backed subscription by Stripe subscription ID.
    pub async fn get_subscription_by_stripe_id(&self, stripe_subscription_id: &str) -> AppResult<Option<Subscription>> {
        let sub = sqlx::query_as::<_, Subscription>(
            r#"SELECT * FROM subscriptions
               WHERE stripe_subscription_id = $1
               LIMIT 1"#,
        )
        .bind(stripe_subscription_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(sub)
    }

    /// Get the newest subscription for a Stripe customer ID.
    pub async fn get_subscription_by_stripe_customer_id(
        &self,
        stripe_customer_id: &str,
    ) -> AppResult<Option<Subscription>> {
        let sub = sqlx::query_as::<_, Subscription>(
            r#"SELECT * FROM subscriptions
               WHERE stripe_customer_id = $1
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(stripe_customer_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(sub)
    }

    /// Create a new subscription for the organization.
    pub async fn create_subscription(
        &self,
        scope: &TenantScope,
        plan_id: Uuid,
        stripe_sub_id: Option<&str>,
        stripe_cust_id: Option<&str>,
    ) -> AppResult<Subscription> {
        sqlx::query_as::<_, Subscription>(
            r#"INSERT INTO subscriptions (organization_id, plan_id, stripe_subscription_id, stripe_customer_id, status)
               VALUES ($1, $2, $3, $4, 'active')
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(plan_id)
        .bind(stripe_sub_id)
        .bind(stripe_cust_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Upsert Stripe's subscription source of truth into the local tenant row.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_stripe_subscription(
        &self,
        organization_id: Uuid,
        plan_id: Uuid,
        stripe_subscription_id: &str,
        stripe_customer_id: Option<&str>,
        status: &str,
        current_period_start: Option<DateTime<Utc>>,
        current_period_end: Option<DateTime<Utc>>,
        cancel_at_period_end: bool,
        canceled_at: Option<DateTime<Utc>>,
    ) -> AppResult<Subscription> {
        let mut tx = self.pool.begin().await?;

        if status == "active" {
            sqlx::query(
                r#"UPDATE subscriptions
                   SET status = 'canceled',
                       cancel_at_period_end = false,
                       canceled_at = COALESCE(canceled_at, now())
                   WHERE organization_id = $1
                     AND status = 'active'
                     AND stripe_subscription_id IS DISTINCT FROM $2"#,
            )
            .bind(organization_id)
            .bind(stripe_subscription_id)
            .execute(&mut *tx)
            .await?;
        }

        let subscription = sqlx::query_as::<_, Subscription>(
            r#"INSERT INTO subscriptions (
                   organization_id,
                   plan_id,
                   stripe_subscription_id,
                   stripe_customer_id,
                   status,
                   current_period_start,
                   current_period_end,
                   cancel_at_period_end,
                   canceled_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (stripe_subscription_id) WHERE stripe_subscription_id IS NOT NULL
               DO UPDATE SET
                   organization_id = EXCLUDED.organization_id,
                   plan_id = EXCLUDED.plan_id,
                   stripe_customer_id = EXCLUDED.stripe_customer_id,
                   status = EXCLUDED.status,
                   current_period_start = EXCLUDED.current_period_start,
                   current_period_end = EXCLUDED.current_period_end,
                   cancel_at_period_end = EXCLUDED.cancel_at_period_end,
                   canceled_at = EXCLUDED.canceled_at,
                   updated_at = now()
               RETURNING *"#,
        )
        .bind(organization_id)
        .bind(plan_id)
        .bind(stripe_subscription_id)
        .bind(stripe_customer_id)
        .bind(status)
        .bind(current_period_start)
        .bind(current_period_end)
        .bind(cancel_at_period_end)
        .bind(canceled_at)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(subscription)
    }

    /// Apply a Stripe lifecycle update to an already-known tenant subscription.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_subscription_from_stripe(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        current_period_start: Option<DateTime<Utc>>,
        current_period_end: Option<DateTime<Utc>>,
        cancel_at_period_end: bool,
        canceled_at: Option<DateTime<Utc>>,
    ) -> AppResult<Subscription> {
        sqlx::query_as::<_, Subscription>(
            r#"UPDATE subscriptions
               SET status = $1,
                   current_period_start = $2,
                   current_period_end = $3,
                   cancel_at_period_end = $4,
                   canceled_at = $5
               WHERE id = $6 AND organization_id = $7
               RETURNING *"#,
        )
        .bind(status)
        .bind(current_period_start)
        .bind(current_period_end)
        .bind(cancel_at_period_end)
        .bind(canceled_at)
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("subscription {id}")).into())
    }

    /// Update subscription status.
    pub async fn update_subscription_status(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
    ) -> AppResult<Subscription> {
        sqlx::query_as::<_, Subscription>(
            r#"UPDATE subscriptions SET status = $1
               WHERE id = $2 AND organization_id = $3
               RETURNING *"#,
        )
        .bind(status)
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("subscription {id}")).into())
    }

    /// Cancel a subscription (set status to 'canceled' and record cancellation time).
    pub async fn cancel_subscription(&self, scope: &TenantScope, id: Uuid) -> AppResult<Subscription> {
        sqlx::query_as::<_, Subscription>(
            r#"UPDATE subscriptions SET status = 'canceled', cancel_at_period_end = false, canceled_at = now()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("subscription {id}")).into())
    }

    // ── Invoices (tenant-scoped) ──────────────────────────────────────

    /// List invoices for the organization (paginated).
    pub async fn list_invoices(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Invoice>> {
        let invoices = sqlx::query_as::<_, Invoice>(
            r#"SELECT * FROM invoices
               WHERE organization_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(invoices)
    }

    /// Upsert a Stripe invoice by Stripe invoice ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_invoice(
        &self,
        organization_id: Uuid,
        subscription_id: Option<Uuid>,
        stripe_invoice_id: Option<&str>,
        amount_cents: i32,
        currency: &str,
        status: &str,
        paid_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
    ) -> AppResult<Invoice> {
        sqlx::query_as::<_, Invoice>(
            r#"INSERT INTO invoices (
                   organization_id,
                   subscription_id,
                   stripe_invoice_id,
                   amount_cents,
                   currency,
                   status,
                   paid_at,
                   created_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (stripe_invoice_id) WHERE stripe_invoice_id IS NOT NULL
               DO UPDATE SET
                   subscription_id = EXCLUDED.subscription_id,
                   amount_cents = EXCLUDED.amount_cents,
                   currency = EXCLUDED.currency,
                   status = EXCLUDED.status,
                   paid_at = EXCLUDED.paid_at
               RETURNING *"#,
        )
        .bind(organization_id)
        .bind(subscription_id)
        .bind(stripe_invoice_id)
        .bind(amount_cents)
        .bind(currency)
        .bind(status)
        .bind(paid_at)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
