//! Integration tests for the platform-admin authorization gate on cross-org
//! `/admin/*` endpoints (#881).
//!
//! These prove the privilege-escalation / tenant-isolation hole is closed:
//! every cross-org admin endpoint is now gated on the server-side
//! `users.is_admin` flag, NOT the self-assignable per-org JWT membership role.
//! A self-registered user is always `owner` of their personal org, so gating on
//! the role would let any registered user reach cross-org surfaces (read every
//! org's users, escalate themselves to admin, impersonate anyone).
//!
//! Coverage:
//!   - a non-admin org OWNER (JWT role = "owner", `users.is_admin = false`) is
//!     rejected with 403 on user listing, self-escalation, impersonation, stats
//!   - a full router walk asserts that same 403 across ALL 16 switched
//!     `/admin/*` endpoints, so a future hand-edit cannot revert the gate on a
//!     single handler unnoticed
//!   - a platform admin (`users.is_admin = true`) succeeds on those endpoints
//!   - `GET /me` exposes the global `isAdmin` flag for both admin and non-admin
//!   - migration 072 bootstraps the oldest account when no admin exists, and is
//!     a no-op when one already does (run against the real migration file via
//!     `include_str!`)
//!
//! Each test runs against a fresh database via `#[sqlx::test]`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use agentforge_api::{
    domain::agent::NewAgent,
    repositories::agent::AgentRepository,
    test_support::{mint_test_jwt, tenant_scope_for_ids, test_app_with_mock_provider},
};
use agentforge_core::CliToolKind;
use sqlx::PgPool;
use uuid::Uuid;

/// Seed an org + workspace + a user with the given global `is_admin` flag +
/// `owner` membership. Returns `(org_id, user_id)`.
async fn seed_org_owner(pool: &PgPool, is_admin: bool) -> (Uuid, Uuid) {
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
    sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .bind(is_admin)
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");
    (org_id, user_id)
}

/// Seed a bare second user (no membership) the operator can act on.
async fn seed_plain_user(pool: &PgPool) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, false)")
        .bind(user_id)
        .bind(format!("target-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed target user");
    user_id
}

async fn send(app: axum::Router, method: &str, uri: &str, jwt: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {jwt}"));
    let request = match body {
        Some(value) => request
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&value).unwrap()))
            .unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };
    let resp = app.oneshot(request).await.expect("request");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Cross-org 403 regression: a non-admin org OWNER must be rejected everywhere.
// ---------------------------------------------------------------------------

/// `GET /admin/users` — a non-admin owner cannot list every org's users.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_owner_cannot_list_users(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) = send(app, "GET", "/api/v1/admin/users", &jwt, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "owner without is_admin must not list cross-org users");
}

