//! Billing domain rules.
//!
//! This module owns pure billing request and lifecycle policies that are
//! independent of repositories, Stripe clients, and HTTP route DTOs.

use std::collections::BTreeMap;

use agentforge_core::{AppResult, ErrorKind};
use uuid::Uuid;

/// Valid subscription statuses.
const VALID_STATUSES: &[&str] =
    &["active", "past_due", "canceled", "trialing", "unpaid", "incomplete", "incomplete_expired", "paused"];

/// Billing cycle policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BillingCycle {
    Monthly,
    Yearly,
}

impl BillingCycle {
    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        match value {
            "monthly" => Ok(Self::Monthly),
            "yearly" => Ok(Self::Yearly),
            other => Err(ErrorKind::Validation(format!("invalid billing_cycle '{other}'")).into()),
        }
    }
}

/// Redirect URL policy for Stripe checkout and portal flows.
pub(crate) struct BillingRedirectUrlPolicy;

impl BillingRedirectUrlPolicy {
    pub(crate) fn validate(value: &str, field: &str) -> AppResult<()> {
        let parsed = url::Url::parse(value)
            .map_err(|err| ErrorKind::Validation(format!("{field} must be an absolute URL: {err}")))?;
        match parsed.scheme() {
            "http" | "https" => Ok(()),
            scheme => Err(ErrorKind::Validation(format!("{field} must use http or https, got '{scheme}'")).into()),
        }
    }
}

/// Stripe Checkout coupon policy.
pub(crate) struct CheckoutCouponPolicy;

impl CheckoutCouponPolicy {
    pub(crate) fn ensure_not_pre_applied(coupon_code: Option<&str>) -> AppResult<()> {
        if coupon_code.map(|value| !value.trim().is_empty()).unwrap_or(false) {
            return Err(ErrorKind::Validation(
                "pre-applied coupon codes are not supported; enable promotion codes in Stripe Checkout".to_string(),
            )
            .into());
        }
        Ok(())
    }
}

/// Billing plan Stripe mapping policy.
pub(crate) struct BillingPlanPolicy;

impl BillingPlanPolicy {
    pub(crate) fn require_stripe_price_id(plan_name: &str, stripe_price_id: Option<&str>) -> AppResult<String> {
        stripe_price_id.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string).ok_or_else(|| {
            ErrorKind::Validation(format!("billing plan '{plan_name}' is not mapped to a Stripe price")).into()
        })
    }
}

/// Subscription lifecycle policy.
pub(crate) struct SubscriptionLifecyclePolicy;

impl SubscriptionLifecyclePolicy {
    pub(crate) fn ensure_no_active_subscription(existing_subscription_id: Option<Uuid>) -> AppResult<()> {
        if let Some(subscription_id) = existing_subscription_id {
            return Err(ErrorKind::Conflict(format!(
                "organization already has an active subscription: {subscription_id}"
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn require_stripe_subscription_id(value: Option<&str>) -> AppResult<&str> {
        value.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
            ErrorKind::Unavailable("active subscription is missing Stripe subscription ID".to_string()).into()
        })
    }

    pub(crate) fn require_stripe_customer_id(value: Option<&str>) -> AppResult<&str> {
        value.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
            ErrorKind::Unavailable("active subscription is missing Stripe customer ID".to_string()).into()
        })
    }
}

/// Billing usage limit policy.
pub(crate) struct BillingUsageLimitPolicy;

impl BillingUsageLimitPolicy {
    pub(crate) fn default_agent_limit() -> i64 {
        1
    }

    pub(crate) fn is_within_agent_limit(current_count: i64, max_agents: i64) -> bool {
        current_count < max_agents
    }
}

/// Subscription webhook organization reconciliation decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriptionOrgResolution {
    Resolved(Uuid),
    MissingMetadata,
}

/// Subscription webhook plan reconciliation decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriptionPlanResolution<'a> {
    Resolved(Uuid),
    LookupByStripePrice(&'a str),
    MissingMetadata,
}

/// Stripe webhook reconciliation policy for subscription snapshots.
pub(crate) struct BillingWebhookReconciliationPolicy;

impl BillingWebhookReconciliationPolicy {
    pub(crate) fn resolve_org_id(
        metadata: &BTreeMap<String, String>,
        fallback_org_id: Option<Uuid>,
        existing_org_id: Option<Uuid>,
    ) -> SubscriptionOrgResolution {
        metadata_uuid(metadata, "org_id")
            .or(fallback_org_id)
            .or(existing_org_id)
            .map(SubscriptionOrgResolution::Resolved)
            .unwrap_or(SubscriptionOrgResolution::MissingMetadata)
    }

