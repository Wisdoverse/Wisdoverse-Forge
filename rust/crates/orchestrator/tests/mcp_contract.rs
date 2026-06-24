use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 128 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

fn mcp_request(body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri("/mcp").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn mcp_route_stays_unmounted_without_explicit_enablement() {
    let app = AppState::test_ready().router();
    let response = app
        .oneshot(mcp_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), None))
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mcp_requires_auth_when_enabled() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();
    let (status, body) =
        json_response(app, mcp_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), None))
            .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({"error": "missing authorization header"}));
}

#[tokio::test]
async fn mcp_initialize_and_tools_list_match_go_tool_surface() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();

    let (status, initialize) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "contract-test", "version": "1.0.0"}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initialize["jsonrpc"], "2.0");
    assert_eq!(initialize["id"], 1);
    assert_eq!(initialize["result"]["serverInfo"]["name"], "agentforge-orchestrator");
    assert_eq!(initialize["result"]["serverInfo"]["version"], "1.0.0");
    assert!(initialize["result"]["capabilities"]["tools"].is_object());

    let (status, tools_list) = json_response(
        app,
        mcp_request(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}), Some("secret-token")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tools_list["jsonrpc"], "2.0");
    assert_eq!(tools_list["id"], 2);
    let tool_names: Vec<&str> = tools_list["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(
        tool_names,
        vec![
            "orchestrator.task.list",
            "orchestrator.task.create",
            "orchestrator.task.get",
            "orchestrator.review.list",
            "orchestrator.review.approve",
            "orchestrator.review.reject",
            "orchestrator.review.comment",
        ]
    );
}

#[tokio::test]
async fn mcp_task_tools_support_create_list_and_get_round_trip() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();

    let (status, created) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.create",
                    "arguments": {
                        "title": "Build feature X",
                        "description": "Implement the new feature",
                        "priority": "high"
                    }
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["result"]["isError"], false);
    let created_text = created["result"]["content"][0]["text"].as_str().expect("create text");
    let created_task: Value = serde_json::from_str(created_text).expect("created task json");
    let task_id = created_task["id"].as_str().expect("task id").to_string();
    assert_eq!(created_task["title"], "Build feature X");
    assert_eq!(created_task["priority"], "high");
    assert_eq!(created_task["state"], "pending");
    assert_eq!(created_task["createdBy"], "mcp");
    assert_eq!(created_task["orgId"], "org-test");

    let (status, listed) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.list",
                    "arguments": {"state": "pending"}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["result"]["isError"], false);
    let listed_text = listed["result"]["content"][0]["text"].as_str().expect("list text");
    let listed_tasks: Value = serde_json::from_str(listed_text).expect("listed tasks json");
    let tasks = listed_tasks.as_array().expect("tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], task_id);

    let (status, got) = json_response(
        app,
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.get",
                    "arguments": {"id": task_id}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["result"]["isError"], false);
    let got_text = got["result"]["content"][0]["text"].as_str().expect("get text");
    let got_task: Value = serde_json::from_str(got_text).expect("got task json");
    assert_eq!(got_task["title"], "Build feature X");
    assert_eq!(got_task["id"], task_id);
}

// --- MCP review tools wired to the real store (#841) -------------------------
//
// These `#[sqlx::test]` cases drive the `/mcp` endpoint against a real Postgres
// store, mirroring the HTTP review contract. They prove an MCP-originated verdict
// matches the HTTP path: `changes_requested`/`approved` + task sync + audit row +
// persisted feedback comment, and that verdicts are org-scoped + state-guarded.

use sqlx::PgPool;

/// MCP request carrying the internal-token auth plus the per-request org + user
/// headers the review tools resolve their identity from.
fn mcp_request_with_identity(body: Value, token: &str, org_id: &str, user_id: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("X-Org-ID", org_id)
        .header("X-User-ID", user_id)
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Seed a participant (the review creator), a task in `review` state, and a
/// pending code_review for `org_id`. Returns (task_id, review_id) as UUID text.
async fn seed_task_and_pending_review(pool: &PgPool, org_id: &str) -> (String, String) {
    let creator: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'review-creator', $1, $2)
         RETURNING id::text",
    )
    .bind(format!("casdoor-creator-{org_id}"))
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed creator participant");

    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('mcp-review-task', 'review', CAST($1 AS uuid), $2)
         RETURNING id::text",
    )
    .bind(&creator)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed task");

    let review_id: String = sqlx::query_scalar(
        "INSERT INTO code_reviews (task_id, session_id, diff_ref, state, org_id, created_by)
         VALUES (CAST($1 AS uuid), 'mcp-session', 'HEAD', 'pending', $2, CAST($3 AS uuid))
         RETURNING id::text",
    )
    .bind(&task_id)
    .bind(org_id)
    .bind(&creator)
    .fetch_one(pool)
    .await
    .expect("seed code_review");

    (task_id, review_id)
}

