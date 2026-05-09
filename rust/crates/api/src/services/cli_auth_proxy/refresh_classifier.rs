//! Maps IdP refresh-token responses to our internal error taxonomy.
//!
//! RFC 6749 §5.2 defines `error` codes returned in a JSON body on 4xx
//! responses when the grant is rejected. We care about three buckets:
//!
//! - `invalid_grant` — the user's refresh token is revoked/expired/reused.
//!   Bump the per-row fail counter; at threshold, flip `revoked_at` so the
//!   next container spawn will prompt re-auth.
//! - `invalid_client` / `unauthorized_client` — the operator's OAuth *app*
//!   is wrong (rotated secret, deleted app, wrong client_id). Never a user
//!   problem; page the operator and leave user rows alone.
//! - Anything else (network error, 5xx, non-JSON 4xx body, unknown error
//!   code) — transient. Retry next sweep; don't touch user state.

use reqwest::Response;

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

pub async fn classify_refresh_failure(resp: Response) -> RefreshErrorKind {
    let status = resp.status();
    if status.is_server_error() {
        return RefreshErrorKind::Transient(format!("HTTP {status}"));
    }
    // 4xx — parse body for RFC 6749 error code.
    let body = resp.text().await.unwrap_or_default();
    let error_code = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
    match error_code.as_deref() {
        Some("invalid_grant") => RefreshErrorKind::InvalidGrant,
        Some("invalid_client") | Some("unauthorized_client") => RefreshErrorKind::InvalidClient,
        Some(other) => RefreshErrorKind::OtherOauthError(other.to_string()),
        // Body not JSON or missing `error` key — could be IdP HTML error
        // page, WAF 403, etc. Defensive: treat as transient, not revoke.
        None => RefreshErrorKind::Transient(format!("HTTP {status}: {}", truncate(&body, 200))),
    }
}

/// Truncate to at most `max_chars` characters, appending `…` if truncated.
///
/// Operates on characters rather than bytes because the IdP body can be any
/// UTF-8 — a byte-indexed `&s[..max]` would panic if the cutoff lands inside
/// a multibyte codepoint (non-ASCII WAF pages, localised IdP error messages).
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with(status: u16, body: &str) -> Response {
        Response::from(http::Response::builder().status(status).body(body.to_owned()).unwrap())
    }

    #[tokio::test]
    async fn invalid_grant_body_classifies_as_invalid_grant() {
        let resp = response_with(400, r#"{"error":"invalid_grant","error_description":"expired"}"#);
        assert_eq!(classify_refresh_failure(resp).await, RefreshErrorKind::InvalidGrant);
    }

    #[tokio::test]
    async fn invalid_client_body_classifies_as_invalid_client() {
        let resp = response_with(401, r#"{"error":"invalid_client"}"#);
        assert_eq!(classify_refresh_failure(resp).await, RefreshErrorKind::InvalidClient);
    }

    #[tokio::test]
    async fn unauthorized_client_treated_same_as_invalid_client() {
        let resp = response_with(400, r#"{"error":"unauthorized_client"}"#);
        assert_eq!(classify_refresh_failure(resp).await, RefreshErrorKind::InvalidClient);
    }

    #[tokio::test]
    async fn server_error_is_transient() {
        let resp = response_with(503, "upstream down");
        match classify_refresh_failure(resp).await {
            RefreshErrorKind::Transient(msg) => assert!(msg.contains("503")),
            _ => panic!("expected transient classification for server error"),
        }
    }

    #[tokio::test]
    async fn non_json_400_is_transient_not_revoke() {
        // Defensive: an IdP HTML error page or WAF block MUST NOT trigger
        // revoke. The whole point of the classifier is to be paranoid here.
        let resp = response_with(400, "<html><body>bad gateway</body></html>");
        match classify_refresh_failure(resp).await {
            RefreshErrorKind::Transient(_) => {}
            _ => panic!("non-JSON 400 must be transient"),
        }
    }

    #[tokio::test]
    async fn unknown_error_code_is_other_oauth_error() {
        let resp = response_with(400, r#"{"error":"invalid_scope"}"#);
        assert_eq!(classify_refresh_failure(resp).await, RefreshErrorKind::OtherOauthError("invalid_scope".into()));
    }

    #[tokio::test]
    async fn empty_body_on_400_is_transient() {
        let resp = response_with(400, "");
        match classify_refresh_failure(resp).await {
            RefreshErrorKind::Transient(_) => {}
            _ => panic!("empty body must be transient"),
        }
    }

    #[tokio::test]
    async fn multibyte_body_does_not_panic_on_byte_cutoff() {
        // Regression: an IdP/WAF page in CJK or emoji longer than the byte
        // cap previously panicked inside `truncate` when the slice boundary
        // landed mid-codepoint. `truncate` must cut on char boundaries.
        let huge_cjk = "网关错误，请稍后再试".repeat(50); // ~900 UTF-8 bytes
        let resp = response_with(400, &huge_cjk);
        match classify_refresh_failure(resp).await {
            RefreshErrorKind::Transient(msg) => assert!(msg.contains("400")),
            _ => panic!("CJK body must be transient"),
        }
    }
}
