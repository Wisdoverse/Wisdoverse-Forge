//! Minimal GitHub App REST v3 client for the self-fix loop. No octocrab.
//! Mints an app JWT, exchanges an installation token (cached), and performs the
//! repo operations the PR Bridge / Merge Executor need. Never logs secrets.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentforge_core::{AppError, AppResult};
use serde::Deserialize;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GithubAppConfig {
    pub app_id: String,
    pub installation_id: String,
    pub private_key_pem: String, // decrypted before construction
    pub repo: String,            // "owner/repo"
}

#[derive(serde::Serialize)]
struct AppJwtClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

/// Build the signed app JWT (RS256). `now_unix` injected for testability.
pub fn build_app_jwt(
    app_id: &str,
    private_key_pem: &str,
    now_unix: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = AppJwtClaims {
        iat: now_unix.saturating_sub(60), // clock-skew backdate
        exp: now_unix + 9 * 60,           // GitHub max 10 min
        iss: app_id.to_string(),
    };
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key_pem.as_bytes())?;
    jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &claims, &key)
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at_unix: u64,
}

#[allow(dead_code)]
pub struct GithubAppClient {
    http: reqwest::Client,
    cfg: GithubAppConfig,
    cache: std::sync::Arc<tokio::sync::Mutex<Option<CachedToken>>>,
}

/// Standard GitHub REST v3 headers, applied to every authed request.
const ACCEPT_GITHUB_JSON: &str = "application/vnd.github+json";
const GITHUB_API_VERSION: &str = "2022-11-28";

