//! CLI auth proxy response shapes.

use agentforge_core::{AppResult, ErrorKind};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::Serialize;
use serde::de::DeserializeOwned;
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

pub(crate) fn cli_auth_callback_success_html() -> String {
    "<html><body><h1>Signed in</h1><p>You can close this tab and return to Wisdoverse Forge.</p></body></html>"
        .to_string()
}

pub(crate) fn cli_auth_callback_idp_error_html(error: &str, description: &str) -> String {
    format!(
        "<html><body><h1>Sign-in failed</h1><p>{}</p><p>{}</p><p>You can close this tab.</p></body></html>",
        html_escape(error),
        html_escape(description),
    )
}

pub(crate) fn cli_auth_callback_missing_params_html() -> &'static str {
    "<html><body><h1>Sign-in failed</h1><p>Callback missing <code>code</code> or <code>state</code>.</p></body></html>"
}

pub(crate) fn cli_auth_callback_service_error_html(kind: &ErrorKind) -> String {
    let message = match kind {
        ErrorKind::Validation(message) | ErrorKind::Unprocessable(message) => message.as_str(),
        _ => "internal error",
    };
    format!("<html><body><h1>Sign-in failed</h1><p>{}</p></body></html>", html_escape(message))
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) struct CliAuthProxyPolicy;

impl CliAuthProxyPolicy {
    pub(crate) fn missing_refresh_storage_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured".into())
    }

    pub(crate) fn missing_token_storage_key() -> ErrorKind {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured — refusing to store plaintext tokens".into())
    }

    pub(crate) fn unknown_provider(name: &str) -> ErrorKind {
        ErrorKind::Validation(format!("unknown Container CLI auth proxy provider: {name}"))
    }

    pub(crate) fn invalid_manual_callback_input() -> ErrorKind {
        ErrorKind::Validation("could not parse authorization code from input. Paste the full callback URL.".into())
    }

    pub(crate) fn invalid_or_expired_manual_state() -> ErrorKind {
        ErrorKind::Validation("invalid or expired OAuth state — re-run authorize".into())
    }

    pub(crate) fn invalid_or_expired_state() -> ErrorKind {
        ErrorKind::Validation("invalid or expired OAuth state".into())
    }

    pub(crate) fn provider_mismatch(stored: &str, requested: &str) -> ErrorKind {
        ErrorKind::Validation(format!("provider mismatch: stored {stored} vs requested {requested}"))
    }

    pub(crate) fn state_user_mismatch() -> ErrorKind {
        ErrorKind::Validation("OAuth state belongs to a different user".into())
    }

    pub(crate) fn token_exchange_failed(status: impl std::fmt::Display, body: &str) -> ErrorKind {
        ErrorKind::Validation(format!("token exchange failed: HTTP {status} — {body}"))
    }

    pub(crate) fn urlencode_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("urlencode: {err}"))
    }

    pub(crate) fn refresh_invalid_json(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("refresh invalid JSON: {err}"))
    }

    pub(crate) fn token_exchange_request_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("token exchange request failed: {err}"))
    }

    pub(crate) fn token_exchange_invalid_json(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("token exchange returned invalid JSON: {err}"))
    }

    pub(crate) fn encrypt_refreshed_tokens_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("encrypt refreshed tokens: {err}"))
    }

    pub(crate) fn encrypt_tokens_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("encrypt tokens: {err}"))
    }

    pub(crate) fn decrypt_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("decrypt: {err}"))
    }

    pub(crate) fn files_json_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("files JSON: {err}"))
    }

    pub(crate) fn auth_json_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("auth.json: {err}"))
    }

    pub(crate) fn redis_connection_unavailable() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Redis connection unavailable"))
    }

    pub(crate) fn redis_set_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Redis SET_EX failed: {err}"))
    }

    pub(crate) fn redis_getdel_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Redis GETDEL failed: {err}"))
    }

    pub(crate) fn state_entry_serialize_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("serialize StateEntry for Redis: {err}"))
    }

    pub(crate) fn state_entry_deserialize_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("corrupt StateEntry in Redis: {err}"))
    }
}

