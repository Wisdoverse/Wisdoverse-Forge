//! Billing domain rules.
//!
//! This module owns pure billing request and lifecycle policies that are
//! independent of repositories, Stripe clients, and HTTP route DTOs.

use agentforge_core::{AppResult, ErrorKind};

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
