//! Only a live owner or admin membership may rename an organization (#1324).
//! A member or viewer gets 403 `FORBIDDEN`; a non-member gets 404 `NOT_FOUND`;
//! the name is unchanged after every refusal.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt, test_app_with_mock_provider};

async fn seed_org(pool: &PgPool) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Original', $2)")
        .bind(org)
        .bind(format!("org-{org}"))
        .execute(pool)
        .await
        .expect("seed org");
    org
}

async fn seed_member(pool: &PgPool, org: Uuid, role: &str) -> Uuid {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind(format!("u-{user}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(user)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed membership");
    user
}

async fn org_name(pool: &PgPool, org: Uuid) -> String {
    sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1").bind(org).fetch_one(pool).await.unwrap()
}

async fn rename(pool: &PgPool, uri: &str, token_org: Uuid, user: Uuid, name: &str) -> (StatusCode, Value) {
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let jwt = mint_test_jwt(token_org, user, "member");
    let request = Request::builder()
        .method(Method::PATCH)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .expect("request");
    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
    let body = if bytes.is_empty() { json!({}) } else { serde_json::from_slice(&bytes).expect("json body") };
    (status, body)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn only_owners_and_admins_can_rename_an_organization(pool: PgPool) {
    let org = seed_org(&pool).await;
    let owner = seed_member(&pool, org, "owner").await;
    let admin = seed_member(&pool, org, "admin").await;
    let member = seed_member(&pool, org, "member").await;
    let viewer = seed_member(&pool, org, "viewer").await;
    let other_org = seed_org(&pool).await;
    let outsider = seed_member(&pool, other_org, "owner").await;

    for uri in [format!("/api/v1/organizations/{org}"), format!("/api/v1/orgs/{org}")] {
        for (label, user) in [("member", member), ("viewer", viewer)] {
            let before = org_name(&pool, org).await;
            let (status, body) = rename(&pool, &uri, org, user, "Renamed by member").await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri} {label}: {body}");
            assert_eq!(body["error"]["code"], "FORBIDDEN", "{uri} {label}: {body}");
            assert_eq!(org_name(&pool, org).await, before, "{uri} {label}: name changed after refusal");
        }

        let before = org_name(&pool, org).await;
        let (status, body) = rename(&pool, &uri, other_org, outsider, "Renamed by outsider").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} non-member: {body}");
        assert_eq!(body["error"]["code"], "NOT_FOUND", "{uri} non-member: {body}");
        assert_eq!(org_name(&pool, org).await, before, "{uri} non-member: name changed after refusal");

        for (label, user) in [("owner", owner), ("admin", admin)] {
            let name = format!("Renamed by {label}");
            let (status, body) = rename(&pool, &uri, org, user, &name).await;
            assert_eq!(status, StatusCode::OK, "{uri} {label}: {body}");
            assert_eq!(org_name(&pool, org).await, name, "{uri} {label}: rename not applied");
        }
    }
}
