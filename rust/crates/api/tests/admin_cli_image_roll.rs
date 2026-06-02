//! Integration tests for `POST /api/v1/admin/cli-images/{tool}/roll` — the
//! operator-initiated roll of running container agents onto a new image.
//!
//! Exercises the full HTTP path through the Axum router:
//!   - a non-admin JWT is rejected with 403 (admin-gated)
//!   - `claude` and unknown tools are rejected with 422 (not 404)
//!   - no running agents for a tool → 200 with an empty no-op report
//!   - a seeded container agent is enumerated and attempted; with no Docker in
//!     the test harness the per-agent roll fails gracefully (recorded, not a
//!     500), proving enumeration + the best-effort per-agent contract
//!
//! The live-Docker roll path itself is verified on staging (see the PR).

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

async fn seed_container_agent_with_container(
    pool: &PgPool,
    repo: &AgentRepository,
    scope: &TenantScope,
    ws: Uuid,
    name: &str,
) {
    let agent_id = repo
        .create_aggregate(
            scope,
            NewAgent::container(scope, CliToolKind::Codex, Some(name), None, None, ws, None, None)
                .expect("build container NewAgent"),
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

async fn roll(app: axum::Router, jwt: &str, tool: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/cli-images/{tool}/roll"))
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

#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_is_forbidden(pool: PgPool) {
    let (org_id, _ws, user_id) = seed_admin_org(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "member");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) = roll(app, &jwt, "codex").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn claude_and_unknown_tools_are_rejected_422(pool: PgPool) {
    let (org_id, _ws, user_id) = seed_admin_org(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = roll(app, &jwt, "claude").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "claude is never rollable: {body}");

    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) = roll(app, &jwt, "nonsense").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn no_running_agents_is_an_empty_noop(pool: PgPool) {
    let (org_id, _ws, user_id) = seed_admin_org(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = roll(app, &jwt, "codex").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["tool"], "codex");
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn idle_agent_to_roll_without_docker_is_one_runtime_error(pool: PgPool) {
    let (org_id, ws, user_id) = seed_admin_org(&pool).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());
    seed_container_agent_with_container(&pool, &repo, &scope, ws, "codex-agent").await;

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) = roll(app, &jwt, "codex").await;

    // There IS an idle agent to roll but the harness has no Docker runtime, so the
    // roll fails ONCE with 503 (a clear environment-level error) rather than N
    // redacted per-agent "internal error" lines.
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn working_agents_are_skipped_not_rolled(pool: PgPool) {
    let (org_id, ws, user_id) = seed_admin_org(&pool).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());
    seed_container_agent_with_container(&pool, &repo, &scope, ws, "busy-codex").await;
    // Force the agent into the `working` state (an in-flight assignment).
    sqlx::query("UPDATE agents SET status = 'working' WHERE cli_tool = 'codex'")
        .execute(&pool)
        .await
        .expect("set working");

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = roll(app, &jwt, "codex").await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    // The busy agent is counted but NOT rolled — no Docker call, no interruption.
    assert_eq!(body["data"]["total"], 1, "the busy agent is still counted: {body}");
    assert_eq!(body["data"]["skippedBusy"], 1, "a working agent is skipped: {body}");
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 0, "a skipped agent is never attempted: {body}");
    assert_eq!(body["data"]["results"].as_array().expect("results").len(), 0);
}
