//! SSO (OpenID Connect) domain rules: provider identity, state/one-time-code
//! policy, and the plain-language errors surfaced on the login page.
//!
//! The OIDC network calls (discovery, token exchange, userinfo) live in the
//! service; this module stays pure so the policy is unit-testable without a
//! provider.

use agentforge_core::{AppError, ErrorKind};
use serde::Serialize;

/// Public description of one configured sign-in provider (login-page button).
#[derive(Debug, Clone, Serialize)]
pub struct SsoProvider {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

/// Policy errors for the SSO flow. All messages are written for the person at
/// the login page (or the operator reading server logs), not for a stack trace.
pub struct SsoPolicy;

impl SsoPolicy {
    pub fn not_configured() -> ErrorKind {
        ErrorKind::Validation("Single sign-on is not configured for this instance".into())
    }

    pub fn discovery_failed(detail: impl Into<String>) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Could not reach the single sign-on provider settings: {}", detail.into()))
    }

    pub fn invalid_state() -> ErrorKind {
        ErrorKind::Validation("This sign-in link has expired. Start sign-in again from the login page.".into())
    }

    pub fn authorization_failed(detail: impl Into<String>) -> ErrorKind {
        ErrorKind::Validation(format!("The single sign-on provider did not sign you in: {}", detail.into()))
    }

    pub fn missing_email() -> ErrorKind {
        ErrorKind::Validation(
            "Your sign-in provider did not return an email address, so Forge cannot create your account here.".into(),
        )
    }

    pub fn invalid_exchange_code() -> ErrorKind {
        ErrorKind::Validation("This sign-in link has expired. Start sign-in again from the login page.".into())
    }

    pub fn sso_unavailable(reason: &str) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Single sign-on is not available right now: {reason}"))
    }

    pub fn into_app_error(kind: ErrorKind) -> AppError {
        kind.into()
    }

    pub fn deprovision_not_configured() -> ErrorKind {
        ErrorKind::NotFound("Instant-off deprovisioning is not configured on this server.".to_string())
    }

    pub fn ct_equal(a: &[u8], b: &[u8]) -> bool {
        let mut diff = a.len() ^ b.len();
        for i in 0..a.len().max(b.len()) {
            let x = a.get(i).copied().unwrap_or(0);
            let y = b.get(i).copied().unwrap_or(0);
            diff |= (x ^ y) as usize;
        }
        diff == 0
    }

    /// User-facing message for a provider error on the login page.
    pub fn start_over_message(err: &AppError) -> String {
        match &err.kind {
            ErrorKind::Validation(message) => message.clone(),
            ErrorKind::ValidationWithCode { message, .. } => message.clone(),
            ErrorKind::Internal(_) => "Single sign-on is not available right now. Start sign-in again.".to_string(),
            _ => "Start sign-in again.".to_string(),
        }
    }

    /// Classify a guard failure without exposing ErrorKind variants.
    pub fn deprovision_guard_state(err: &AppError) -> DeprovisionGuardState {
        if matches!(&err.kind, ErrorKind::NotFound(_)) {
            DeprovisionGuardState::Unconfigured
        } else {
            DeprovisionGuardState::Unauthorized
        }
    }

    /// Response body for POST /auth/deprovision.
    pub fn deprovision_response(user_found: bool, removed_memberships: usize) -> serde_json::Value {
        serde_json::json!({ "ok": true, "userFound": user_found, "removedMemberships": removed_memberships })
    }

    /// Response body for POST /auth/sso/provision.
    pub fn provision_response(user_id: uuid::Uuid) -> serde_json::Value {
        serde_json::json!({ "ok": true, "userId": user_id })
    }
}

/// SSO state cookie name shared by authorize/callback handlers.
pub(crate) const SSO_STATE_COOKIE_NAME: &str = "af_sso_state";

/// Classification of a deprovision-webhook guard failure (route-proof:
/// handlers build HTTP status from this instead of matching ErrorKind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeprovisionGuardState {
    /// `AUTH_SSO__DEPROVISION_TOKEN` is not set — the endpoints are off.
    Unconfigured,
    /// The shared-secret header did not match.
    Unauthorized,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_messages_stay_plain_language() {
        assert!(SsoPolicy::invalid_state().to_string().contains("login page"));
        assert!(SsoPolicy::missing_email().to_string().contains("email address"));
        let discovery = SsoPolicy::discovery_failed("connection refused").to_string();
        assert!(discovery.contains("single sign-on provider settings"), "got: {discovery}");
    }

    #[test]
    fn ct_equal_matches_without_early_exit() {
        assert!(SsoPolicy::ct_equal(b"token-123", b"token-123"));
        assert!(!SsoPolicy::ct_equal(b"token-123", b"token-124"));
        assert!(!SsoPolicy::ct_equal(b"ab", b"abc"));
        assert!(!SsoPolicy::ct_equal(b"", b"x"));
    }

    #[test]
    fn provider_serializes_for_the_login_page() {
        let provider = SsoProvider { name: "oidc".into(), display_name: "Single sign-on".into() };
        let value = serde_json::to_value(provider).expect("provider serializes");
        assert_eq!(value["name"], "oidc");
        assert_eq!(value["displayName"], "Single sign-on");
    }
}