/// Legacy TS supported three paste formats; we match verbatim so UI hints
/// remain identical:
/// - Full callback URL
/// - `code#state` (Codex CLI's own shortcut format)
/// - Bare query string
pub(crate) fn parse_callback_input(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Format 1: URL with query.
    if let Ok(url) = url::Url::parse(trimmed) {
        let mut code = None;
        let mut state = None;
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                _ => {}
            }
        }
        if let (Some(c), Some(s)) = (code, state) {
            return Some((c, s));
        }
    }
    // Format 2: code#state (no `=` means it's not a query string).
    if trimmed.contains('#')
        && !trimmed.contains('=')
        && let Some((code, state)) = trimmed.split_once('#')
        && !code.is_empty()
        && !state.is_empty()
    {
        return Some((code.to_string(), state.to_string()));
    }
    // Format 3: bare query string. Percent-decode via `form_urlencoded` so a
    // code like `abc%2Bdef` survives token exchange.
    let qs = trimmed.strip_prefix('?').unwrap_or(trimmed);
    let mut code = None;
    let mut state = None;
    for (k, v) in url::form_urlencoded::parse(qs.as_bytes()) {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            _ => {}
        }
    }
    if let (Some(c), Some(s)) = (code, state) {
        return Some((c, s));
    }
    None
}