#[derive(Deserialize)]
struct InstallTokenResp {
    token: String,
    expires_at: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct PullRequest {
    pub number: i32,
    #[serde(rename = "html_url")]
    pub html_url: String,
    pub node_id: String,
    pub head: PrHead,
    #[serde(default)]
    pub draft: bool,
    /// `true` once the PR has been merged. GitHub omits this field on the
    /// list endpoint, so it defaults to `false`; it is present on the
    /// single-PR `GET /repos/{repo}/pulls/{number}` response we use for the
    /// idempotency check.
    #[serde(default)]
    pub merged: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct PrHead {
    pub sha: String,
}

impl GithubAppClient {
    #[allow(dead_code)]
    pub fn new(cfg: GithubAppConfig) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("agentforge-self-fix")
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            cfg,
            cache: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    fn now_unix() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    /// Reads `GITHUB_API_BASE` so tests can point at a mock server; in
    /// production the var is unset and we talk to real GitHub.
    fn api_base() -> String {
        std::env::var("GITHUB_API_BASE").unwrap_or_else(|_| "https://api.github.com".into())
    }

    /// Mint or reuse an installation access token. Cached until ~60s before
    /// expiry. The app JWT and the installation token are NEVER logged.
    async fn installation_token(&self) -> AppResult<String> {
        {
            let guard = self.cache.lock().await;
            if let Some(cached) = guard.as_ref()
                && cached.expires_at_unix > Self::now_unix() + 60
            {
                return Ok(cached.token.clone());
            }
        }

        let endpoint = "POST /app/installations/{id}/access_tokens";
        let jwt = build_app_jwt(&self.cfg.app_id, &self.cfg.private_key_pem, Self::now_unix()).map_err(|_| {
            // The error type can carry key material context; do not surface it.
            crate::domain::github_app::sign_jwt_failed()
        })?;

        let url = format!("{}/app/installations/{}/access_tokens", Self::api_base(), self.cfg.installation_id);
        let resp = self
            .http
            .post(url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {jwt}"))
            .header(reqwest::header::ACCEPT, ACCEPT_GITHUB_JSON)
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
            .send()
            .await
            .map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }

        let parsed: InstallTokenResp = resp.json().await.map_err(|_| unavailable(endpoint))?;
        let expires_at_unix = chrono::DateTime::parse_from_rfc3339(&parsed.expires_at)
            .map(|dt| dt.timestamp().max(0) as u64)
            .unwrap_or_else(|_| Self::now_unix());

        {
            let mut guard = self.cache.lock().await;
            *guard = Some(CachedToken { token: parsed.token.clone(), expires_at_unix });
        }
        Ok(parsed.token)
    }

    /// Build a request carrying the installation token + standard headers.
    async fn authed(&self, method: reqwest::Method, url: String) -> AppResult<reqwest::RequestBuilder> {
        let token = self.installation_token().await?;
        Ok(self
            .http
            .request(method, url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .header(reqwest::header::ACCEPT, ACCEPT_GITHUB_JSON)
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION))
    }

    /// HTTPS remote with a short-lived installation token embedded (GitHub App
    /// pattern). Used ONLY for the server-owned clone's `origin`; NEVER written
    /// to `/workspace` and NEVER logged (the returned string carries a secret).
    #[allow(dead_code)]
    pub(crate) async fn authed_remote_url(&self) -> AppResult<String> {
        let token = self.installation_token().await?;
        Ok(format!("https://x-access-token:{token}@github.com/{}.git", self.cfg.repo))
    }

    /// `origin/main` SHA — the base pin for self-fix branches.
    #[allow(dead_code)]
    pub async fn default_branch_sha(&self) -> AppResult<String> {
        let endpoint = "GET /repos/{repo}/git/ref/heads/main";
        let url = format!("{}/repos/{}/git/ref/heads/main", Self::api_base(), self.cfg.repo);
        let resp = self.authed(reqwest::Method::GET, url).await?.send().await.map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }

        #[derive(Deserialize)]
        struct RefResp {
            object: RefObject,
        }
        #[derive(Deserialize)]
        struct RefObject {
            sha: String,
        }
        let parsed: RefResp = resp.json().await.map_err(|_| unavailable(endpoint))?;
        Ok(parsed.object.sha)
    }

    /// Open a draft PR for a self-fix branch.
    ///
    /// Retry-safe: on a partial-success retry the deterministic head branch is
    /// force-pushed and a *new* create call returns **422** ("A pull request
    /// already exists for {owner}:{branch}"). Rather than stranding the task we
    /// fetch the existing open PR for that head and return it — same shape — so
    /// the bridge is idempotent. Any other non-2xx (or a 422 with no matching
    /// open PR) propagates as the original error.
    #[allow(dead_code)]
    pub async fn create_draft_pr(
        &self,
        head_branch: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> AppResult<PullRequest> {
        let endpoint = "POST /repos/{repo}/pulls";
        let url = format!("{}/repos/{}/pulls", Self::api_base(), self.cfg.repo);
        let resp = self
            .authed(reqwest::Method::POST, url)
            .await?
            .json(&crate::domain::github_app::create_pull_request_body(title, body, head_branch, base))
            .send()
            .await
            .map_err(|_| unavailable(endpoint))?;

        let status = resp.status();
        if status.is_success() {
            return resp.json::<PullRequest>().await.map_err(|_| unavailable(endpoint));
        }

        // 422 = a PR already exists for this head (retry after a partial success).
        // Recover the existing open PR instead of failing the task.
        if status.as_u16() == 422
            && let Some(existing) = self.find_open_pr_for_head(head_branch).await?
        {
            return Ok(existing);
        }
        Err(unavailable_status(status, endpoint))
    }

    /// Find the single open PR whose head is `head_branch` in this repo, if any.
    /// Returns `None` when GitHub reports no matching open PR.
    async fn find_open_pr_for_head(&self, head_branch: &str) -> AppResult<Option<PullRequest>> {
        let endpoint = "GET /repos/{repo}/pulls?head=...&state=open";
        // `head` is `owner:branch`; the owner is the first segment of `owner/repo`.
        let owner = self.cfg.repo.split('/').next().unwrap_or(&self.cfg.repo);
        let url = format!(
            "{}/repos/{}/pulls?head={}:{}&state=open&per_page=1",
            Self::api_base(),
            self.cfg.repo,
            owner,
            head_branch
        );
        let resp = self.authed(reqwest::Method::GET, url).await?.send().await.map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }
        let prs: Vec<PullRequest> = resp.json().await.map_err(|_| unavailable(endpoint))?;
        Ok(prs.into_iter().next())
    }

    /// True IFF the head commit has at least one check run AND every run is
    /// `completed` with conclusion `success`. This is an *all-checks* gate
    /// (stricter than a required-checks-only gate): a single skipped, failed,
    /// pending, or neutral run makes this false. An empty list is false because
    /// nothing has been verified yet.
    ///
    /// This is the MERGE safety gate, so it must see EVERY check run, not just
    /// the first page. The check-runs endpoint paginates at 100/page; we request
    /// `per_page=100` and follow the `Link: rel="next"` header until exhausted,
    /// aggregating the per-run conclusions across all pages.
    #[allow(dead_code)]
    pub async fn all_checks_green(&self, head_sha: &str) -> AppResult<bool> {
        let endpoint = "GET /repos/{repo}/commits/{sha}/check-runs";
        let mut next_url =
            Some(format!("{}/repos/{}/commits/{}/check-runs?per_page=100", Self::api_base(), self.cfg.repo, head_sha));

        // Number of runs actually observed across all pages. We count the runs
        // we see rather than trusting a `total_count` field (a partial response
        // could omit it); one observed run clears the "something ran" bar.
        let mut seen_runs: u64 = 0;
        let mut all_success = true;

        while let Some(url) = next_url.take() {
            let resp = self.authed(reqwest::Method::GET, url).await?.send().await.map_err(|_| unavailable(endpoint))?;

            if !resp.status().is_success() {
                return Err(unavailable_status(resp.status(), endpoint));
            }

            // Capture the `Link` header before consuming the body for JSON.
            let link_next =
                resp.headers().get(reqwest::header::LINK).and_then(|v| v.to_str().ok()).and_then(parse_link_next);

            #[derive(Deserialize)]
            struct CheckRunsResp {
                check_runs: Vec<CheckRun>,
            }
            #[derive(Deserialize)]
            struct CheckRun {
                status: String,
                conclusion: Option<String>,
            }
            let parsed: CheckRunsResp = resp.json().await.map_err(|_| unavailable(endpoint))?;
            seen_runs += parsed.check_runs.len() as u64;
            for run in &parsed.check_runs {
                if !(run.status == "completed" && run.conclusion.as_deref() == Some("success")) {
                    all_success = false;
                }
            }

            next_url = link_next;
        }

        // Green IFF at least one check ran AND every run across ALL pages passed.
        Ok(seen_runs > 0 && all_success)
    }

    /// Current head SHA of a PR (re-read just before an atomic merge).
    #[allow(dead_code)]
    pub async fn pr_head_sha(&self, pr_number: i32) -> AppResult<String> {
        Ok(self.fetch_pull_request(pr_number).await?.head.sha)
    }

    /// `true` if the PR is already merged. Used by the Merge Executor for
    /// idempotency: a retry after a successful merge must succeed instead of
    /// erroring on a no-longer-mergeable PR.
    #[allow(dead_code)]
    pub async fn pr_is_merged(&self, pr_number: i32) -> AppResult<bool> {
        Ok(self.fetch_pull_request(pr_number).await?.merged)
    }

    /// `true` if the PR is currently a draft. Used by the Merge Executor to skip
    /// the `mark_ready_for_review` call when a previous attempt already flipped
    /// the PR to ready (making the call idempotent at the executor level).
    #[allow(dead_code)]
    pub async fn pr_is_draft(&self, pr_number: i32) -> AppResult<bool> {
        Ok(self.fetch_pull_request(pr_number).await?.draft)
    }

    /// Fetch a PR object (used for `head.sha` and `node_id`).
    async fn fetch_pull_request(&self, pr_number: i32) -> AppResult<PullRequest> {
        let endpoint = "GET /repos/{repo}/pulls/{number}";
        let url = format!("{}/repos/{}/pulls/{}", Self::api_base(), self.cfg.repo, pr_number);
        let resp = self.authed(reqwest::Method::GET, url).await?.send().await.map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }
        resp.json::<PullRequest>().await.map_err(|_| unavailable(endpoint))
    }

