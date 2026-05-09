//! Billing service — plan management, Stripe lifecycle, and limit checks.

mod stripe;

use std::sync::Arc;

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{BillingPlan, Invoice, Subscription};
use uuid::Uuid;

use crate::repositories::billing::BillingRepository;
pub use stripe::{
    BillingGateway, CheckoutSession, CheckoutSessionInput, DirectSubscriptionInput, DisabledBillingGateway,
    PortalSession, StripeInvoiceSnapshot, StripeSubscriptionSnapshot, billing_gateway_from_config,
};
use stripe::{parse_invoice_object, parse_subscription_object, stripe_event};

/// Valid subscription statuses.
const VALID_STATUSES: &[&str] =
    &["active", "past_due", "canceled", "trialing", "unpaid", "incomplete", "incomplete_expired", "paused"];

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

    /// List all available billing plans.
    pub async fn list_plans(&self) -> AppResult<Vec<BillingPlan>> {
        self.repo.list_plans().await
    }

    /// Get the current active subscription for the organization.
    pub async fn get_current_subscription(&self, scope: &TenantScope) -> AppResult<Option<Subscription>> {
        self.repo.get_subscription(scope).await
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
        validate_billing_cycle(billing_cycle)?;
        validate_redirect_url(success_url, "success_url")?;
        validate_redirect_url(cancel_url, "cancel_url")?;
        if coupon_code.map(|value| !value.trim().is_empty()).unwrap_or(false) {
            return Err(ErrorKind::Validation(
                "pre-applied coupon codes are not supported; enable promotion codes in Stripe Checkout".to_string(),
            )
            .into());
        }

        let plan = self.repo.find_plan_by_id(plan_id).await?;
        let price_id = stripe_price_id(&plan)?;

        if let Some(existing) = self.repo.get_subscription(scope).await? {
            return Err(ErrorKind::Conflict(format!(
                "organization already has an active subscription: {}",
                existing.id
            ))
            .into());
        }

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
        let price_id = stripe_price_id(&plan)?;
        let payment_method_id = payment_method_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ErrorKind::Validation("payment_method_id is required for direct subscribe".to_string()))?;

        if let Some(existing) = self.repo.get_subscription(scope).await? {
            return Err(ErrorKind::Conflict(format!(
                "organization already has an active subscription: {}",
                existing.id
            ))
            .into());
        }

        let user_email = self.repo.get_user_email(scope).await?;
        let snapshot = self
            .gateway
            .create_direct_subscription(DirectSubscriptionInput {
                org_id: scope.org_id().as_uuid(),
                user_id: scope.user_id().as_uuid(),
                user_email,
                plan_id,
                price_id,
                payment_method_id: payment_method_id.to_string(),
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
            .ok_or_else(|| ErrorKind::NotFound("no active subscription".to_string()))?;
        let stripe_subscription_id = sub.stripe_subscription_id.as_deref().ok_or_else(|| {
            ErrorKind::Unavailable("active subscription is missing Stripe subscription ID".to_string())
        })?;

        let snapshot = self.gateway.cancel_subscription(stripe_subscription_id, immediately).await?;
        validate_subscription_status(&snapshot.status)?;
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
            .ok_or_else(|| ErrorKind::NotFound("no active subscription".to_string()))?;
        let stripe_subscription_id = sub.stripe_subscription_id.as_deref().ok_or_else(|| {
            ErrorKind::Unavailable("active subscription is missing Stripe subscription ID".to_string())
        })?;

        let snapshot = self.gateway.resume_subscription(stripe_subscription_id).await?;
        validate_subscription_status(&snapshot.status)?;
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
        validate_redirect_url(return_url, "return_url")?;
        let sub = self
            .repo
            .get_subscription(scope)
            .await?
            .ok_or_else(|| ErrorKind::NotFound("no active subscription".to_string()))?;
        let customer_id = sub
            .stripe_customer_id
            .as_deref()
            .ok_or_else(|| ErrorKind::Unavailable("active subscription is missing Stripe customer ID".to_string()))?;

        self.gateway.create_portal_session(customer_id, return_url).await
    }

    /// List invoices for the organization (paginated).
    pub async fn list_invoices(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Invoice>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list_invoices(scope, limit, offset).await
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
                1
            }
        };
        Ok(current_count < max)
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
        if self.gateway.is_configured() { Ok(()) } else { Err(billing_not_configured().into()) }
    }

    async fn persist_subscription_snapshot(
        &self,
        snapshot: StripeSubscriptionSnapshot,
        fallback_org_id: Option<Uuid>,
        fallback_plan_id: Option<Uuid>,
    ) -> AppResult<Subscription> {
        validate_subscription_status(&snapshot.status)?;

        let existing = self.repo.get_subscription_by_stripe_id(&snapshot.id).await?;
        let org_id = match metadata_uuid(&snapshot.metadata, "org_id").or(fallback_org_id) {
            Some(org_id) => org_id,
            None => match &existing {
                Some(sub) => sub.organization_id.as_uuid(),
                None => {
                    tracing::warn!(
                        stripe_subscription_id = %snapshot.id,
                        "Stripe subscription webhook missing org_id metadata; cannot reconcile"
                    );
                    return Err(ErrorKind::Validation(
                        "Stripe subscription event is missing Wisdoverse Forge org_id metadata".to_string(),
                    )
                    .into());
                }
            },
        };

        let plan_id = match metadata_uuid(&snapshot.metadata, "plan_id").or(fallback_plan_id) {
            Some(plan_id) => plan_id,
            None => match snapshot.price_id.as_deref() {
                Some(price_id) => self.repo.find_plan_by_stripe_price_id(price_id).await?.id,
                None => match &existing {
                    Some(sub) => sub.plan_id,
                    None => {
                        tracing::warn!(
                            stripe_subscription_id = %snapshot.id,
                            "Stripe subscription webhook missing plan metadata and price; cannot reconcile"
                        );
                        return Err(ErrorKind::Validation(
                            "Stripe subscription event is missing Wisdoverse Forge plan metadata".to_string(),
                        )
                        .into());
                    }
                },
            },
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
        let mut subscription = match snapshot.subscription_id.as_deref() {
            Some(subscription_id) => self.repo.get_subscription_by_stripe_id(subscription_id).await?,
            None => None,
        };
        if subscription.is_none()
            && let Some(customer_id) = snapshot.customer_id.as_deref()
        {
            subscription = self.repo.get_subscription_by_stripe_customer_id(customer_id).await?;
        }

        let Some(subscription) = subscription else {
            tracing::warn!(
                stripe_invoice_id = %snapshot.id,
                stripe_subscription_id = ?snapshot.subscription_id,
                stripe_customer_id = ?snapshot.customer_id,
                "Stripe invoice webhook could not be correlated to a local subscription"
            );
            return Err(ErrorKind::Validation(
                "Stripe invoice event does not match a Wisdoverse Forge subscription".to_string(),
            )
            .into());
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
}

fn billing_not_configured() -> ErrorKind {
    ErrorKind::Unavailable(
        "Stripe billing is not configured; refusing to change local subscription state without Stripe confirmation"
            .to_string(),
    )
}

fn stripe_price_id(plan: &BillingPlan) -> AppResult<String> {
    plan.stripe_price_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            ErrorKind::Validation(format!("billing plan '{}' is not mapped to a Stripe price", plan.name)).into()
        })
}

