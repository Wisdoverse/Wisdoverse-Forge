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
    // Remove this happy-path create mock so the later 422-retry create test is
    // unambiguous (both match `POST /repos/{REPO}/pulls`).
    pr_mock.delete_async().await;
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
    checks_ok.delete_async().await;

    // --- checks pagination: page 2 contains a failure => not green -----------
    // Page 1 (the first check-runs request) is all-success but advertises a
    // `next` Link; page 2 carries a `failure`. The next page is served from a
    // DISTINCT path (`/checks-page-2-fail`) — the client follows the Link URL
    // verbatim — so each page matches exactly one mock with no ambiguity.
    // Without pagination the gate would wrongly read green from page 1 only.
    let page2_fail_url = format!("{}/checks-page-2-fail", server.base_url());
    let checks_pg1_fail = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/repos/{REPO}/commits/abc/check-runs"))
                .query_param("per_page", "100");
            then.status(200)
                .header("content-type", "application/json")
                .header("Link", format!("<{page2_fail_url}>; rel=\"next\""))
                .json_body(serde_json::json!({
                    "total_count": 2,
                    "check_runs": [ { "status": "completed", "conclusion": "success" } ]
                }));
        })
        .await;
    let checks_pg2_fail = server
        .mock_async(|when, then| {
            when.method(GET).path("/checks-page-2-fail");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "total_count": 2,
                    "check_runs": [ { "status": "completed", "conclusion": "failure" } ]
                }));
        })
        .await;
    assert!(
        !c.all_checks_green("abc").await.expect("paginated checks call"),
        "a failure on page 2 must NOT be green"
    );
    checks_pg1_fail.assert_async().await;
    checks_pg2_fail.assert_async().await;
    checks_pg1_fail.delete_async().await;
    checks_pg2_fail.delete_async().await;

    // --- checks pagination: both pages all-success => green ------------------
    let page2_ok_url = format!("{}/checks-page-2-ok", server.base_url());
    let checks_pg1_ok = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/repos/{REPO}/commits/abc/check-runs"))
                .query_param("per_page", "100");
            then.status(200)
                .header("content-type", "application/json")
                .header("Link", format!("<{page2_ok_url}>; rel=\"next\""))
                .json_body(serde_json::json!({
                    "total_count": 2,
                    "check_runs": [ { "status": "completed", "conclusion": "success" } ]
                }));
        })
        .await;
    let checks_pg2_ok = server
        .mock_async(|when, then| {
            when.method(GET).path("/checks-page-2-ok");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "total_count": 2,
                    "check_runs": [ { "status": "completed", "conclusion": "success" } ]
                }));
        })
        .await;
    assert!(
        c.all_checks_green("abc").await.expect("paginated checks call"),
        "all-success across two pages must be green"
    );
    checks_pg1_ok.assert_async().await;
    checks_pg2_ok.assert_async().await;
    checks_pg1_ok.delete_async().await;
    checks_pg2_ok.delete_async().await;

    // --- create_draft_pr: 422 (PR exists) => reuse existing open PR ----------
    let create_422 = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/repos/{REPO}/pulls"))
                .json_body_partial(r#"{ "head": "agent/retry" }"#);
            then.status(422)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "message": "Validation Failed",
                    "errors": [ { "message": "A pull request already exists for acme:agent/retry." } ]
                }));
        })
        .await;
    let list_existing = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/repos/{REPO}/pulls"))
                .query_param("head", "acme:agent/retry")
                .query_param("state", "open");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!([
                    {
                        "number": 99,
                        "html_url": "https://github.com/acme/widgets/pull/99",
                        "node_id": "PR_existing",
                        "head": { "sha": "deadbeef" },
                        "draft": true,
                    }
                ]));
        })
        .await;
    let reused = c
        .create_draft_pr("agent/retry", "main", "title", "body")
        .await
        .expect("422 must recover the existing open PR");
    assert_eq!(reused.number, 99, "must return the existing PR number");
    assert_eq!(reused.html_url, "https://github.com/acme/widgets/pull/99");
    assert_eq!(reused.head.sha, "deadbeef");
    create_422.assert_async().await;
    list_existing.assert_async().await;
    create_422.delete_async().await;
    list_existing.delete_async().await;

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
