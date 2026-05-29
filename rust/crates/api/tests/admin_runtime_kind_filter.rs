//! Integration tests for the admin agents `runtime_kind` projection field and
//! the `?runtimeKind=` list filter (issue #461).
//!
//! These exercise the full HTTP path `GET /api/v1/admin/agents` through the
//! Axum router with an admin JWT, asserting:
//!   - the projection serialises `runtimeKind` as `container | cli | api`
//!   - `?runtimeKind=cli` returns only host-CLI agents
//!   - an unknown `runtimeKind` value yields HTTP 422 (not a silent empty list)
//!   - per-org seeding stays attributable (org boundaries survive the join)
//!
//! Each test runs against a fresh database via `#[sqlx::test]`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_api::{
    domain::agent::{HostCliIdentity, NewAgent},
    repositories::agent::AgentRepository,
    test_support::{mint_test_jwt, tenant_scope_for_ids, test_app_with_mock_provider},
};
use agentforge_core::{CliToolKind, RuntimeKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Seed a minimal (organization + workspace + user + admin membership) tuple.
/// Returns `(org_id, workspace_id, user_id)`; workspace_id == org_id.
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

async fn seed_container_agent(repo: &AgentRepository, scope: &TenantScope, ws: Uuid, name: &str) {
    repo.create_aggregate(
        scope,
        NewAgent::container(scope, CliToolKind::Codex, Some(name), None, None, ws, None, None)
            .expect("build container NewAgent"),
    )
    .await
    .expect("create container agent");
}

async fn seed_cli_agent(repo: &AgentRepository, scope: &TenantScope, ws: Uuid, name: &str) {
    repo.create_aggregate(
        scope,
        NewAgent::host_cli(scope, CliToolKind::Claude, HostCliIdentity::generate(), Some(name), None, None, ws, None)
            .expect("build host-cli NewAgent"),
    )
    .await
    .expect("create host-cli agent");
}

async fn seed_api_agent(repo: &AgentRepository, scope: &TenantScope, ws: Uuid, name: &str) {
    repo.create_aggregate(
        scope,
        NewAgent::api(scope, "anthropic", "claude-sonnet-4-6", Some(name), None, ws, None).expect("build api NewAgent"),
    )
    .await
    .expect("create api agent");
}

fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

/// Issue `GET /api/v1/admin/agents{query}` with the admin JWT and return
/// `(status, json_body)`.
async fn get_admin_agents(app: axum::Router, jwt: &str, query: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/admin/agents{query}"))
                .header("authorization", auth_header(jwt))
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The unfiltered admin agents list must serialise `runtimeKind` for every row
/// using the canonical `container | cli | api` slugs.
#[sqlx::test(migrations = "../db/migrations")]
async fn projection_includes_runtime_kind(pool: PgPool) {
    let (org_id, ws, user_id) = seed_admin_org(&pool).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    seed_container_agent(&repo, &scope, ws, "container-agent").await;
    seed_cli_agent(&repo, &scope, ws, "cli-agent").await;
    seed_api_agent(&repo, &scope, ws, "api-agent").await;

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = get_admin_agents(app, &jwt, "").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["ok"], true);

    let agents = body["agents"].as_array().expect("agents array");
    assert_eq!(agents.len(), 3, "expected the three seeded agents: {body}");

    // Every row must carry a canonical runtimeKind slug.
    let mut kinds: Vec<String> =
        agents.iter().map(|a| a["runtimeKind"].as_str().expect("runtimeKind string").to_string()).collect();
    kinds.sort();
    assert_eq!(kinds, vec!["api", "cli", "container"], "all canonical kinds must be present");
}

/// `?runtimeKind=cli` must return only host-CLI agents and exclude others.
#[sqlx::test(migrations = "../db/migrations")]
async fn filter_returns_only_requested_kind(pool: PgPool) {
    let (org_id, ws, user_id) = seed_admin_org(&pool).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    seed_container_agent(&repo, &scope, ws, "container-agent").await;
    seed_cli_agent(&repo, &scope, ws, "cli-agent-1").await;
    seed_cli_agent(&repo, &scope, ws, "cli-agent-2").await;
    seed_api_agent(&repo, &scope, ws, "api-agent").await;

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = get_admin_agents(app, &jwt, "?runtimeKind=cli").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let agents = body["agents"].as_array().expect("agents array");
    assert_eq!(agents.len(), 2, "expected exactly the two cli agents: {body}");
    assert!(agents.iter().all(|a| a["runtimeKind"] == "cli"), "every filtered row must be a cli agent: {body}");
    assert_eq!(body["total"], 2, "total must reflect the filtered count");
}

/// An unknown `runtimeKind` value must be rejected with HTTP 422 rather than
/// silently returning an empty list.
#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_runtime_kind_returns_422(pool: PgPool) {
    let (org_id, ws, user_id) = seed_admin_org(&pool).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());
    seed_container_agent(&repo, &scope, ws, "container-agent").await;

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    // "host_cli" is a legacy/UI label, NOT a canonical RuntimeKind slug.
    let (status, body) = get_admin_agents(app, &jwt, "?runtimeKind=host_cli").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");
}

/// Filtering by kind must keep per-org attribution: an org-B cli agent and an
/// org-A cli agent both appear under `?runtimeKind=cli`, each tagged with its
/// own organization, while a container agent never leaks into the cli filter.
#[sqlx::test(migrations = "../db/migrations")]
async fn filter_preserves_org_attribution(pool: PgPool) {
    // Org A: one container agent + one cli agent.
    let (org_a, ws_a, user_a) = seed_admin_org(&pool).await;
    let scope_a = tenant_scope_for_ids(org_a, user_a);
    // Org B: one cli agent only.
    let (org_b, ws_b, user_b) = seed_admin_org(&pool).await;
    let scope_b = tenant_scope_for_ids(org_b, user_b);

    let repo = AgentRepository::new(pool.clone());
    seed_container_agent(&repo, &scope_a, ws_a, "org-a-container").await;
    seed_cli_agent(&repo, &scope_a, ws_a, "org-a-cli").await;
    seed_cli_agent(&repo, &scope_b, ws_b, "org-b-cli").await;

    // Admin authenticated as org A's owner can still see across tenants.
    let jwt = mint_test_jwt(org_a, user_a, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = get_admin_agents(app, &jwt, "?runtimeKind=cli").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let agents = body["agents"].as_array().expect("agents array");
    assert_eq!(agents.len(), 2, "both cli agents (org A + org B) must appear: {body}");
    assert!(
        agents.iter().all(|a| a["runtimeKind"] == "cli"),
        "the container agent must not leak into the cli filter: {body}"
    );
    let names: Vec<&str> = agents.iter().filter_map(|a| a["name"].as_str()).collect();
    assert!(names.contains(&"org-a-cli"), "org A's cli agent must appear: {body}");
    assert!(names.contains(&"org-b-cli"), "org B's cli agent must appear: {body}");
    assert!(!names.contains(&"org-a-container"), "container agent must be filtered out: {body}");
}

/// Sanity: the same `RuntimeKind` slugs used in the API surface round-trip
/// through the strict parser, so the route + DB agree on the canonical set.
#[test]
fn canonical_runtime_kind_slugs_round_trip() {
    for kind in [RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
        assert_eq!(RuntimeKind::parse_legacy(kind.as_str()).unwrap(), kind);
    }
}
