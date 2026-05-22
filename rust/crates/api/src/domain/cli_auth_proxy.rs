//! CLI auth proxy response shapes.

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CallbackMode {
    Manual,
    Server,
}

/// Public projection for the status endpoint — never leaks tokens.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub provider: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "cliTool")]
    pub cli_tool: String,
    pub connected: bool,
    #[serde(rename = "lastRefresh")]
    pub last_refresh: Option<String>,
    #[serde(rename = "callbackMode")]
    pub callback_mode: CallbackMode,
    /// RFC 3339 timestamp — present only when the row has been revoked.
    /// Frontend renders a "re-auth needed" banner when set.
    #[serde(rename = "revokedAt", skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    /// Human-readable reason. Matches the OAuth error code when revoked by the
    /// refresh worker (`"invalid_grant"`), or a local availability reason when
    /// the stored row exists but cannot be used by a new agent container.
    #[serde(rename = "revokeReason", skip_serializing_if = "Option::is_none")]
    pub revoke_reason: Option<String>,
    /// Consecutive refresh failures since the last success. Values below the
    /// revoke threshold signal the row will be revoked on the next
    /// consecutive failure — surfaced for ops visibility.
    #[serde(rename = "refreshFailCount", skip_serializing_if = "is_zero_i32")]
    pub refresh_fail_count: i32,
}

fn is_zero_i32(n: &i32) -> bool {
    *n == 0
}

/// Public projection for the providers list endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "cliTool")]
    pub cli_tool: String,
    #[serde(rename = "callbackMode")]
    pub callback_mode: CallbackMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokedCliCredential {
    pub user_id: Uuid,
    pub cli_tool: String,
    pub reason: String,
    pub revoked_at: chrono::DateTime<chrono::Utc>,
}

/// Aggregate result of one `refresh_stale` sweep. Logged by the worker loop.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RefreshSummary {
    pub refreshed: usize,
    pub failed: usize,
    /// Entries that were old enough to attempt refresh (sum of refreshed +
    /// failed + invalid_grant + invalid_client).
    pub eligible: usize,
    /// Entries where the IdP returned `invalid_grant` — bumped the fail
    /// counter, possibly revoked the row.
    pub invalid_grant: usize,
    /// Entries where the IdP returned `invalid_client` / `unauthorized_client`
    /// — operator-level signal; user row not touched.
    pub invalid_client: usize,
    /// Credentials revoked in this sweep; the server worker turns these into
    /// owner-scoped Inbox notifications.
    #[serde(rename = "revokedCredentials")]
    pub revoked_credentials: Vec<RevokedCliCredential>,
}

pub(crate) fn cli_auth_providers_response<T: Serialize>(providers: T) -> Value {
    json!({ "ok": true, "providers": providers })
}

pub(crate) fn cli_auth_statuses_response<T: Serialize>(statuses: T) -> Value {
    json!({ "ok": true, "statuses": statuses })
}

pub(crate) fn cli_auth_authorize_response(url: String) -> Value {
    json!({ "ok": true, "url": url })
}

pub(crate) fn cli_auth_connected_response(provider: &str) -> Value {
    json!({ "ok": true, "provider": provider, "status": "connected" })
}

pub(crate) fn cli_auth_disconnected_response(provider: &str) -> Value {
    json!({ "ok": true, "provider": provider, "status": "disconnected" })
}

/// Raw token response from a provider token endpoint.
///
/// Secret token fields use [`SecretString`] so accidental debug output redacts
/// OAuth material before service code encrypts it.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct TokenResponse {
    pub(crate) id_token: Option<SecretString>,
    pub(crate) access_token: SecretString,
    pub(crate) refresh_token: Option<SecretString>,
    #[allow(dead_code)]
    pub(crate) expires_in: Option<u64>,
    pub(crate) account_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshErrorKind {
    /// Refresh token rejected — user must re-authenticate.
    InvalidGrant,
    /// OAuth app-level rejection — operator must investigate.
    InvalidClient,
    /// Other RFC 6749 error code (e.g. `invalid_scope`). Log + metric only.
    OtherOauthError(String),
    /// 5xx, network failure, non-JSON body on 4xx, or unknown code. Retry.
    Transient(String),
}

pub(crate) struct RefreshFailureClassifier;

