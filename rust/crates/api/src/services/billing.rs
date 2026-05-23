//! Billing service — plan management, Stripe lifecycle, and limit checks.

mod stripe;

use std::sync::Arc;

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::{BillingPlan, Invoice, Subscription};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::billing::{
    BillingCycle, BillingPlanPolicy, BillingPlanView, BillingRedirectUrlPolicy, BillingSubscriptionProjection,
    BillingUsageLimitPolicy, BillingWebhookReconciliationPolicy, CheckoutCouponPolicy, InvoiceListPage,
    InvoiceSubscriptionLookup, InvoiceView, PaymentMethodId, SubscriptionLifecyclePolicy, SubscriptionOrgResolution,
    SubscriptionPlanResolution, SubscriptionStatusPolicy, SubscriptionView, UsageMetricView, stripe_event,
};
pub(crate) use crate::domain::billing::{
    BillingStripeGatewayPolicy, billing_checkout_response, billing_data_response, billing_invoices_response,
    billing_plans_response, billing_portal_response, billing_subscription_data_response, billing_subscription_response,
    billing_usage_response, billing_webhook_received_response,
};
pub use crate::domain::billing::{StripeInvoiceSnapshot, StripeSubscriptionSnapshot};
use crate::repositories::billing::BillingRepository;
pub use stripe::{
    BillingGateway, CheckoutSession, CheckoutSessionInput, DirectSubscriptionInput, DisabledBillingGateway,
    PortalSession, billing_gateway_from_config,
};
use stripe::{parse_invoice_object, parse_subscription_object};

/// Business logic layer for billing operations.
pub struct BillingService {
    repo: BillingRepository,
    gateway: Arc<dyn BillingGateway>,
}

impl BillingService {
    pub fn new(repo: BillingRepository) -> Self {
        Self::with_gateway(repo, Arc::new(DisabledBillingGateway))
    }

    pub fn with_gateway(repo: BillingRepository, gateway: Arc<dyn BillingGateway>) -> Self {
        Self { repo, gateway }
    }

    pub fn from_runtime(pool: PgPool, gateway: Arc<dyn BillingGateway>) -> Self {
        Self::with_gateway(BillingRepository::new(pool), gateway)
    }

    /// List all available billing plans.
    pub async fn list_plans(&self) -> AppResult<Vec<BillingPlan>> {
        self.repo.list_plans().await
    }

    /// List all available billing plans in the legacy browser projection.
    pub(crate) async fn list_plan_views(&self) -> AppResult<Vec<BillingPlanView>> {
        Ok(self.repo.list_plans().await?.into_iter().map(plan_view).collect())
    }

    /// Get the current active subscription for the organization.
    pub async fn get_current_subscription(&self, scope: &TenantScope) -> AppResult<Option<Subscription>> {
        self.repo.get_subscription(scope).await
    }

    /// Get the current subscription and associated plan projection.
    pub(crate) async fn get_current_subscription_projection(
        &self,
        scope: &TenantScope,
    ) -> AppResult<BillingSubscriptionProjection> {
        let subscription = self.repo.get_subscription(scope).await?;
        let plan = match &subscription {
            Some(subscription) => Some(plan_view(self.repo.find_plan_by_id(subscription.plan_id).await?)),
            None => None,
        };

        Ok(BillingSubscriptionProjection { subscription: subscription.map(subscription_view), plan })
    }

    /// Create a hosted Stripe Checkout session for a plan.
    pub async fn create_checkout_session(
        &self,
        scope: &TenantScope,
        plan_id: Uuid,
        billing_cycle: &str,
        success_url: &str,
        cancel_url: &str,
        coupon_code: Option<&str>,
    ) -> AppResult<CheckoutSession> {
        self.ensure_gateway_configured()?;
        BillingCycle::parse(billing_cycle)?;
        BillingRedirectUrlPolicy::validate(success_url, "success_url")?;
        BillingRedirectUrlPolicy::validate(cancel_url, "cancel_url")?;
        CheckoutCouponPolicy::ensure_not_pre_applied(coupon_code)?;

        let plan = self.repo.find_plan_by_id(plan_id).await?;
        let price_id = BillingPlanPolicy::require_stripe_price_id(&plan.name, plan.stripe_price_id.as_deref())?;

        let existing_subscription_id = self.repo.get_subscription(scope).await?.map(|subscription| subscription.id);
        SubscriptionLifecyclePolicy::ensure_no_active_subscription(existing_subscription_id)?;

        let user_email = self.repo.get_user_email(scope).await?;
        self.gateway
            .create_checkout_session(CheckoutSessionInput {
                org_id: scope.org_id().as_uuid(),
                user_id: scope.user_id().as_uuid(),
                user_email,
                plan_id,
                price_id,
                billing_cycle: billing_cycle.to_string(),
                success_url: success_url.to_string(),
                cancel_url: cancel_url.to_string(),
            })
            .await
    }