async fn mcp_review_app(pool: PgPool, org_id: &str) -> axum::Router {
    AppState::test_mcp_pg(pool, "secret-token", org_id).router()
}

/// review.reject via MCP → review row `changes_requested`, task synced to
/// `changes_requested`, a `review.reject` audit row (valid actor_type), and the
/// feedback comment persisted — identical to the HTTP reject path.
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_reject_syncs_review_task_audit_and_comment(pool: PgPool) {
    let org_id = "org-mcp-reject";
    let (task_id, review_id) = seed_task_and_pending_review(&pool, org_id).await;
    let app = mcp_review_app(pool.clone(), org_id).await;

    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.review.reject",
                    "arguments": {"id": review_id, "feedback": "needs major refactoring"}
                }
            }),
            "secret-token",
            org_id,
            "mcp-reviewer",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "reject should not be an error: {body}");

    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "changes_requested");

    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = CAST($1 AS uuid)")
        .bind(&task_id)
        .fetch_one(&pool)
        .await
        .expect("fetch task state");
    assert_eq!(task_state, "changes_requested", "task must be synced to changes_requested");

    let (audit_count, actor_type): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(actor_type) FROM audit_logs
         WHERE action = 'review.reject' AND resource_id = $1 AND org_id = $2",
    )
    .bind(&review_id)
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("fetch audit");
    assert_eq!(audit_count, 1, "exactly one review.reject audit row");
    assert_eq!(actor_type.as_deref(), Some("system"), "internal-token actor_type must be 'system'");

    let (comment_count, comment_body): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(body) FROM review_comments WHERE review_id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("fetch comments");
    assert_eq!(comment_count, 1, "feedback comment persisted");
    assert_eq!(comment_body.as_deref(), Some("needs major refactoring"));
}

/// review.approve via MCP → review `approved`, task synced to `completed`, and a
/// `review.approve` audit row.
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_approve_syncs_review_task_and_audit(pool: PgPool) {
    let org_id = "org-mcp-approve";
    let (task_id, review_id) = seed_task_and_pending_review(&pool, org_id).await;
    let app = mcp_review_app(pool.clone(), org_id).await;

    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/call",
                "params": {"name": "orchestrator.review.approve", "arguments": {"id": review_id}}
            }),
            "secret-token",
            org_id,
            "mcp-approver",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "approve should not be an error: {body}");

    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "approved");

    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = CAST($1 AS uuid)")
        .bind(&task_id)
        .fetch_one(&pool)
        .await
        .expect("fetch task state");
    assert_eq!(task_state, "completed", "approve must sync task to completed");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'review.approve' AND resource_id = $1 AND org_id = $2",
    )
    .bind(&review_id)
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("fetch audit");
    assert_eq!(audit_count, 1, "exactly one review.approve audit row");
}

/// Tenant isolation: a caller in org B cannot reject org A's review. The verdict
/// must be a tool error and org A's review must remain `pending`.
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_reject_is_org_scoped(pool: PgPool) {
    let org_a = "org-mcp-tenant-a";
    let org_b = "org-mcp-tenant-b";
    let (_task_a, review_a) = seed_task_and_pending_review(&pool, org_a).await;
    // org B caller targets org A's review id but authenticates as org B.
    let app = mcp_review_app(pool.clone(), org_b).await;

    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.review.reject",
                    "arguments": {"id": review_a, "feedback": "cross-tenant attempt"}
                }
            }),
            "secret-token",
            org_b,
            "org-b-user",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true, "cross-tenant reject must be a tool error");
    let text = body["result"]["content"][0]["text"].as_str().expect("error text");
    assert!(text.contains("not found"), "expected not-found error, got: {text}");

    // org A's review must be untouched.
    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_a)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "pending", "org A review must remain pending");
}