    /// Flip a draft PR to ready-for-review. GitHub exposes no REST verb for
    /// this, so we use the GraphQL `markPullRequestReadyForReview` mutation.
    #[allow(dead_code)]
    pub async fn mark_ready_for_review(&self, pr_number: i32) -> AppResult<()> {
        let node_id = self.fetch_pull_request(pr_number).await?.node_id;

        let endpoint = "POST /graphql (markPullRequestReadyForReview)";
        let url = format!("{}/graphql", Self::api_base());
        let resp = self
            .authed(reqwest::Method::POST, url)
            .await?
            .json(&crate::domain::github_app::mark_ready_mutation_body(&node_id))
            .send()
            .await
            .map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }

        // A GraphQL 200 can still carry an `errors` array. Treat it as failure
        // EXCEPT when the PR is already ready-for-review: GitHub's GraphQL returns
        // a "Pull request is not in the draft state" error in that case, which is
        // idempotent — the desired final state (ready) already holds.
        let body: serde_json::Value = resp.json().await.map_err(|_| unavailable(endpoint))?;
        if let Some(errors) = body.get("errors").and_then(|e| e.as_array())
            && !errors.is_empty()
        {
            let all_already_ready = errors.iter().all(|e| {
                e.get("message").and_then(|m| m.as_str()).is_some_and(|msg| msg.contains("not in the draft state"))
            });
            if !all_already_ready {
                return Err(unavailable(endpoint));
            }
            // Every error is "already ready" — desired state holds; treat as Ok.
        }
        Ok(())
    }

    /// Atomically squash-merge a PR only if its head still matches
    /// `expected_head`. GitHub returns 409 when the head moved (or the PR is no
    /// longer mergeable for SHA reasons): that maps to the self-fix head-moved
    /// conflict so the caller re-reviews. 405 means not mergeable (e.g. still a
    /// draft or branch protection blocked).
    #[allow(dead_code)]
    pub async fn merge_with_expected_head(&self, pr_number: i32, expected_head: &str) -> AppResult<()> {
        let endpoint = "PUT /repos/{repo}/pulls/{number}/merge";
        let url = format!("{}/repos/{}/pulls/{}/merge", Self::api_base(), self.cfg.repo, pr_number);
        let resp = self
            .authed(reqwest::Method::PUT, url)
            .await?
            .json(&crate::domain::github_app::merge_squash_body(expected_head))
            .send()
            .await
            .map_err(|_| unavailable(endpoint))?;

        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        match status.as_u16() {
            409 => Err(crate::domain::self_fix::SelfFixPolicy::head_moved()),
            405 => Err(crate::domain::github_app::not_mergeable()),
            _ => Err(unavailable_status(status, endpoint)),
        }
    }

    /// Post a comment on the PR (self-fix status / audit trail).
    #[allow(dead_code)]
    pub async fn comment(&self, pr_number: i32, body: &str) -> AppResult<()> {
        let endpoint = "POST /repos/{repo}/issues/{number}/comments";
        let url = format!("{}/repos/{}/issues/{}/comments", Self::api_base(), self.cfg.repo, pr_number);
        let resp = self
            .authed(reqwest::Method::POST, url)
            .await?
            .json(&crate::domain::github_app::comment_body(body))
            .send()
            .await
            .map_err(|_| unavailable(endpoint))?;

        if !resp.status().is_success() {
            return Err(unavailable_status(resp.status(), endpoint));
        }
        Ok(())
    }
}