/// `PUT /admin/users/{id}` — the escalation path. A non-admin owner cannot
/// promote anyone (including, by extension, themselves) to admin.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_owner_cannot_escalate_roles(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let target = seed_plain_user(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) =
        send(app, "PUT", &format!("/api/v1/admin/users/{target}"), &jwt, Some(json!({ "role": "admin" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "role escalation must be platform-admin-only");
}

/// `POST /admin/impersonate` — a non-admin owner cannot impersonate anyone.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_owner_cannot_impersonate(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let target = seed_plain_user(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) =
        send(app, "POST", "/api/v1/admin/impersonate", &jwt, Some(json!({ "target_user_id": target }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "impersonation must be platform-admin-only");
}

/// `GET /admin/stats` — a non-admin owner cannot read platform-wide stats.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_owner_cannot_read_stats(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, _body) = send(app, "GET", "/api/v1/admin/stats", &jwt, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "platform stats must be platform-admin-only");
}

/// Full router walk: a single non-admin org OWNER (`users.is_admin = false`,
/// membership role `owner`, JWT role `"owner"`) must be rejected with 403 on
/// EVERY cross-org `/admin/*` endpoint switched to the platform-admin gate in
/// #881. The individual tests above assert the security-critical surfaces in
/// detail; this table-driven test extends that to all 16 switched endpoints so a
/// future hand-edit cannot silently revert the gate on any single handler.
///
/// The platform-admin gate runs BEFORE any path-param lookup or not-found logic,
/// so placeholder UUIDs / tool slugs for `{id}` / `{tool}` still yield 403 (the
/// caller is rejected before the handler ever resolves the target). Bodies are
/// supplied only where the route requires a JSON body to reach the gate.
#[sqlx::test(migrations = "../db/migrations")]
async fn non_admin_owner_is_forbidden_on_every_switched_admin_endpoint(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");

    // Placeholder path params. The gate rejects before any not-found logic, so
    // these never need to exist.
    let placeholder_id = Uuid::nil();
    let placeholder_tool = "codex";

    // (method, uri, body) for ALL 16 endpoints switched to the platform-admin
    // gate in #881. `/admin/control-plane` is intentionally absent: it stays on
    // the per-org `require_admin` gate (org-scoped data), not the platform-admin
    // gate, so a non-admin owner is NOT expected to 403 there.
    let body = || Some(json!({ "target_user_id": Uuid::nil() }));
    let cases: Vec<(&str, String, Option<Value>)> = vec![
        ("GET", "/api/v1/admin/users".to_string(), None),
        ("PUT", format!("/api/v1/admin/users/{placeholder_id}"), Some(json!({ "role": "admin" }))),
        ("DELETE", format!("/api/v1/admin/users/{placeholder_id}"), None),
        ("GET", "/api/v1/admin/organizations".to_string(), None),
        ("POST", "/api/v1/admin/impersonate".to_string(), body()),
        ("POST", "/api/v1/admin/impersonate/end".to_string(), None),
        ("GET", "/api/v1/admin/impersonation-log".to_string(), None),
        ("GET", "/api/v1/admin/stats".to_string(), None),
        ("GET", "/api/v1/admin/agents".to_string(), None),
        ("GET", format!("/api/v1/admin/agents/{placeholder_id}"), None),
        ("DELETE", format!("/api/v1/admin/agents/{placeholder_id}"), None),
        ("DELETE", "/api/v1/admin/agents".to_string(), Some(json!({ "ids": [] }))),
        ("GET", "/api/v1/admin/cli-images".to_string(), None),
        ("POST", format!("/api/v1/admin/cli-images/{placeholder_tool}/roll"), None),
        ("POST", format!("/api/v1/admin/cli-images/{placeholder_tool}/build"), None),
        ("GET", "/api/v1/admin/dead-events".to_string(), None),
    ];
    assert_eq!(cases.len(), 16, "must cover all 16 switched endpoints");

    for (method, uri, body) in cases {
        // Fresh app per request: `oneshot` consumes the router.
        let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
        let (status, json) = send(app, method, &uri, &jwt, body).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "non-admin owner must be 403 on {method} {uri}, got {status}: {json}"
        );
    }
}

// ---------------------------------------------------------------------------
// Platform-admin 200: the same surfaces succeed for a real platform admin.
// ---------------------------------------------------------------------------

/// A platform admin (`users.is_admin = true`) reaches the same endpoints.
#[sqlx::test(migrations = "../db/migrations")]
async fn platform_admin_can_use_cross_org_admin_endpoints(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, true).await;
    let target = seed_plain_user(&pool).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");

    // List users.
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = send(app, "GET", "/api/v1/admin/users", &jwt, None).await;
    assert_eq!(status, StatusCode::OK, "platform admin lists users: {body}");
    assert_eq!(body["ok"], true);

    // Stats.
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = send(app, "GET", "/api/v1/admin/stats", &jwt, None).await;
    assert_eq!(status, StatusCode::OK, "platform admin reads stats: {body}");
    assert_eq!(body["ok"], true);

    // Role change on a managed (non-owner) target succeeds.
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) =
        send(app, "PUT", &format!("/api/v1/admin/users/{target}"), &jwt, Some(json!({ "role": "admin" }))).await;
    assert_eq!(status, StatusCode::OK, "platform admin promotes a user: {body}");
    assert_eq!(body["user"]["role"], "admin");

    // Impersonation of a different user succeeds.
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) =
        send(app, "POST", "/api/v1/admin/impersonate", &jwt, Some(json!({ "target_user_id": target }))).await;
    assert_eq!(status, StatusCode::OK, "platform admin impersonates: {body}");
    assert_eq!(body["ok"], true);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn platform_admin_agent_delete_honors_lifecycle_admission(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, true).await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let agent_id = AgentRepository::new(pool.clone())
        .create_aggregate(
            &scope,
            NewAgent::container(
                &scope,
                CliToolKind::Codex,
                Some("admin-delete-target"),
                None,
                None,
                org_id,
                None,
                None,
            )
            .expect("build Agent"),
        )
        .await
        .expect("create Agent");
    sqlx::query("UPDATE agents SET interactive_lease_expires_at = NOW() + INTERVAL '1 minute' WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("lease Agent");
    let jwt = mint_test_jwt(org_id, user_id, "owner");

    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, _body) = send(app, "DELETE", &format!("/api/v1/admin/agents/{agent_id}"), &jwt, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "active interactive work must block admin deletion");

    sqlx::query("UPDATE agents SET interactive_lease_expires_at = NULL WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("release Agent");
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = send(app, "DELETE", &format!("/api/v1/admin/agents/{agent_id}"), &jwt, None).await;
    assert_eq!(status, StatusCode::OK, "idle Agent can be deleted: {body}");
    let remains: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE id = $1").bind(agent_id).fetch_one(&pool).await.unwrap();
    assert_eq!(remains, 0);
}

// ---------------------------------------------------------------------------
// /me isAdmin
// ---------------------------------------------------------------------------