/// Illegal transition: rejecting an already-approved review is refused and leaves
/// no mutation. Mirrors the HTTP 409 contract (as an MCP tool error here).
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_reject_rejects_illegal_transition(pool: PgPool) {
    let org_id = "org-mcp-illegal";
    let (_task_id, review_id) = seed_task_and_pending_review(&pool, org_id).await;
    // Drive the review to a terminal `approved` state directly.
    sqlx::query("UPDATE code_reviews SET state = 'approved' WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .execute(&pool)
        .await
        .expect("set review approved");

    let app = mcp_review_app(pool.clone(), org_id).await;
    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 13,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.review.reject",
                    "arguments": {"id": review_id, "feedback": "too late"}
                }
            }),
            "secret-token",
            org_id,
            "mcp-reviewer",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true, "illegal transition must be a tool error");
    let text = body["result"]["content"][0]["text"].as_str().expect("error text");
    assert!(text.contains("transition"), "expected transition error, got: {text}");

    // No comment should have been written and the state stays approved.
    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "approved", "review must remain approved (no mutation)");

    let comment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM review_comments WHERE review_id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("count comments");
    assert_eq!(comment_count, 0, "no feedback comment on a refused transition");
}

// --- review.list org scoping + state filter (#841 review FIX 6) ---------------
//
// review.list is the most tenant-isolation-sensitive new path: a leak would expose
// another org's reviews to an LLM agent. These cases prove the result is scoped to
// the caller's org and that the `state` filter round-trips.

/// Seed an additional code_review in `state` for `org_id`, reusing the existing
/// creator participant + a fresh task. Returns the review id as UUID text.
async fn seed_review_in_state(pool: &PgPool, org_id: &str, state: &str) -> String {
    let creator: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'extra-creator', $1, $2)
         RETURNING id::text",
    )
    .bind(format!("casdoor-extra-{org_id}-{state}"))
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed extra creator");

    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('extra-review-task', 'review', CAST($1 AS uuid), $2)
         RETURNING id::text",
    )
    .bind(&creator)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed extra task");

    sqlx::query_scalar(
        "INSERT INTO code_reviews (task_id, session_id, diff_ref, state, org_id, created_by)
         VALUES (CAST($1 AS uuid), 'extra-session', 'HEAD', $2, $3, CAST($4 AS uuid))
         RETURNING id::text",
    )
    .bind(&task_id)
    .bind(state)
    .bind(org_id)
    .bind(&creator)
    .fetch_one(pool)
    .await
    .expect("seed extra code_review")
}

/// Parse the JSON array of reviews out of a successful `review.list` tool result.
fn review_ids_from_tool_result(body: &Value) -> Vec<String> {
    assert_eq!(body["result"]["isError"], false, "review.list should not be an error: {body}");
    let text = body["result"]["content"][0]["text"].as_str().expect("list text");
    let reviews: Value = serde_json::from_str(text).expect("reviews json array");
    reviews
        .as_array()
        .expect("reviews array")
        .iter()
        .map(|review| review["id"].as_str().expect("review id").to_string())
        .collect()
}

/// review.list is org-scoped: org B's caller sees ONLY org B's reviews, never org
/// A's. Also proves the `state` filter round-trips (pending vs approved).
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_list_is_org_scoped_and_state_filtered(pool: PgPool) {
    let org_a = "org-mcp-list-a";
    let org_b = "org-mcp-list-b";

    // Org A review (must never appear in org B's list).
    let (_task_a, review_a) = seed_task_and_pending_review(&pool, org_a).await;
    // Org B: one pending + one approved.
    let (_task_b, review_b_pending) = seed_task_and_pending_review(&pool, org_b).await;
    let review_b_approved = seed_review_in_state(&pool, org_b, "approved").await;

    let app = mcp_review_app(pool.clone(), org_b).await;

    // Unfiltered list as org B: exactly org B's two reviews, never org A's.
    let (status, body) = json_response(
        app.clone(),
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 20,
                "method": "tools/call",
                "params": {"name": "orchestrator.review.list", "arguments": {}}
            }),
            "secret-token",
            org_b,
            "org-b-lister",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ids = review_ids_from_tool_result(&body);
    assert!(ids.contains(&review_b_pending), "org B pending review must be listed");
    assert!(ids.contains(&review_b_approved), "org B approved review must be listed");
    assert!(!ids.contains(&review_a), "org A review must NOT leak into org B's list");
    assert_eq!(ids.len(), 2, "org B must see exactly its own two reviews: {ids:?}");

    // state=pending filter: only the pending org B review, not the approved one.
    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 21,
                "method": "tools/call",
                "params": {"name": "orchestrator.review.list", "arguments": {"state": "pending"}}
            }),
            "secret-token",
            org_b,
            "org-b-lister",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ids = review_ids_from_tool_result(&body);
    assert_eq!(ids, vec![review_b_pending.clone()], "state=pending must return only the pending review");
    assert!(!ids.contains(&review_b_approved), "approved review must be filtered out by state=pending");
}