impl RefreshFailureClassifier {
    pub(crate) fn classify(status_code: u16, body: &str) -> RefreshErrorKind {
        if (500..=599).contains(&status_code) {
            return RefreshErrorKind::Transient(format!("HTTP {status_code}"));
        }

        let error_code = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
        match error_code.as_deref() {
            Some("invalid_grant") => RefreshErrorKind::InvalidGrant,
            Some("invalid_client") | Some("unauthorized_client") => RefreshErrorKind::InvalidClient,
            Some(other) => RefreshErrorKind::OtherOauthError(other.to_string()),
            None => RefreshErrorKind::Transient(format!("HTTP {status_code}: {}", truncate(body, 200))),
        }
    }
}

/// Truncate to at most `max_chars` characters, appending `...` if truncated.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() { format!("{head}...") } else { head }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CliAuthTokenFileInput<'a> {
    pub(crate) id_token: Option<&'a str>,
    pub(crate) access_token: &'a str,
    pub(crate) refresh_token: Option<&'a str>,
    pub(crate) account_id: Option<&'a str>,
    pub(crate) last_refresh: DateTime<Utc>,
}

pub(crate) fn cli_auth_token_file_map(input: CliAuthTokenFileInput<'_>) -> Value {
    let mut auth_tokens = serde_json::Map::new();
    if let Some(id_token) = input.id_token {
        auth_tokens.insert("id_token".into(), Value::String(id_token.to_string()));
    }
    auth_tokens.insert("access_token".into(), Value::String(input.access_token.to_string()));
    if let Some(refresh_token) = input.refresh_token {
        auth_tokens.insert("refresh_token".into(), Value::String(refresh_token.to_string()));
    }
    if let Some(account_id) = input.account_id {
        auth_tokens.insert("account_id".into(), Value::String(account_id.to_string()));
    }

    let auth_json = json!({
        "tokens": auth_tokens,
        "last_refresh": input.last_refresh.to_rfc3339(),
    });
    json!({ "auth.json": auth_json.to_string() })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use secrecy::SecretString;

    use super::*;

    #[test]
    fn token_file_map_owns_auth_json_shape() {
        let files = cli_auth_token_file_map(CliAuthTokenFileInput {
            id_token: Some("id"),
            access_token: "access",
            refresh_token: Some("refresh"),
            account_id: Some("acct"),
            last_refresh: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        });

        let auth_json = files["auth.json"].as_str().expect("auth.json string");
        let auth: Value = serde_json::from_str(auth_json).expect("auth json");
        assert_eq!(auth["tokens"]["id_token"], "id");
        assert_eq!(auth["tokens"]["access_token"], "access");
        assert_eq!(auth["tokens"]["refresh_token"], "refresh");
        assert_eq!(auth["tokens"]["account_id"], "acct");
        assert_eq!(auth["last_refresh"], "2026-04-01T00:00:00+00:00");
    }

    #[test]
    fn token_response_debug_redacts_secret_fields() {
        let tokens = TokenResponse {
            id_token: Some(SecretString::from("id-supersecret".to_string())),
            access_token: SecretString::from("at-supersecret".to_string()),
            refresh_token: Some(SecretString::from("rt-supersecret".to_string())),
            expires_in: Some(3600),
            account_id: Some("acct-public".to_string()),
        };
        let dbg = format!("{tokens:?}");

        for needle in ["id-supersecret", "at-supersecret", "rt-supersecret"] {
            assert!(!dbg.contains(needle), "Debug leaked {needle:?}: {dbg}");
        }
        assert!(dbg.contains("acct-public"), "account_id should remain visible: {dbg}");
    }

    #[test]
    fn refresh_failure_classifier_maps_oauth_error_codes() {
        assert_eq!(
            RefreshFailureClassifier::classify(400, r#"{"error":"invalid_grant"}"#),
            RefreshErrorKind::InvalidGrant
        );
        assert_eq!(
            RefreshFailureClassifier::classify(401, r#"{"error":"invalid_client"}"#),
            RefreshErrorKind::InvalidClient
        );
        assert_eq!(
            RefreshFailureClassifier::classify(400, r#"{"error":"unauthorized_client"}"#),
            RefreshErrorKind::InvalidClient
        );
        assert_eq!(
            RefreshFailureClassifier::classify(400, r#"{"error":"invalid_scope"}"#),
            RefreshErrorKind::OtherOauthError("invalid_scope".into())
        );
    }

    #[test]
    fn refresh_failure_classifier_treats_non_json_or_5xx_as_transient() {
        assert_eq!(
            RefreshFailureClassifier::classify(503, "upstream down"),
            RefreshErrorKind::Transient("HTTP 503".into())
        );
        match RefreshFailureClassifier::classify(400, "<html><body>bad gateway</body></html>") {
            RefreshErrorKind::Transient(message) => assert!(message.contains("HTTP 400")),
            other => panic!("expected transient classification, got {other:?}"),
        }
    }
}
