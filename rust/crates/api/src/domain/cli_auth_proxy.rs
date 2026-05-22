//! CLI auth proxy response shapes.

use chrono::{DateTime, Utc};
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
}