fn validate_billing_cycle(value: &str) -> AppResult<()> {
    match value {
        "monthly" | "yearly" => Ok(()),
        other => Err(ErrorKind::Validation(format!("invalid billing_cycle '{other}'")).into()),
    }
}

fn validate_redirect_url(value: &str, field: &str) -> AppResult<()> {
    let parsed = url::Url::parse(value)
        .map_err(|err| ErrorKind::Validation(format!("{field} must be an absolute URL: {err}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(ErrorKind::Validation(format!("{field} must use http or https, got '{scheme}'")).into()),
    }
}

fn metadata_uuid(metadata: &std::collections::BTreeMap<String, String>, key: &str) -> Option<Uuid> {
    metadata.get(key).and_then(|value| Uuid::parse_str(value).ok())
}

/// Validate that a status string is a recognized subscription status.
pub fn validate_subscription_status(status: &str) -> AppResult<()> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ErrorKind::Validation(format!(
            "invalid subscription status '{}', expected one of: {:?}",
            status, VALID_STATUSES
        ))
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_subscription_statuses() {
        assert!(validate_subscription_status("active").is_ok());
        assert!(validate_subscription_status("past_due").is_ok());
        assert!(validate_subscription_status("canceled").is_ok());
        assert!(validate_subscription_status("trialing").is_ok());
    }

    #[test]
    fn invalid_subscription_status() {
        assert!(validate_subscription_status("expired").is_err());
        assert!(validate_subscription_status("").is_err());
        assert!(validate_subscription_status("ACTIVE").is_err());
    }

    #[test]
    fn valid_statuses_list_is_complete() {
        // Ensure the constant matches what we document
        assert_eq!(VALID_STATUSES.len(), 8);
        assert!(VALID_STATUSES.contains(&"active"));
        assert!(VALID_STATUSES.contains(&"past_due"));
        assert!(VALID_STATUSES.contains(&"canceled"));
        assert!(VALID_STATUSES.contains(&"trialing"));
        assert!(VALID_STATUSES.contains(&"unpaid"));
        assert!(VALID_STATUSES.contains(&"incomplete"));
        assert!(VALID_STATUSES.contains(&"incomplete_expired"));
        assert!(VALID_STATUSES.contains(&"paused"));
    }
}
