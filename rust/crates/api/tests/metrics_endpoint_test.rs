//! Integration tests for the `/metrics` Prometheus scrape endpoint.
//!
//! #889/F005: the scrape is platform-admin only, verified against the LIVE
//! `users.is_admin` column — NOT the JWT `role` claim. A caller holding a
//! forged or stale `role=admin` token whose DB row is not a platform admin
//! must receive 403, closing the cross-tenant metrics-disclosure / recon
//! vector. An actual platform admin (`is_admin=true`) still gets the
//! Prometheus exposition.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt, test_app_with_mock_provider};

/// Seed an org (+ workspace) and a user with the given platform-admin flag.
async fn seed_user(pool: &PgPool, is_admin: bool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .bind(is_admin)
        .execute(pool)
        .await
        .expect("seed user");
    (org_id, user_id)
}

async fn metrics_status(app: &axum::Router, token: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// A caller whose JWT carries `role=admin` but whose DB row is NOT a platform
/// admin must be refused — the gate keys off `users.is_admin`, not the claim.
#[sqlx::test(migrations = "../db/migrations")]
async fn metrics_rejects_forged_admin_claim_when_db_not_admin(pool: PgPool) {
    let (org_id, user_id) = seed_user(&pool, false).await;
    let app = test_app_with_mock_provider(pool, "mock", "reply").await;

    // Forged / stale elevated claim: role=admin in the token, is_admin=false in DB.
    let token = mint_test_jwt(org_id, user_id, "admin");
    let (status, _body) = metrics_status(&app, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "forged admin claim must not scrape /metrics");
}

/// An ordinary member is likewise refused.
#[sqlx::test(migrations = "../db/migrations")]
async fn metrics_rejects_member(pool: PgPool) {
    let (org_id, user_id) = seed_user(&pool, false).await;
    let app = test_app_with_mock_provider(pool, "mock", "reply").await;

    let token = mint_test_jwt(org_id, user_id, "member");
    let (status, _body) = metrics_status(&app, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member must not scrape /metrics");
}

/// A real platform admin (`is_admin=true`) receives the Prometheus exposition.
#[sqlx::test(migrations = "../db/migrations")]
async fn metrics_allows_platform_admin(pool: PgPool) {
    agentforge_api::testing::emit_test_counter();
    let (org_id, user_id) = seed_user(&pool, true).await;
    let app = test_app_with_mock_provider(pool, "mock", "reply").await;

    let token = mint_test_jwt(org_id, user_id, "admin");
    let (status, body) = metrics_status(&app, &token).await;
    assert_eq!(status, StatusCode::OK, "platform admin must scrape /metrics");
    assert!(body.contains("af_test_bootstrap_total"), "body should render the test counter");
    assert!(body.contains("# TYPE"), "body should be Prometheus text format");
}
