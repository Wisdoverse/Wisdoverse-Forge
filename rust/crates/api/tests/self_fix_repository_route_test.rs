//! Exercise repository preflight through the real HTTP router and live admin
//! gate, with PostgreSQL and a local GitHub mock. No real repository writes.

use std::sync::Arc;

use agentforge_api::create_router;
use agentforge_api::test_support::{app_state_with_mock_provider, mint_test_jwt, test_app_config};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

#[sqlx::test(migrations = "../db/migrations")]
async fn repository_preflight_is_authenticated_and_live_admin_gated(pool: PgPool) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test', $2)")
        .bind(org_id)
        .bind(format!("org-{org_id}"))
        .execute(&pool)
        .await
        .expect("seed organization");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(&pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, 'dev@example.com', true)")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed admin");
    let server = MockServer::start_async().await;
    unsafe {
        std::env::set_var("GITHUB_API_BASE", server.base_url());
    }
    let token_mock = server
        .mock_async(|when, then| {
            when.method(POST).path("/app/installations/1/access_tokens");
            then.status(201).json_body(json!({
                "token": "ghs_route_secret",
                "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
                "permissions": { "contents": "write", "pull_requests": "write", "checks": "read" },
            }));
        })
        .await;
    let repo_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets");
            then.status(200).json_body(json!({
                "default_branch": "develop", "archived": false, "disabled": false, "allow_squash_merge": true,
            }));
        })
        .await;
    let ref_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/git/ref/heads/develop");
            then.status(200).json_body(json!({ "object": { "sha": "0123456789abcdef0123456789abcdef01234567" } }));
        })
        .await;
    let mut state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let mut config = test_app_config("postgres://test");
    config.github_app_id = Some("12345".into());
    config.github_app_installation_id = Some("1".into());
    config.github_app_private_key = Some(include_str!("fixtures/test_rsa_private_key.pem").to_string().into());
    config.github_app_repo = Some("acme/widgets".into());
    state.config = Arc::new(config);
    let app = create_router(state);

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/api/v1/self-fix/repository").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let request = || {
        Request::builder()
            .uri("/api/v1/self-fix/repository")
            .header("authorization", format!("Bearer {jwt}"))
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["data"]["repository"], "acme/widgets");
    assert_eq!(payload["data"]["defaultBranch"], "develop");
    assert_eq!(payload["data"]["baseSha"], "0123456789abcdef0123456789abcdef01234567");
    assert!(!String::from_utf8_lossy(&body).contains("ghs_route_secret"));

    sqlx::query("UPDATE users SET is_admin = false WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("revoke admin");
    let response = app.oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "stale admin JWT must not disclose repository metadata");
    // Only the authorized request contacted GitHub.
    token_mock.assert_async().await;
    repo_mock.assert_async().await;
    ref_mock.assert_async().await;
    unsafe {
        std::env::remove_var("GITHUB_API_BASE");
    }
}