    pub(crate) fn resolve_plan_id<'a>(
        metadata: &BTreeMap<String, String>,
        fallback_plan_id: Option<Uuid>,
        price_id: Option<&'a str>,
        existing_plan_id: Option<Uuid>,
    ) -> SubscriptionPlanResolution<'a> {
        if let Some(plan_id) = metadata_uuid(metadata, "plan_id").or(fallback_plan_id) {
            return SubscriptionPlanResolution::Resolved(plan_id);
        }

        if let Some(price_id) = price_id {
            return SubscriptionPlanResolution::LookupByStripePrice(price_id);
        }

        existing_plan_id
            .map(SubscriptionPlanResolution::Resolved)
            .unwrap_or(SubscriptionPlanResolution::MissingMetadata)
    }

    pub(crate) fn missing_org_metadata_error() -> ErrorKind {
        ErrorKind::Validation("Stripe subscription event is missing Wisdoverse Forge org_id metadata".to_string())
    }

    pub(crate) fn missing_plan_metadata_error() -> ErrorKind {
        ErrorKind::Validation("Stripe subscription event is missing Wisdoverse Forge plan metadata".to_string())
    }
}

fn metadata_uuid(metadata: &BTreeMap<String, String>, key: &str) -> Option<Uuid> {
    metadata.get(key).and_then(|value| Uuid::parse_str(value).ok())
}

/// Direct subscription PaymentMethod policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaymentMethodId<'a> {
    value: &'a str,
}

