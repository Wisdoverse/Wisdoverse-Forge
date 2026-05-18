//! CLI auth proxy response shapes.

use serde::Serialize;
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