/// `GET /me` exposes the global `isAdmin` flag (true for an admin).
#[sqlx::test(migrations = "../db/migrations")]
async fn me_reports_is_admin_true_for_platform_admin(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, true).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = send(app, "GET", "/api/v1/me", &jwt, None).await;
    assert_eq!(status, StatusCode::OK, "me: {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["isAdmin"], true, "platform admin's /me reports isAdmin=true: {body}");
}

/// `GET /me` reports `isAdmin=false` for a non-admin, even an org owner.
#[sqlx::test(migrations = "../db/migrations")]
async fn me_reports_is_admin_false_for_non_admin(pool: PgPool) {
    let (org_id, user_id) = seed_org_owner(&pool, false).await;
    let jwt = mint_test_jwt(org_id, user_id, "owner");
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = send(app, "GET", "/api/v1/me", &jwt, None).await;
    assert_eq!(status, StatusCode::OK, "me: {body}");
    assert_eq!(body["isAdmin"], false, "a non-admin owner's /me reports isAdmin=false: {body}");
    // The legacy contract fields are preserved.
    assert_eq!(body["role"], "owner");
}

// ---------------------------------------------------------------------------
// Migration 072 bootstrap
// ---------------------------------------------------------------------------

/// Migration 072 already ran as part of `migrations` (a no-op on the empty test
/// DB). Re-running its body must be a no-op once an admin exists, and it promotes
/// the oldest account when none does. We exercise both by seeding rows and
/// invoking the migration's SQL directly (idempotent by construction).
///
/// `include_str!` pulls the EXACT migration file rather than an inlined copy, so
/// these tests cannot silently drift from what production runs: editing
/// `072_bootstrap_platform_admin.sql` re-exercises that change here. The path is
/// relative to this source file (`rust/crates/api/tests/`).
const BOOTSTRAP_SQL: &str = include_str!("../../db/migrations/072_bootstrap_platform_admin.sql");

/// With no admin present, the bootstrap promotes the OLDEST surviving account.
#[sqlx::test(migrations = "../db/migrations")]
async fn migration_072_promotes_oldest_when_no_admin(pool: PgPool) {
    // Two non-admin users with explicit, ordered created_at timestamps.
    let oldest = Uuid::new_v4();
    let newest = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, is_admin, created_at) VALUES ($1, $2, false, $3)")
        .bind(oldest)
        .bind(format!("oldest-{oldest}@example.com"))
        .bind(chrono::Utc::now() - chrono::Duration::days(2))
        .execute(&pool)
        .await
        .expect("seed oldest");
    sqlx::query("INSERT INTO users (id, email, is_admin, created_at) VALUES ($1, $2, false, $3)")
        .bind(newest)
        .bind(format!("newest-{newest}@example.com"))
        .bind(chrono::Utc::now())
        .execute(&pool)
        .await
        .expect("seed newest");

    sqlx::query(BOOTSTRAP_SQL).execute(&pool).await.expect("run bootstrap");

    let oldest_admin: bool =
        sqlx::query_scalar("SELECT is_admin FROM users WHERE id = $1").bind(oldest).fetch_one(&pool).await.unwrap();
    let newest_admin: bool =
        sqlx::query_scalar("SELECT is_admin FROM users WHERE id = $1").bind(newest).fetch_one(&pool).await.unwrap();
    assert!(oldest_admin, "the oldest account is promoted");
    assert!(!newest_admin, "no other account is promoted");
}

/// When an admin already exists, the bootstrap is a no-op (it promotes no one
/// else, and re-running it is harmless).
#[sqlx::test(migrations = "../db/migrations")]
async fn migration_072_is_noop_when_admin_exists(pool: PgPool) {
    let existing_admin = Uuid::new_v4();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, is_admin, created_at) VALUES ($1, $2, true, $3)")
        .bind(existing_admin)
        .bind(format!("admin-{existing_admin}@example.com"))
        .bind(chrono::Utc::now())
        .execute(&pool)
        .await
        .expect("seed existing admin");
    // An OLDER non-admin: if the guard were missing this would wrongly be promoted.
    sqlx::query("INSERT INTO users (id, email, is_admin, created_at) VALUES ($1, $2, false, $3)")
        .bind(other)
        .bind(format!("other-{other}@example.com"))
        .bind(chrono::Utc::now() - chrono::Duration::days(5))
        .execute(&pool)
        .await
        .expect("seed older non-admin");

    sqlx::query(BOOTSTRAP_SQL).execute(&pool).await.expect("run bootstrap");

    let other_admin: bool =
        sqlx::query_scalar("SELECT is_admin FROM users WHERE id = $1").bind(other).fetch_one(&pool).await.unwrap();
    assert!(!other_admin, "no additional admin is minted once one exists");
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin AND deleted_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(admin_count, 1, "exactly the pre-existing admin remains");
}