// --- review.comment persist + audit + human actor (#841 review FIX 3 + FIX 7) -

/// review.comment via MCP persists the comment (including the `line` field) and
/// writes a best-effort `review.comment` audit row (FIX 3). The internal-token
/// caller records `actor_type = "system"`.
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_comment_persists_and_audits(pool: PgPool) {
    let org_id = "org-mcp-comment";
    let (_task_id, review_id) = seed_task_and_pending_review(&pool, org_id).await;
    let app = mcp_review_app(pool.clone(), org_id).await;

    let (status, body) = json_response(
        app,
        mcp_request_with_identity(
            json!({
                "jsonrpc": "2.0",
                "id": 22,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.review.comment",
                    "arguments": {"id": review_id, "body": "consider extracting this helper", "filePath": "src/lib.rs", "line": 42}
                }
            }),
            "secret-token",
            org_id,
            "mcp-commenter",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "comment should not be an error: {body}");

    // The comment persisted to the real store, with the line round-tripped.
    let (comment_count, comment_body, comment_line): (i64, Option<String>, Option<i32>) =
        sqlx::query_as("SELECT COUNT(*), MAX(body), MAX(line) FROM review_comments WHERE review_id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("fetch comments");
    assert_eq!(comment_count, 1, "comment persisted");
    assert_eq!(comment_body.as_deref(), Some("consider extracting this helper"));
    assert_eq!(comment_line, Some(42), "line field round-trips");

    // FIX 3: a best-effort review.comment audit row was written (HTTP path parity).
    let (audit_count, actor_type): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(actor_type) FROM audit_logs
         WHERE action = 'review.comment' AND resource_id = $1 AND org_id = $2",
    )
    .bind(&review_id)
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("fetch comment audit");
    assert_eq!(audit_count, 1, "exactly one review.comment audit row");
    assert_eq!(actor_type.as_deref(), Some("system"), "internal-token actor_type must be 'system'");
}

/// FIX 7: a session-JWT MCP reject records `actor_type = "human"`, mirroring the
/// existing internal-token (`"system"`) reject test. Proves `resolve_actor` derives
/// the honest human actor type from a session token end-to-end.
#[sqlx::test(migrations = "./migrations")]
async fn mcp_review_reject_records_human_actor_for_session_jwt(pool: PgPool) {
    const SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let org_id = "org-mcp-human";
    let (_task_id, review_id) = seed_task_and_pending_review(&pool, org_id).await;

    let state = AppState::test_mcp_pg_with_sessions(pool.clone(), "secret-token", org_id, SIGNING_KEY);
    // Mint a session access token for a human user in this org.
    let pair = state
        .sessions
        .as_ref()
        .expect("session manager")
        .issue_token_pair("user-human-1", "human@example.com", "Human Reviewer", org_id)
        .await
        .expect("issue token pair");
    let app = state.router();

    // Session JWT auth -> no internal token, no X-Org-ID header (the org comes from
    // the JWT claims). This is the "human" actor branch.
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", pair.access_token))
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 23,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.review.reject",
                    "arguments": {"id": review_id, "feedback": "needs work (human)"}
                }
            })
            .to_string(),
        ))
        .unwrap();

    let (status, body) = json_response(app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "session-JWT reject should not be an error: {body}");

    let (audit_count, actor_type): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(actor_type) FROM audit_logs
         WHERE action = 'review.reject' AND resource_id = $1 AND org_id = $2",
    )
    .bind(&review_id)
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("fetch audit");
    assert_eq!(audit_count, 1, "exactly one review.reject audit row");
    assert_eq!(actor_type.as_deref(), Some("human"), "session-JWT actor_type must be 'human'");
}
