//! Integration tests for `GET /api/v1/admin/cli-images` — the CLI agent-image
//! auto-updater status endpoint.
//!
//! Exercises the full HTTP path through the Axum router, asserting:
//!   - a non-admin JWT is rejected with 403 (admin-gated)
//!   - an admin sees every pollable tool (codex/gemini/opencode; never claude),
//!     each `pending` because the worker is off in tests
//!   - the report echoes deployment config (`auto_update_enabled=false`)
//!   - `agents_with_container` counts agents that have a container, cross-org
//!
//! Each test runs against a fresh database via `#[sqlx::test]`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_api::{
    domain::agent::NewAgent,
    repositories::agent::AgentRepository,
    test_support::{mint_test_jwt, tenant_scope_for_ids, test_app_with_mock_provider},
};
use agentforge_core::{CliToolKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_admin_org(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed organization");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");

    (org_id, org_id, user_id)
}

/// Seed a container agent for `tool` and force a non-null `container_id` so it
/// counts toward `agents_with_container`.
async fn seed_container_agent_with_container(
    pool: &PgPool,
    repo: &AgentRepository,
    scope: &TenantScope,
    ws: Uuid,
    tool: CliToolKind,
    name: &str,
) {
    let agent_id = repo
        .create_aggregate(
            scope,
            NewAgent::container(scope, tool, Some(name), None, None, ws, None, None).expect("build container NewAgent"),
        )
        .await
        .expect("create container agent");
    sqlx::query("UPDATE agents SET container_id = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(format!("container-{name}"))
        .execute(pool)
        .await
        .expect("set container_id");
}

async fn get_cli_images(app: axum::Router, jwt: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/cli-images")
                .header("authorization", format!("Bearer {jwt}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

/// A non-admin (member) JWT must be rejected with 403.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_is_forbidden(pool: PgPool) {
    let (org_id, _ws, user_id) = seed_admin_org(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "member");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, _body) = get_cli_images(app, &jwt).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// An admin sees the canonical pollable tool set, every tool `pending` (worker
/// off in tests), and the auto-update flag reported as off.
#[sqlx::test(migrations = "../db/migrations")]
async fn admin_sees_pending_pollable_tools(pool: PgPool) {
    let (org_id, _ws, user_id) = seed_admin_org(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = get_cli_images(app, &jwt).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["autoUpdateEnabled"], false);

    let tools = body["data"]["tools"].as_array().expect("tools array");
    let mut names: Vec<&str> = tools.iter().filter_map(|t| t["tool"].as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["codex", "gemini", "opencode"], "claude must never be polled: {body}");
    assert!(tools.iter().all(|t| t["state"] == "pending"), "no tick ran, all pending: {body}");
    assert!(tools.iter().all(|t| t["agentsWithContainer"] == 0), "no containers seeded: {body}");

    // Prune defaults to disabled/zeroed when the (default-off) worker never ran.
    assert_eq!(body["data"]["prune"]["enabled"], false, "prune off by default: {body}");
    assert_eq!(body["data"]["prune"]["removed"], 0, "nothing pruned: {body}");
    assert_eq!(body["data"]["prune"]["lastRunUnix"], serde_json::Value::Null, "no sweep ran: {body}");
}

/// `agents_with_container` counts agents that have a container, across orgs, and
/// only for agents whose `container_id` is set.
#[sqlx::test(migrations = "../db/migrations")]
async fn counts_agents_with_container_cross_org(pool: PgPool) {
    let (org_a, ws_a, user_a) = seed_admin_org(&pool).await;
    let scope_a = tenant_scope_for_ids(org_a, user_a);
    let (org_b, ws_b, user_b) = seed_admin_org(&pool).await;
    let scope_b = tenant_scope_for_ids(org_b, user_b);
    let repo = AgentRepository::new(pool.clone());

    // Two codex agents WITH a container (one per org) + one codex agent without
    // (should not be counted).
    seed_container_agent_with_container(&pool, &repo, &scope_a, ws_a, CliToolKind::Codex, "a-codex").await;
    seed_container_agent_with_container(&pool, &repo, &scope_b, ws_b, CliToolKind::Codex, "b-codex").await;
    repo.create_aggregate(
        &scope_a,
        NewAgent::container(&scope_a, CliToolKind::Codex, Some("a-codex-nocontainer"), None, None, ws_a, None, None)
            .expect("build container NewAgent"),
    )
    .await
    .expect("create no-container agent");

    let jwt = mint_test_jwt(org_a, user_a, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = get_cli_images(app, &jwt).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let tools = body["data"]["tools"].as_array().expect("tools array");
    let codex = tools.iter().find(|t| t["tool"] == "codex").expect("codex row");
    assert_eq!(codex["agentsWithContainer"], 2, "both orgs' container-backed codex agents count: {body}");
    let gemini = tools.iter().find(|t| t["tool"] == "gemini").expect("gemini row");
    assert_eq!(gemini["agentsWithContainer"], 0, "gemini has no agents: {body}");
}
