//! Wire tests for the GitHub App client using a local httpmock server.
//!
//! `GITHUB_API_BASE` is process-global, so every assertion runs inside a single
//! `#[tokio::test]` that sets the var once, in sequence. The client is exercised
//! through `agentforge_api::testing::github_app`, a test-only re-export.

use agentforge_api::testing::github_app::{GithubAppClient, GithubAppConfig};
use httpmock::prelude::*;

const TEST_RSA_PEM: &str = include_str!("fixtures/test_rsa_private_key.pem");
const REPO: &str = "acme/widgets";

fn client(base: &str) -> GithubAppClient {
    // SAFETY: single-threaded test entry; we point the client at the mock base.
    unsafe {
        std::env::set_var("GITHUB_API_BASE", base);
    }
    GithubAppClient::new(GithubAppConfig {
        app_id: "12345".into(),
        installation_id: "1".into(),
        private_key_pem: TEST_RSA_PEM.into(),
        repo: REPO.into(),
    })
}

#[tokio::test]
async fn github_app_client_drives_pr_lifecycle() {
    let server = MockServer::start_async().await;

    // Token mint: must be hit before any authed call.
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let token_mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/app/installations/1/access_tokens")
                .header("Accept", "application/vnd.github+json")
                .header_exists("Authorization");
            then.status(201)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "token": "ghs_x",
                    "expires_at": expires_at,
                }));
        })
        .await;

    // --- draft PR ------------------------------------------------------------
    let pr_mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/repos/{REPO}/pulls"))
                .header("Accept", "application/vnd.github+json")
                .header("Authorization", "Bearer ghs_x")
                .json_body_partial(r#"{ "draft": true }"#);
            then.status(201)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "number": 7,
                    "html_url": "https://github.com/acme/widgets/pull/7",
                    "node_id": "PR_kw1",
                    "head": { "sha": "abc" },
                    "draft": true,
                }));
        })
        .await;

    let c = client(&server.base_url());
    let pr = c
        .create_draft_pr("self-fix/x", "main", "title", "body")
        .await
        .expect("create draft pr");
    assert_eq!(pr.number, 7);
    assert_eq!(pr.html_url, "https://github.com/acme/widgets/pull/7");
    assert_eq!(pr.node_id, "PR_kw1");
    assert_eq!(pr.head.sha, "abc");
    assert!(pr.draft);
    pr_mock.assert_async().await;
    token_mock.assert_async().await; // token was minted exactly once

    // --- checks: failure conclusion => not green -----------------------------
    let checks_fail = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/repos/{REPO}/commits/abc/check-runs"));
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "check_runs": [ { "status": "completed", "conclusion": "failure" } ]
                }));
        })
        .await;
    assert!(
        !c.all_checks_green("abc").await.expect("checks call"),
        "a failure conclusion must not be green"
    );
    checks_fail.assert_async().await;
    checks_fail.delete_async().await;

    // --- checks: all success => green ----------------------------------------
    let checks_ok = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/repos/{REPO}/commits/abc/check-runs"));
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "check_runs": [
                        { "status": "completed", "conclusion": "success" },
                        { "status": "completed", "conclusion": "success" }
                    ]
                }));
        })
        .await;
    assert!(
        c.all_checks_green("abc").await.expect("checks call"),
        "all-success runs must be green"
    );
    checks_ok.assert_async().await;

    // --- merge: 409 head moved => head-moved conflict ------------------------
    let merge_409 = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path(format!("/repos/{REPO}/pulls/7/merge"))
                .json_body_partial(r#"{ "sha": "abc" }"#);
            then.status(409)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({ "message": "Head branch was modified." }));
        })
        .await;
    let err = c
        .merge_with_expected_head(7, "abc")
        .await
        .expect_err("409 must be an error");
    // The head-moved guard maps to a 409 CONFLICT HTTP response.
    use axum::response::IntoResponse;
    let status = err.into_response().status();
    assert_eq!(
        status,
        axum::http::StatusCode::CONFLICT,
        "409 must map to the head-moved conflict"
    );
    merge_409.assert_async().await;
}