    /// Subscribe the organization to a plan through a Stripe PaymentMethod.
    ///
    /// Hosted Checkout remains the preferred browser flow. This compatibility
    /// route exists for API callers that already created a Stripe PaymentMethod
    /// client-side and want the backend to provision customer + subscription.
    pub async fn subscribe(
        &self,
        scope: &TenantScope,
        plan_id: Uuid,
        payment_method_id: Option<&str>,
    ) -> AppResult<Subscription> {
        self.ensure_gateway_configured()?;
        let plan = self.repo.find_plan_by_id(plan_id).await?;
        let price_id = BillingPlanPolicy::require_stripe_price_id(&plan.name, plan.stripe_price_id.as_deref())?;
        let payment_method_id = PaymentMethodId::parse(payment_method_id)?;

        let existing_subscription_id = self.repo.get_subscription(scope).await?.map(|subscription| subscription.id);
        SubscriptionLifecyclePolicy::ensure_no_active_subscription(existing_subscription_id)?;

        let user_email = self.repo.get_user_email(scope).await?;
        let snapshot = self
            .gateway
            .create_direct_subscription(DirectSubscriptionInput {
                org_id: scope.org_id().as_uuid(),
                user_id: scope.user_id().as_uuid(),
                user_email,
                plan_id,
                price_id,
                payment_method_id: payment_method_id.value().to_string(),
            })
            .await?;

        self.persist_subscription_snapshot(snapshot, Some(scope.org_id().as_uuid()), Some(plan_id)).await
    }

    /// Cancel the current active subscription.
    pub async fn cancel(&self, scope: &TenantScope, immediately: bool) -> AppResult<Subscription> {
        self.ensure_gateway_configured()?;
        let sub = self
            .repo
            .get_subscription(scope)
            .await?
            .ok_or_else(SubscriptionLifecyclePolicy::missing_active_subscription)?;
        let stripe_subscription_id =
            SubscriptionLifecyclePolicy::require_stripe_subscription_id(sub.stripe_subscription_id.as_deref())?;

        let snapshot = self.gateway.cancel_subscription(stripe_subscription_id, immediately).await?;
        SubscriptionStatusPolicy::validate(&snapshot.status)?;
        self.repo
            .update_subscription_from_stripe(
                scope,
                sub.id,
                &snapshot.status,
                snapshot.current_period_start,
                snapshot.current_period_end,
                snapshot.cancel_at_period_end,
                snapshot.canceled_at,
            )
            .await
    }

    /// Resume a subscription that is scheduled to cancel.
    pub async fn resume(&self, scope: &TenantScope) -> AppResult<Subscription> {
        self.ensure_gateway_configured()?;
        let sub = self
            .repo
            .get_subscription(scope)
            .await?
            .ok_or_else(SubscriptionLifecyclePolicy::missing_active_subscription)?;
        let stripe_subscription_id =
            SubscriptionLifecyclePolicy::require_stripe_subscription_id(sub.stripe_subscription_id.as_deref())?;

        let snapshot = self.gateway.resume_subscription(stripe_subscription_id).await?;
        SubscriptionStatusPolicy::validate(&snapshot.status)?;
        self.repo
            .update_subscription_from_stripe(
                scope,
                sub.id,
                &snapshot.status,
                snapshot.current_period_start,
                snapshot.current_period_end,
                snapshot.cancel_at_period_end,
                snapshot.canceled_at,
            )
            .await
    }