/// Parse a GitHub `Link` header and return the URL of the `rel="next"` page, if
/// any. The header is a comma-separated list of `<url>; rel="name"` entries; we
/// pick the one whose `rel` is `next`. Returns `None` when there is no next page.
fn parse_link_next(link_header: &str) -> Option<String> {
    for part in link_header.split(',') {
        let part = part.trim();
        // Each part looks like: <https://api.github.com/...?page=2>; rel="next"
        let mut segments = part.split(';');
        let url_seg = segments.next()?.trim();
        let is_next = segments.any(|s| {
            let s = s.trim();
            s == "rel=\"next\"" || s == "rel=next"
        });
        if is_next {
            let url = url_seg.trim_start_matches('<').trim_end_matches('>');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Map a transport failure to a typed error WITHOUT leaking any token, header,
/// or response body. `endpoint_label` is a static route shape, never user data.
/// The `ErrorKind` policy lives in `domain::github_app` (DDD boundary); this is
/// a thin call-site convenience so the many fallible sites read cleanly.
fn unavailable(endpoint_label: &str) -> AppError {
    crate::domain::github_app::request_failed(endpoint_label)
}

/// Map a non-2xx status to a typed error. Only the numeric status and the
/// static endpoint label are included — never the response body or headers.
fn unavailable_status(status: reqwest::StatusCode, endpoint_label: &str) -> AppError {
    crate::domain::github_app::status_failed(status.as_u16(), endpoint_label)
}

/// Build a `GithubAppClient` from the four `github_app_*` config fields.
/// Returns `None` if any required field is absent.
///
/// Expects `github_app_private_key` to be base64-encoded PEM (env-safe single-
/// line form). Raw PEM (starting with `-----BEGIN`) is also accepted as a
/// fallback for operators who set the key directly.
pub(crate) fn build_github_app_client(config: &agentforge_core::AppConfig) -> Option<GithubAppClient> {
    use secrecy::ExposeSecret;
    let app_id = config.github_app_id.clone()?;
    let installation_id = config.github_app_installation_id.clone()?;
    let repo = config.github_app_repo.clone()?;
    let raw = config.github_app_private_key.as_ref()?.expose_secret().to_string();
    let private_key_pem = decode_private_key_pem(&raw)?;
    Some(GithubAppClient::new(GithubAppConfig { app_id, installation_id, private_key_pem, repo }))
}

/// Decode the private key from config. Accepts two forms:
/// 1. Base64-encoded PEM (standard env-var form) → base64-decode → UTF-8.
/// 2. Raw PEM (starts with `-----BEGIN`) → used as-is.
///
/// Returns `None` if the value is neither valid base64-of-UTF8 nor raw PEM.
fn decode_private_key_pem(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("-----BEGIN") {
        return Some(trimmed.to_string());
    }
    // Attempt base64 standard decode → UTF-8 PEM.
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(trimmed).ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_RSA_PEM: &str = include_str!("../../../tests/fixtures/test_rsa_private_key.pem");

    #[test]
    fn app_jwt_has_backdated_iat_and_bounded_exp() {
        use base64::Engine;

        let now = 1_700_000_000u64;
        let token = build_app_jwt("12345", TEST_RSA_PEM, now).expect("jwt");

        // Decode the claims segment directly. We deliberately avoid
        // `jsonwebtoken::decode` here: under the `aws_lc_rs` backend a
        // `DecodingKey::from_secret` is rejected as an invalid RSA key even
        // with signature validation disabled. Base64url-decoding the payload
        // verifies the *claim contents*, which is all this test asserts.
        let payload_b64 = token.split('.').nth(1).expect("jwt payload segment");
        let payload_bytes =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64).expect("base64url payload");
        let claims: serde_json::Value = serde_json::from_slice(&payload_bytes).expect("claims json");

        assert_eq!(claims["iss"], "12345");
        assert_eq!(claims["iat"].as_u64().unwrap(), now - 60);
        assert!(claims["exp"].as_u64().unwrap() <= now + 600);
        // The header still declares RS256 (the signing path exercised the key).
        let header = jsonwebtoken::decode_header(&token).expect("jwt header");
        assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    }

    #[tokio::test]
    async fn cached_token_reused_before_expiry() {
        // A bogus base + repo: if the client made ANY HTTP call it would fail,
        // so a successful return proves the cached token short-circuited I/O.
        let client = GithubAppClient::new(GithubAppConfig {
            app_id: "12345".into(),
            installation_id: "1".into(),
            private_key_pem: TEST_RSA_PEM.into(),
            repo: "acme/widgets".into(),
        });
        {
            let mut guard = client.cache.lock().await;
            *guard =
                Some(CachedToken { token: "ghs_cached".into(), expires_at_unix: GithubAppClient::now_unix() + 3600 });
        }
        let token = client.installation_token().await.expect("cached token");
        assert_eq!(token, "ghs_cached");
    }

    // --- build_github_app_client / decode_private_key_pem unit tests ---

    fn base64_pem() -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(TEST_RSA_PEM.as_bytes())
    }

    fn full_config_with_key(key_value: &str) -> agentforge_core::AppConfig {
        use secrecy::SecretString;
        let mut config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
        config.github_app_id = Some("12345".into());
        config.github_app_installation_id = Some("67890".into());
        config.github_app_private_key = Some(SecretString::from(key_value.to_string()));
        config.github_app_repo = Some("acme/widgets".into());
        config
    }

    #[test]
    fn decode_pem_passthrough_for_raw_pem() {
        let result = decode_private_key_pem(TEST_RSA_PEM);
        assert!(result.is_some(), "raw PEM should pass through");
        assert!(result.unwrap().starts_with("-----BEGIN"));
    }

    #[test]
    fn decode_pem_decodes_base64_encoded_pem() {
        let b64 = base64_pem();
        let result = decode_private_key_pem(&b64);
        assert!(result.is_some(), "base64 PEM should decode");
        assert!(result.unwrap().starts_with("-----BEGIN"));
    }

    #[test]
    fn build_github_app_client_returns_some_when_all_fields_set_raw_pem() {
        let config = full_config_with_key(TEST_RSA_PEM);
        let client = build_github_app_client(&config);
        assert!(client.is_some(), "all fields set with raw PEM → Some");
    }

    #[test]
    fn build_github_app_client_returns_some_when_all_fields_set_base64_pem() {
        let b64 = base64_pem();
        let config = full_config_with_key(&b64);
        let client = build_github_app_client(&config);
        assert!(client.is_some(), "all fields set with base64 PEM → Some");
    }

    #[test]
    fn build_github_app_client_returns_none_when_fields_absent() {
        // All github_app_* fields are None in the default test config.
        let config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
        let client = build_github_app_client(&config);
        assert!(client.is_none(), "missing fields → None");
    }
}