impl<'a> PaymentMethodId<'a> {
    pub(crate) fn parse(value: Option<&'a str>) -> AppResult<Self> {
        let value = value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ErrorKind::Validation("payment_method_id is required for direct subscribe".to_string()))?;
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Billing invoice list pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvoiceListPage {
    limit: i64,
    offset: i64,
}

impl InvoiceListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Subscription status policy.
pub(crate) struct SubscriptionStatusPolicy;

impl SubscriptionStatusPolicy {
    pub(crate) fn validate(status: &str) -> AppResult<()> {
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
    fn valid_statuses() -> &'static [&'static str] {
        VALID_STATUSES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn billing_cycle_accepts_monthly_and_yearly() {
        assert!(BillingCycle::parse("monthly").is_ok());
        assert!(BillingCycle::parse("yearly").is_ok());
        assert!(BillingCycle::parse("weekly").is_err());
    }

    #[test]
    fn redirect_url_requires_absolute_http_or_https() {
        assert!(BillingRedirectUrlPolicy::validate("https://example.com/success", "success_url").is_ok());
        assert!(BillingRedirectUrlPolicy::validate("http://example.com/success", "success_url").is_ok());
        assert!(BillingRedirectUrlPolicy::validate("/success", "success_url").is_err());
        assert!(BillingRedirectUrlPolicy::validate("ftp://example.com/success", "success_url").is_err());
    }

    #[test]
    fn checkout_coupon_policy_rejects_pre_applied_coupon_codes() {
        assert!(CheckoutCouponPolicy::ensure_not_pre_applied(None).is_ok());
        assert!(CheckoutCouponPolicy::ensure_not_pre_applied(Some("   ")).is_ok());
        assert!(CheckoutCouponPolicy::ensure_not_pre_applied(Some("PROMO")).is_err());
    }

    #[test]
    fn billing_plan_policy_requires_mapped_stripe_price() {
        assert_eq!(BillingPlanPolicy::require_stripe_price_id("Team", Some(" price_123 ")).unwrap(), "price_123");
        assert!(BillingPlanPolicy::require_stripe_price_id("Team", None).is_err());
        assert!(BillingPlanPolicy::require_stripe_price_id("Team", Some("   ")).is_err());
    }

    #[test]
    fn subscription_lifecycle_policy_requires_single_active_subscription_and_stripe_ids() {
        let subscription_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();

        assert!(SubscriptionLifecyclePolicy::ensure_no_active_subscription(None).is_ok());
        assert!(SubscriptionLifecyclePolicy::ensure_no_active_subscription(Some(subscription_id)).is_err());
        assert_eq!(SubscriptionLifecyclePolicy::require_stripe_subscription_id(Some(" sub_123 ")).unwrap(), "sub_123");
        assert_eq!(SubscriptionLifecyclePolicy::require_stripe_customer_id(Some(" cus_123 ")).unwrap(), "cus_123");
        assert!(SubscriptionLifecyclePolicy::require_stripe_subscription_id(None).is_err());
        assert!(SubscriptionLifecyclePolicy::require_stripe_subscription_id(Some("")).is_err());
        assert!(SubscriptionLifecyclePolicy::require_stripe_customer_id(None).is_err());
        assert!(SubscriptionLifecyclePolicy::require_stripe_customer_id(Some("   ")).is_err());
    }

    #[test]
    fn billing_usage_limit_policy_uses_strict_agent_limit() {
        assert_eq!(BillingUsageLimitPolicy::default_agent_limit(), 1);
        assert!(BillingUsageLimitPolicy::is_within_agent_limit(0, 1));
        assert!(!BillingUsageLimitPolicy::is_within_agent_limit(1, 1));
        assert!(!BillingUsageLimitPolicy::is_within_agent_limit(2, 2));
    }

    #[test]
    fn webhook_reconciliation_resolves_subscription_org() {
        let metadata_org_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let fallback_org_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let existing_org_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let mut metadata = BTreeMap::new();
        metadata.insert("org_id".to_string(), metadata_org_id.to_string());

        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_org_id(&metadata, Some(fallback_org_id), Some(existing_org_id),),
            SubscriptionOrgResolution::Resolved(metadata_org_id)
        );

        metadata.clear();
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_org_id(&metadata, Some(fallback_org_id), Some(existing_org_id),),
            SubscriptionOrgResolution::Resolved(fallback_org_id)
        );
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_org_id(&metadata, None, Some(existing_org_id)),
            SubscriptionOrgResolution::Resolved(existing_org_id)
        );
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_org_id(&metadata, None, None),
            SubscriptionOrgResolution::MissingMetadata
        );
    }

    #[test]
    fn webhook_reconciliation_resolves_subscription_plan() {
        let metadata_plan_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let fallback_plan_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let existing_plan_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let mut metadata = BTreeMap::new();
        metadata.insert("plan_id".to_string(), metadata_plan_id.to_string());

        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_plan_id(
                &metadata,
                Some(fallback_plan_id),
                Some("price_123"),
                Some(existing_plan_id),
            ),
            SubscriptionPlanResolution::Resolved(metadata_plan_id)
        );

        metadata.clear();
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_plan_id(
                &metadata,
                Some(fallback_plan_id),
                Some("price_123"),
                Some(existing_plan_id),
            ),
            SubscriptionPlanResolution::Resolved(fallback_plan_id)
        );
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_plan_id(
                &metadata,
                None,
                Some("price_123"),
                Some(existing_plan_id),
            ),
            SubscriptionPlanResolution::LookupByStripePrice("price_123")
        );
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_plan_id(&metadata, None, None, Some(existing_plan_id)),
            SubscriptionPlanResolution::Resolved(existing_plan_id)
        );
        assert_eq!(
            BillingWebhookReconciliationPolicy::resolve_plan_id(&metadata, None, None, None),
            SubscriptionPlanResolution::MissingMetadata
        );
    }

    #[test]
    fn payment_method_id_trims_and_requires_value() {
        assert_eq!(PaymentMethodId::parse(Some(" pm_123 ")).unwrap().value(), "pm_123");
        assert!(PaymentMethodId::parse(None).is_err());
        assert!(PaymentMethodId::parse(Some("")).is_err());
        assert!(PaymentMethodId::parse(Some("   ")).is_err());
    }

    #[test]
    fn invoice_list_page_clamps_bounds() {
        let low = InvoiceListPage::new(0, -10);
        assert_eq!(low.limit(), 1);
        assert_eq!(low.offset(), 0);

        let high = InvoiceListPage::new(500, 10);
        assert_eq!(high.limit(), 100);
        assert_eq!(high.offset(), 10);
    }

    #[test]
    fn subscription_status_policy_accepts_valid_statuses() {
        assert!(SubscriptionStatusPolicy::validate("active").is_ok());
        assert!(SubscriptionStatusPolicy::validate("past_due").is_ok());
        assert!(SubscriptionStatusPolicy::validate("canceled").is_ok());
        assert!(SubscriptionStatusPolicy::validate("trialing").is_ok());
        assert!(SubscriptionStatusPolicy::validate("expired").is_err());
        assert!(SubscriptionStatusPolicy::validate("").is_err());
        assert!(SubscriptionStatusPolicy::validate("ACTIVE").is_err());
    }

    #[test]
    fn valid_statuses_list_is_complete() {
        let statuses = SubscriptionStatusPolicy::valid_statuses();

        assert_eq!(statuses.len(), 8);
        assert!(statuses.contains(&"active"));
        assert!(statuses.contains(&"past_due"));
        assert!(statuses.contains(&"canceled"));
        assert!(statuses.contains(&"trialing"));
        assert!(statuses.contains(&"unpaid"));
        assert!(statuses.contains(&"incomplete"));
        assert!(statuses.contains(&"incomplete_expired"));
        assert!(statuses.contains(&"paused"));
    }
}