    /// Create a Stripe customer portal session for the current organization.
    pub async fn create_portal_session(&self, scope: &TenantScope, return_url: &str) -> AppResult<PortalSession> {
        self.ensure_gateway_configured()?;
        BillingRedirectUrlPolicy::validate(return_url, "return_url")?;
        let sub = self
            .repo
            .get_subscription(scope)
            .await?
            .ok_or_else(SubscriptionLifecyclePolicy::missing_active_subscription)?;
        let customer_id = SubscriptionLifecyclePolicy::require_stripe_customer_id(sub.stripe_customer_id.as_deref())?;

        self.gateway.create_portal_session(customer_id, return_url).await
    }

    /// List invoices for the organization (paginated).
    pub async fn list_invoices(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Invoice>> {
        let page = InvoiceListPage::new(limit, offset);
        self.repo.list_invoices(scope, page.limit(), page.offset()).await
    }

    /// List invoices in the browser projection.
    pub(crate) async fn list_invoice_views(
        &self,
        scope: &TenantScope,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<InvoiceView>> {
        Ok(self.list_invoices(scope, limit, offset).await?.into_iter().map(invoice_view).collect())
    }

    /// Current billing usage projection.
    pub(crate) async fn usage_metrics(&self, scope: &TenantScope) -> AppResult<Vec<UsageMetricView>> {
        let max_agents = match self.repo.get_subscription(scope).await? {
            Some(sub) => self.repo.find_plan_by_id(sub.plan_id).await?.max_agents as i64,
            None => BillingUsageLimitPolicy::default_agent_limit(),
        };
        Ok(vec![UsageMetricView::new("agents", 0, max_agents, 0)])
    }

    /// Check if the organization is within its plan's agent limit.
    ///
    /// Returns `true` if `current_count` is below the plan's `max_agents`.
    /// If no subscription exists, uses a default limit of 1.
    pub async fn check_agent_limit(&self, scope: &TenantScope, current_count: i64) -> AppResult<bool> {
        let max = match self.repo.get_subscription(scope).await? {
            Some(sub) => {
                let plan = self.repo.find_plan_by_id(sub.plan_id).await?;
                plan.max_agents as i64
            }
            None => {
                tracing::info!(org_id = %scope.org_id(), "No active subscription, applying default agent limit of 1");
                BillingUsageLimitPolicy::default_agent_limit()
            }
        };
        Ok(BillingUsageLimitPolicy::is_within_agent_limit(current_count, max))
    }

    /// Verify and apply a Stripe webhook event.
    pub async fn handle_webhook(&self, payload: &str, signature: &str) -> AppResult<()> {
        self.ensure_gateway_configured()?;
        let verified_payload = self.gateway.verify_webhook_payload(payload, signature)?;
        let event = stripe_event(verified_payload)?;

        match event.event_type.as_str() {
            "customer.subscription.created" | "customer.subscription.updated" | "customer.subscription.deleted" => {
                let snapshot = parse_subscription_object(event.data.object)?;
                self.persist_subscription_snapshot(snapshot, None, None).await?;
            }
            "invoice.paid" | "invoice.payment_failed" | "invoice.updated" => {
                let snapshot = parse_invoice_object(event.data.object)?;
                self.persist_invoice_snapshot(snapshot).await?;
            }
            other => {
                tracing::debug!(event_type = other, "ignoring Stripe webhook event");
            }
        }

        Ok(())
    }

    fn ensure_gateway_configured(&self) -> AppResult<()> {
        if self.gateway.is_configured() { Ok(()) } else { Err(BillingStripeGatewayPolicy::not_configured().into()) }
    }

    async fn persist_subscription_snapshot(
        &self,
        snapshot: StripeSubscriptionSnapshot,
        fallback_org_id: Option<Uuid>,
        fallback_plan_id: Option<Uuid>,
    ) -> AppResult<Subscription> {
        SubscriptionStatusPolicy::validate(&snapshot.status)?;

        let existing = self.repo.get_subscription_by_stripe_id(&snapshot.id).await?;
        let org_id = match BillingWebhookReconciliationPolicy::resolve_org_id(
            &snapshot.metadata,
            fallback_org_id,
            existing.as_ref().map(|sub| sub.organization_id.as_uuid()),
        ) {
            SubscriptionOrgResolution::Resolved(org_id) => org_id,
            SubscriptionOrgResolution::MissingMetadata => {
                tracing::warn!(
                    stripe_subscription_id = %snapshot.id,
                    "Stripe subscription webhook missing org_id metadata; cannot reconcile"
                );
                return Err(BillingWebhookReconciliationPolicy::missing_org_metadata_error().into());
            }
        };

        let plan_id = match BillingWebhookReconciliationPolicy::resolve_plan_id(
            &snapshot.metadata,
            fallback_plan_id,
            snapshot.price_id.as_deref(),
            existing.as_ref().map(|sub| sub.plan_id),
        ) {
            SubscriptionPlanResolution::Resolved(plan_id) => plan_id,
            SubscriptionPlanResolution::LookupByStripePrice(price_id) => {
                self.repo.find_plan_by_stripe_price_id(price_id).await?.id
            }
            SubscriptionPlanResolution::MissingMetadata => {
                tracing::warn!(
                    stripe_subscription_id = %snapshot.id,
                    "Stripe subscription webhook missing plan metadata and price; cannot reconcile"
                );
                return Err(BillingWebhookReconciliationPolicy::missing_plan_metadata_error().into());
            }
        };

        self.repo
            .upsert_stripe_subscription(
                org_id,
                plan_id,
                &snapshot.id,
                snapshot.customer_id.as_deref(),
                &snapshot.status,
                snapshot.current_period_start,
                snapshot.current_period_end,
                snapshot.cancel_at_period_end,
                snapshot.canceled_at,
            )
            .await
    }

    async fn persist_invoice_snapshot(&self, snapshot: StripeInvoiceSnapshot) -> AppResult<Invoice> {
        let lookup_plan = BillingWebhookReconciliationPolicy::invoice_subscription_lookup_plan(
            snapshot.subscription_id.as_deref(),
            snapshot.customer_id.as_deref(),
        );
        let mut subscription = match lookup_plan.primary() {
            Some(lookup) => self.lookup_invoice_subscription(lookup).await?,
            None => None,
        };
        if subscription.is_none()
            && let Some(lookup) = lookup_plan.fallback()
        {
            subscription = self.lookup_invoice_subscription(lookup).await?;
        }

        let Some(subscription) = subscription else {
            tracing::warn!(
                stripe_invoice_id = %snapshot.id,
                stripe_subscription_id = ?snapshot.subscription_id,
                stripe_customer_id = ?snapshot.customer_id,
                "Stripe invoice webhook could not be correlated to a local subscription"
            );
            return Err(BillingWebhookReconciliationPolicy::missing_invoice_subscription_error().into());
        };

        self.repo
            .upsert_invoice(
                subscription.organization_id.as_uuid(),
                Some(subscription.id),
                Some(&snapshot.id),
                snapshot.amount_cents,
                &snapshot.currency,
                &snapshot.status,
                snapshot.paid_at,
                snapshot.created_at,
            )
            .await
    }

    async fn lookup_invoice_subscription(
        &self,
        lookup: InvoiceSubscriptionLookup<'_>,
    ) -> AppResult<Option<Subscription>> {
        match lookup {
            InvoiceSubscriptionLookup::StripeSubscriptionId(subscription_id) => {
                self.repo.get_subscription_by_stripe_id(subscription_id).await
            }
            InvoiceSubscriptionLookup::StripeCustomerId(customer_id) => {
                self.repo.get_subscription_by_stripe_customer_id(customer_id).await
            }
        }
    }
}

fn plan_view(plan: BillingPlan) -> BillingPlanView {
    BillingPlanView::from_plan_parts(
        plan.id,
        plan.name,
        &plan.features,
        plan.max_agents,
        plan.max_events_per_day,
        plan.max_storage_mb,
    )
}

fn subscription_view(sub: Subscription) -> SubscriptionView {
    SubscriptionView::new(
        sub.id,
        sub.plan_id,
        sub.status,
        sub.current_period_start,
        sub.current_period_end,
        sub.cancel_at_period_end,
        sub.canceled_at,
    )
}

fn invoice_view(invoice: Invoice) -> InvoiceView {
    InvoiceView::new(
        invoice.id,
        invoice.status,
        invoice.amount_cents,
        invoice.currency,
        invoice.paid_at,
        invoice.created_at,
    )
}