/// Extract `chatgpt_account_id` from the `https://api.openai.com/auth` claim
/// of an OpenAI access-token JWT. Returns `None` if the token is malformed or
/// missing the claim.
pub(crate) fn extract_chatgpt_account_id(access_token: &str) -> Option<String> {
    let parts: Vec<&str> = access_token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload_b64 = parts[1];
    let payload = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(|claim| claim.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
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

pub(crate) fn cli_auth_authorize_url(auth_endpoint: &str, params: &[(&str, &str)]) -> AppResult<String> {
    let query = serde_urlencoded::to_string(params).map_err(CliAuthProxyPolicy::urlencode_failed)?;
    Ok(format!("{auth_endpoint}?{query}"))
}

pub(crate) fn cli_auth_token_file_payload(input: CliAuthTokenFileInput<'_>) -> String {
    cli_auth_token_file_map(input).to_string()
}

pub(crate) fn cli_auth_token_files_from_plain(plain: &str) -> AppResult<Value> {
    serde_json::from_str(plain).map_err(|err| CliAuthProxyPolicy::files_json_failed(err).into())
}

pub(crate) fn cli_auth_auth_json_from_str(auth_json: &str) -> AppResult<Value> {
    serde_json::from_str(auth_json).map_err(|err| CliAuthProxyPolicy::auth_json_failed(err).into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAuthCredentialPayloadRead {
    Usable { last_refresh: Option<String> },
    InvalidPayload { error: String },
}

pub(crate) fn cli_auth_credential_payload_from_plain(plain: &str) -> CliAuthCredentialPayloadRead {
    let files = match serde_json::from_str::<Value>(plain) {
        Ok(files) => files,
        Err(err) => return CliAuthCredentialPayloadRead::InvalidPayload { error: err.to_string() },
    };

    let last_refresh = files
        .get("auth.json")
        .and_then(Value::as_str)
        .and_then(|auth_json| serde_json::from_str::<Value>(auth_json).ok())
        .and_then(|auth| auth.get("last_refresh").and_then(Value::as_str).map(str::to_string));

    CliAuthCredentialPayloadRead::Usable { last_refresh }
}

pub(crate) fn cli_auth_credential_payload_invalid_reason() -> &'static str {
    "credential_payload_invalid"
}

pub(crate) fn cli_auth_credential_decrypt_failed_reason() -> &'static str {
    "credential_decrypt_failed"
}

pub(crate) fn cli_auth_encryption_key_missing_reason() -> &'static str {
    "encryption_key_missing"
}

pub(crate) fn cli_auth_state_entry_payload<T: Serialize>(entry: &T) -> AppResult<String> {
    serde_json::to_string(entry).map_err(|err| CliAuthProxyPolicy::state_entry_serialize_failed(err).into())
}

pub(crate) fn cli_auth_state_entry_from_payload<T: DeserializeOwned>(payload: &str) -> AppResult<T> {
    serde_json::from_str(payload).map_err(|err| CliAuthProxyPolicy::state_entry_deserialize_failed(err).into())
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
    fn token_file_payload_serializes_file_map() {
        let payload = cli_auth_token_file_payload(CliAuthTokenFileInput {
            id_token: None,
            access_token: "access",
            refresh_token: Some("refresh"),
            account_id: None,
            last_refresh: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        });

        let files: Value = serde_json::from_str(&payload).expect("files JSON");
        let auth_json = files["auth.json"].as_str().expect("auth.json string");
        let auth: Value = serde_json::from_str(auth_json).expect("auth JSON");

        assert_eq!(auth["tokens"]["access_token"], "access");
        assert_eq!(auth["tokens"]["refresh_token"], "refresh");
        assert!(auth["tokens"].get("id_token").is_none());
    }

    #[test]
    fn authorize_url_owns_query_serialization() {
        let url = cli_auth_authorize_url(
            "https://auth.example.test/oauth/authorize",
            &[
                ("response_type", "code"),
                ("client_id", "client one"),
                ("redirect_uri", "http://localhost:1455/auth/callback"),
                ("scope", "openid profile"),
            ],
        )
        .unwrap();

        assert_eq!(
            url,
            "https://auth.example.test/oauth/authorize?response_type=code&client_id=client+one&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&scope=openid+profile"
        );
    }

    #[test]
    fn callback_html_helpers_escape_user_controlled_text() {
        assert!(cli_auth_callback_success_html().contains("Signed in"));
        assert!(cli_auth_callback_missing_params_html().contains("Callback missing"));

        let idp = cli_auth_callback_idp_error_html("<denied>", "\"bad\"");
        assert!(idp.contains("&lt;denied&gt;"));
        assert!(idp.contains("&quot;bad&quot;"));

        let service = cli_auth_callback_service_error_html(&ErrorKind::Validation("<bad state>".to_string()));
        assert!(service.contains("&lt;bad state&gt;"));

        let internal = cli_auth_callback_service_error_html(&ErrorKind::Internal(anyhow::anyhow!("secret")));
        assert!(internal.contains("internal error"));
        assert!(!internal.contains("secret"));
    }

    #[test]
    fn stored_token_file_parsers_map_invalid_json_to_internal() {
        let files = cli_auth_token_files_from_plain(r#"{"auth.json":"{\"tokens\":{\"refresh_token\":\"rt\"}}"}"#)
            .expect("files");
        assert_eq!(files["auth.json"].as_str().unwrap(), r#"{"tokens":{"refresh_token":"rt"}}"#);

        let auth = cli_auth_auth_json_from_str(r#"{"tokens":{"refresh_token":"rt"}}"#).expect("auth");
        assert_eq!(auth["tokens"]["refresh_token"], "rt");

        assert!(format!("{}", cli_auth_token_files_from_plain("not-json").unwrap_err().kind).contains("files JSON"));
        assert!(format!("{}", cli_auth_auth_json_from_str("not-json").unwrap_err().kind).contains("auth.json"));
    }

    #[test]
    fn credential_payload_status_extracts_last_refresh_from_auth_json() {
        let payload = cli_auth_token_file_payload(CliAuthTokenFileInput {
            id_token: None,
            access_token: "access",
            refresh_token: Some("refresh"),
            account_id: None,
            last_refresh: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        });

        assert_eq!(
            cli_auth_credential_payload_from_plain(&payload),
            CliAuthCredentialPayloadRead::Usable { last_refresh: Some("2026-04-01T00:00:00+00:00".to_string()) }
        );
    }

    #[test]
    fn credential_payload_status_preserves_legacy_usable_edge_cases() {
        assert_eq!(
            cli_auth_credential_payload_from_plain(r#"{"other.json":"{}"}"#),
            CliAuthCredentialPayloadRead::Usable { last_refresh: None }
        );
        assert_eq!(
            cli_auth_credential_payload_from_plain(r#"{"auth.json":"not-json"}"#),
            CliAuthCredentialPayloadRead::Usable { last_refresh: None }
        );

        match cli_auth_credential_payload_from_plain("not-json") {
            CliAuthCredentialPayloadRead::InvalidPayload { error } => assert!(!error.is_empty()),
            other => panic!("expected invalid payload, got {other:?}"),
        }
    }

    #[test]
    fn credential_payload_status_reasons_are_domain_owned() {
        assert_eq!(cli_auth_credential_payload_invalid_reason(), "credential_payload_invalid");
        assert_eq!(cli_auth_credential_decrypt_failed_reason(), "credential_decrypt_failed");
        assert_eq!(cli_auth_encryption_key_missing_reason(), "encryption_key_missing");
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    struct TestStateEntry {
        provider: String,
        user_id: Uuid,
    }

    #[test]
    fn state_entry_payload_helpers_own_redis_json_contract() {
        let entry = TestStateEntry { provider: "openai".to_string(), user_id: Uuid::nil() };
        let payload = cli_auth_state_entry_payload(&entry).expect("state entry serializes");
        let parsed: TestStateEntry = cli_auth_state_entry_from_payload(&payload).expect("state entry deserializes");

        assert_eq!(parsed, entry);
        assert!(
            format!("{}", cli_auth_state_entry_from_payload::<TestStateEntry>("not-json").unwrap_err().kind)
                .contains("corrupt StateEntry")
        );
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
    fn cli_auth_proxy_policy_owns_storage_and_provider_errors() {
        assert!(format!("{}", CliAuthProxyPolicy::missing_refresh_storage_key()).contains("LLM_ENCRYPTION_KEY"));
        assert!(format!("{}", CliAuthProxyPolicy::missing_token_storage_key()).contains("plaintext tokens"));
        assert!(
            format!("{}", CliAuthProxyPolicy::unknown_provider("openai"))
                .contains("unknown Container CLI auth proxy provider")
        );
    }

    #[test]
    fn cli_auth_proxy_policy_owns_oauth_flow_errors() {
        assert!(format!("{}", CliAuthProxyPolicy::invalid_manual_callback_input()).contains("full callback URL"));
        assert!(format!("{}", CliAuthProxyPolicy::invalid_or_expired_manual_state()).contains("re-run authorize"));
        assert!(
            format!("{}", CliAuthProxyPolicy::invalid_or_expired_state()).contains("invalid or expired OAuth state")
        );
        assert!(format!("{}", CliAuthProxyPolicy::provider_mismatch("openai", "codex")).contains("stored openai"));
        assert!(format!("{}", CliAuthProxyPolicy::state_user_mismatch()).contains("different user"));
        assert!(format!("{}", CliAuthProxyPolicy::token_exchange_failed(400, "bad")).contains("HTTP 400"));
    }

    #[test]
    fn cli_auth_proxy_policy_owns_serialization_errors() {
        for err in [
            CliAuthProxyPolicy::urlencode_failed("bad"),
            CliAuthProxyPolicy::refresh_invalid_json("bad"),
            CliAuthProxyPolicy::token_exchange_request_failed("bad"),
            CliAuthProxyPolicy::token_exchange_invalid_json("bad"),
            CliAuthProxyPolicy::encrypt_refreshed_tokens_failed("bad"),
            CliAuthProxyPolicy::encrypt_tokens_failed("bad"),
            CliAuthProxyPolicy::decrypt_failed("bad"),
            CliAuthProxyPolicy::files_json_failed("bad"),
            CliAuthProxyPolicy::auth_json_failed("bad"),
            CliAuthProxyPolicy::redis_connection_unavailable(),
            CliAuthProxyPolicy::redis_set_failed("bad"),
            CliAuthProxyPolicy::redis_getdel_failed("bad"),
            CliAuthProxyPolicy::state_entry_serialize_failed("bad"),
            CliAuthProxyPolicy::state_entry_deserialize_failed("bad"),
        ] {
            assert!(!format!("{err}").is_empty());
        }
    }

    #[test]
    fn parse_callback_input_accepts_legacy_formats() {
        assert_eq!(
            parse_callback_input("http://localhost:1455/auth/callback?code=abc&state=xyz"),
            Some(("abc".to_string(), "xyz".to_string()))
        );
        assert_eq!(parse_callback_input("abc#xyz"), Some(("abc".to_string(), "xyz".to_string())));
        assert_eq!(parse_callback_input("?code=abc%2Bdef&state=xyz"), Some(("abc+def".to_string(), "xyz".to_string())));
        assert_eq!(parse_callback_input("not a callback"), None);
    }

    #[test]
    fn extract_chatgpt_account_id_reads_openai_auth_claim() {
        let payload = serde_json::json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_123"
            }
        });
        let encoded = URL_SAFE_NO_PAD.encode(payload.to_string());
        assert_eq!(extract_chatgpt_account_id(&format!("header.{encoded}.sig")), Some("acct_123".to_string()));
        assert_eq!(extract_chatgpt_account_id("malformed"), None);
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
