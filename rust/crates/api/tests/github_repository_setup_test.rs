//! Read-only repository setup against a local GitHub REST mock. One test owns
//! GITHUB_API_BASE; scenarios run sequentially to avoid environment races.

use agentforge_api::testing::github_app::{GithubAppClient, GithubAppConfig};
use agentforge_core::ErrorKind;
use httpmock::Mock;
use httpmock::prelude::*;
use serde_json::{Value, json};

const REPO: &str = "acme/widgets";
const TEST_RSA_PEM: &str = include_str!("fixtures/test_rsa_private_key.pem");

fn client() -> GithubAppClient {
    GithubAppClient::new(GithubAppConfig {
        app_id: "12345".into(),
        installation_id: "1".into(),
        private_key_pem: TEST_RSA_PEM.into(),
        repo: REPO.into(),
    })
}

async fn token(server: &MockServer, permissions: Value) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(POST).path("/app/installations/1/access_tokens");
            then.status(201).json_body(json!({
                "token": "ghs_setup_secret",
                "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
                "permissions": permissions,
            }));
        })
        .await
}

async fn repository(server: &MockServer, status: u16, body: Value) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/repos/{REPO}")).header("Authorization", "Bearer ghs_setup_secret");
            then.status(status).json_body(body);
        })
        .await
}

fn metadata(branch: &str) -> Value {
    json!({ "default_branch": branch, "archived": false, "disabled": false, "allow_squash_merge": true })
}

#[tokio::test]
async fn repository_setup_discovers_branch_and_checks_prerequisites_without_writes() {
    let server = MockServer::start_async().await;
    unsafe {
        std::env::set_var("GITHUB_API_BASE", server.base_url());
    }
    let token_mock = token(&server, json!({ "contents": "write", "pull_requests": "write", "checks": "read" })).await;
    let c = client();
    // A changing default branch is observed on each read, while the installation
    // token remains cached. '/' and '#' must stay inside the branch path segment.
    for (branch, encoded) in
        [("master", "master"), ("release/stable", "release%2Fstable"), ("release/#stable", "release%2F%23stable")]
    {
        let repo = repository(&server, 200, metadata(branch)).await;
        let reference = server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/repos/{REPO}/git/ref/heads/{encoded}"));
                then.status(200).json_body(json!({ "object": { "sha": "0123456789abcdef0123456789abcdef01234567" } }));
            })
            .await;
        let setup = c.repository_setup().await.expect("repository setup");
        assert_eq!(setup.repository, REPO);
        assert_eq!(setup.default_branch, branch);
        assert_eq!(setup.base_sha, "0123456789abcdef0123456789abcdef01234567");
        assert!(setup.contents_write && setup.pull_requests_write && setup.checks_read && setup.squash_merge_allowed);
        let wire = serde_json::to_value(&setup).expect("safe setup projection");
        assert_eq!(wire["defaultBranch"], branch);
        assert_eq!(wire["baseSha"], "0123456789abcdef0123456789abcdef01234567");
        assert!(!wire.to_string().contains("ghs_setup_secret"));
        repo.assert_async().await;
        reference.assert_async().await;
        repo.delete_async().await;
        reference.delete_async().await;
    }
    token_mock.assert_async().await;

    // Invalid/inaccessible metadata is never replaced by a guessed main branch.
    for (status, body, permanent) in [
        (200, json!({ "default_branch": "develop", "archived": true, "disabled": false }), true),
        (200, json!({ "default_branch": "develop", "archived": false, "disabled": true }), true),
        (200, metadata(""), true),
        (200, metadata("../main"), true),
        (200, metadata("--upload-pack=unexpected"), true),
        (200, metadata("a@{b"), true),
        (200, metadata("main.lock"), true),
        (200, metadata("@"), true),
        (200, json!({ "archived": false, "disabled": false }), false),
        (404, json!({ "message": "ghs_setup_secret" }), true),
        (401, json!({ "message": "ghs_setup_secret" }), true),
        (403, json!({ "message": "rate limit" }), false),
        (500, json!({ "message": "ghs_setup_secret" }), false),
    ] {
        let repo = repository(&server, status, body).await;
        let err = c.repository_setup().await.expect_err("unusable setup must fail");
        assert_eq!(matches!(err.kind, ErrorKind::ValidationWithCode { .. }), permanent);
        assert!(!err.to_string().contains("ghs_setup_secret"));
        repo.assert_async().await;
        repo.delete_async().await;
    }
    let repo = repository(&server, 200, metadata("master")).await;
    for sha in ["", "not-a-sha", "abc123", "g123456789abcdef0123456789abcdef01234567", "-"] {
        let reference = server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/repos/{REPO}/git/ref/heads/master"));
                then.status(200).json_body(json!({ "object": { "sha": sha } }));
            })
            .await;
        let err = c.repository_setup().await.expect_err("invalid revision must fail before git");
        assert!(
            matches!(err.kind, ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.repository_base_unavailable")
        );
        reference.assert_async().await;
        reference.delete_async().await;
    }
    repo.delete_async().await;
    token_mock.delete_async().await;

    // Missing or read-only grants fail before reading a revision or pushing.
    for permissions in [
        json!({}),
        json!({ "contents": "read", "pull_requests": "write" }),
        json!({ "contents": "write", "pull_requests": "read" }),
    ] {
        let token_mock = token(&server, permissions).await;
        let repo = repository(&server, 200, metadata("develop")).await;
        let err = client().repository_setup().await.expect_err("write permissions are required");
        assert!(
            matches!(err.kind, ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.repository_permissions")
        );
        token_mock.assert_async().await;
        repo.assert_async().await;
        token_mock.delete_async().await;
        repo.delete_async().await;
    }

    // CI visibility and squash support are reported independently from the
    // ability to open a draft. They do not imply that CI passed or a human approved.
    let token_mock = token(&server, json!({ "contents": "write", "pull_requests": "write" })).await;
    let mut body = metadata("develop");
    body["allow_squash_merge"] = json!(false);
    let repo = repository(&server, 200, body).await;
    let reference = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/repos/{REPO}/git/ref/heads/develop"));
            then.status(200).json_body(json!({ "object": { "sha": "0123456789abcdef0123456789abcdef01234567" } }));
        })
        .await;
    let setup = client().repository_setup().await.expect("draft-only setup");
    assert!(!setup.checks_read && !setup.squash_merge_allowed);
    reference.assert_async().await;
    reference.delete_async().await;

    // Empty repositories / removed default refs need an explicit operator fix.
    let missing_ref = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/repos/{REPO}/git/ref/heads/develop"));
            then.status(404).json_body(json!({ "message": "ghs_setup_secret" }));
        })
        .await;
    let err = client().repository_setup().await.expect_err("missing default ref");
    assert!(
        matches!(err.kind, ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.repository_access")
    );
    assert!(!err.to_string().contains("ghs_setup_secret"));
    missing_ref.assert_async().await;
    repo.delete_async().await;
    token_mock.delete_async().await;
    unsafe {
        std::env::remove_var("GITHUB_API_BASE");
    }
}
