//! Unit 2.4 coverage for context candidate approval flow.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt_with_axes, test_app_with_mock_provider};

struct ContextApprovalSeed {
    workspace_id: Uuid,
    other_workspace_id: Uuid,
    team_id: Uuid,
    owner_id: Uuid,
    owner_jwt: String,
    teammate_jwt: String,
    other_workspace_jwt: String,
    completed_run_id: Uuid,
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn json_request(
    app: Router,
    method: Method,
    uri: impl AsRef<str>,
    jwt: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri.as_ref()).header(header::AUTHORIZATION, bearer(jwt));
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app.oneshot(builder.body(body).expect("request body")).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("response body");
    let value = if bytes.is_empty() { json!({}) } else { serde_json::from_slice(&bytes).expect("json response") };
    (status, value)
}

async fn seed_context_approval(pool: &PgPool) -> ContextApprovalSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let teammate_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let completed_run_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    for (id, name) in [(workspace_id, "Default"), (other_workspace_id, "Other")] {
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(org_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("seed workspace");
    }

    for (user_id, role) in [(owner_id, "owner"), (teammate_id, "member")] {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .expect("seed org member");
    }

    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', 'platform')")
        .bind(team_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed team");
    for user_id in [owner_id, teammate_id] {
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(team_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed team member");
    }

    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $3, $4, 'Control Plane', 'control-plane')",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .execute(pool)
    .await
    .expect("seed project");

    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, cli_tool, status)
         VALUES ($1, $2, $3, $4, 'codex', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(owner_id)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by, assigned_agent_id)
         VALUES ($1, $2, 'Ship governed context', 'completed', $3, $4)",
    )
    .bind(task_id)
    .bind(org_id)
    .bind(owner_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed task");
    sqlx::query(
        "INSERT INTO task_runs (
             id, organization_id, workspace_id, orchestration_task_id, agent_id,
             idempotency_key, status, started_at, finished_at, capability_profile
         )
         VALUES ($1, $2, $3, $4, $5, 'seed-run', 'completed', now(), now(), '{}'::jsonb)",
    )
    .bind(completed_run_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(task_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed task run");

    let owner_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(workspace_id), None, None);
    let teammate_jwt = mint_test_jwt_with_axes(org_id, teammate_id, "member", Some(workspace_id), None, None);
    let other_workspace_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(other_workspace_id), None, None);

    ContextApprovalSeed {
        workspace_id,
        other_workspace_id,
        team_id,
        owner_id,
        owner_jwt,
        teammate_jwt,
        other_workspace_jwt,
        completed_run_id,
    }
}

async fn insert_memory_candidate(pool: &PgPool, seed: &ContextApprovalSeed, source_run_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO context_candidates (
               organization_id, workspace_id, source_run_id, item_kind,
               proposed_content, owner_user_id
           )
           VALUES (
               (SELECT organization_id FROM workspaces WHERE id = $1),
               $1, $2, 'memory', $3, $4
           )
           RETURNING id"#,
    )
    .bind(seed.workspace_id)
    .bind(source_run_id)
    .bind(json!({
        "title": "Production validation",
        "content": "Use make prod-ext after main pipeline success.",
        "visibility": "shared",
        "confidence": 0.91
    }))
    .bind(seed.owner_id)
    .fetch_one(pool)
    .await
    .expect("insert memory candidate")
}

async fn insert_skill_candidate(pool: &PgPool, seed: &ContextApprovalSeed, skill_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO context_candidates (
               organization_id, workspace_id, source_run_id, target_skill_id,
               item_kind, proposed_content, owner_user_id
           )
           VALUES (
               (SELECT organization_id FROM workspaces WHERE id = $1),
               $1, $2, $3, 'skill', $4, $5
           )
           RETURNING id"#,
    )
    .bind(seed.workspace_id)
    .bind(seed.completed_run_id)
    .bind(skill_id)
    .bind(json!({
        "name": "candidate-skill",
        "description": "promote existing skill in place"
    }))
    .bind(seed.owner_id)
    .fetch_one(pool)
    .await
    .expect("insert skill candidate")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn approving_memory_candidate_creates_governed_memory_once(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let candidate_id = insert_memory_candidate(&pool, &seed, Some(seed.completed_run_id)).await;

    let (status, pending) =
        json_request(app.clone(), Method::GET, "/api/v1/context/candidates", &seed.teammate_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {pending}");
    assert_eq!(
        pending["data"][0]["proposed_preview"]["content_preview"],
        "Use make prod-ext after main pipeline success."
    );

    let ttl = (Utc::now() + Duration::days(30)).to_rfc3339();
    let (status, rejected) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.teammate_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "ttl_at": ttl,
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {rejected}");

    let (state, approval_count, memory_count): (String, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT state FROM context_candidates WHERE id = $1),
             (SELECT COUNT(*) FROM context_approvals WHERE candidate_id = $1),
             (SELECT COUNT(*) FROM memory_items WHERE source_run_id = $2)",
    )
    .bind(candidate_id)
    .bind(seed.completed_run_id)
    .fetch_one(&pool)
    .await
    .expect("candidate should remain pending after expansion rejection");
    assert_eq!(state, "pending");
    assert_eq!(approval_count, 0);
    assert_eq!(memory_count, 0);

    let (status, approved) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.teammate_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "ttl_at": ttl,
            "sensitivity": "internal",
            "reason": "approved from approval queue",
            "confirm_expansion": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {approved}");
    assert_eq!(approved["data"]["candidate"]["state"], "approved");
    assert_eq!(approved["data"]["memory_item"]["scope_kind"], "team");
    assert_eq!(approved["data"]["memory_item"]["scope_id"], seed.team_id.to_string());
    assert_eq!(approved["data"]["memory_item"]["owner_user_id"], seed.owner_id.to_string());
    assert!(approved["data"]["memory_item"].get("content").is_none(), "memory response must not serialize content");

    let memory_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM memory_items WHERE source_run_id = $1")
        .bind(seed.completed_run_id)
        .fetch_one(&pool)
        .await
        .expect("memory count");
    assert_eq!(memory_count, 1);
    let (approval_count, approval_reason): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(reason) FROM context_approvals WHERE candidate_id = $1")
            .bind(candidate_id)
            .fetch_one(&pool)
            .await
            .expect("approval count and reason");
    assert_eq!(approval_count, 1);
    assert_eq!(approval_reason.as_deref(), Some("approved from approval queue"));

    let (status, duplicate) = json_request(
        app,
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.teammate_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "sensitivity": "internal",
            "confirm_expansion": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {duplicate}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn list_candidates_filters_and_marks_source_availability(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let available_id = insert_memory_candidate(&pool, &seed, Some(seed.completed_run_id)).await;
    let unavailable_id = insert_memory_candidate(&pool, &seed, None).await;

    let (status, skill_body) = json_request(
        app.clone(),
        Method::POST,
        "/api/v1/skills",
        &seed.owner_jwt,
        Some(json!({
            "name": "queue-filter-skill",
            "content": "Use queue filters before approval.",
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "state": "candidate"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {skill_body}");
    let skill_id = Uuid::parse_str(skill_body["data"]["id"].as_str().expect("skill id")).expect("skill uuid");
    let skill_candidate_id = insert_skill_candidate(&pool, &seed, skill_id).await;

    let (status, memory_body) = json_request(
        app.clone(),
        Method::GET,
        "/api/v1/context/candidates?state=all&item_kind=memory&scope_kind=user",
        &seed.teammate_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {memory_body}");
    let memory_rows = memory_body["data"].as_array().expect("memory candidates");
    assert_eq!(memory_rows.len(), 2);
    let available =
        memory_rows.iter().find(|row| row["id"] == available_id.to_string()).expect("available memory candidate");
    let unavailable =
        memory_rows.iter().find(|row| row["id"] == unavailable_id.to_string()).expect("unavailable memory candidate");
    assert_eq!(available["proposed_scope_kind"], "user");
    assert_eq!(available["source_available"], true);
    assert_eq!(unavailable["source_available"], false);

    let (status, skill_body) = json_request(
        app,
        Method::GET,
        "/api/v1/context/candidates?state=pending&item_kind=skill&scope_kind=team",
        &seed.teammate_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {skill_body}");
    let skill_rows = skill_body["data"].as_array().expect("skill candidates");
    assert_eq!(skill_rows.len(), 1);
    assert_eq!(skill_rows[0]["id"], skill_candidate_id.to_string());
    assert_eq!(skill_rows[0]["proposed_scope_kind"], "team");
    assert_eq!(skill_rows[0]["source_available"], true);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rejecting_candidate_is_terminal_and_creates_no_memory(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let candidate_id = insert_memory_candidate(&pool, &seed, Some(seed.completed_run_id)).await;

    let (status, rejected) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/reject"),
        &seed.teammate_jwt,
        Some(json!({ "reason": "not durable enough" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {rejected}");
    assert_eq!(rejected["data"]["candidate"]["state"], "rejected");

    let memory_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM memory_items").fetch_one(&pool).await.expect("memory count");
    assert_eq!(memory_count, 0);

    let (status, approve_after_reject) = json_request(
        app,
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.teammate_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {approve_after_reject}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn self_approval_to_team_fails_and_cross_workspace_is_not_found(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let candidate_id = insert_memory_candidate(&pool, &seed, Some(seed.completed_run_id)).await;

    let (status, self_team) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.owner_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {self_team}");

    let state = sqlx::query_scalar::<_, String>("SELECT state FROM context_candidates WHERE id = $1")
        .bind(candidate_id)
        .fetch_one(&pool)
        .await
        .expect("candidate state");
    assert_eq!(state, "pending");

    let (status, other_workspace_list) =
        json_request(app.clone(), Method::GET, "/api/v1/context/candidates", &seed.other_workspace_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {other_workspace_list}");
    assert!(other_workspace_list["data"].as_array().expect("candidate array").is_empty());

    let (status, cross_workspace_approve) = json_request(
        app,
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.other_workspace_jwt,
        Some(json!({
            "scope_kind": "user",
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {cross_workspace_approve}");
    assert_ne!(seed.workspace_id, seed.other_workspace_id);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn source_run_missing_auto_rejects_without_approval_row(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let candidate_id = insert_memory_candidate(&pool, &seed, None).await;

    let (status, body) = json_request(
        app,
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.owner_jwt,
        Some(json!({
            "scope_kind": "user",
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let (state, approval_count): (String, i64) = sqlx::query_as(
        "SELECT c.state, COUNT(a.id)
           FROM context_candidates c
           LEFT JOIN context_approvals a ON a.candidate_id = c.id
          WHERE c.id = $1
          GROUP BY c.state",
    )
    .bind(candidate_id)
    .fetch_one(&pool)
    .await
    .expect("state and approval count");
    assert_eq!(state, "rejected");
    assert_eq!(approval_count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn skill_candidate_promotes_existing_skill_in_place(pool: PgPool) {
    let seed = seed_context_approval(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, skill_body) = json_request(
        app.clone(),
        Method::POST,
        "/api/v1/skills",
        &seed.owner_jwt,
        Some(json!({
            "name": "candidate-skill",
            "content": "Use release evidence before rollout notes.",
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "state": "candidate"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {skill_body}");
    let skill_id = skill_body["data"]["id"].as_str().expect("skill id");
    assert_eq!(skill_body["data"]["state"], "candidate");

    let candidate_id = insert_skill_candidate(&pool, &seed, Uuid::parse_str(skill_id).expect("skill uuid")).await;

    let (status, approved) = json_request(
        app,
        Method::POST,
        format!("/api/v1/context/candidates/{candidate_id}/approve"),
        &seed.teammate_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "sensitivity": "internal"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {approved}");
    assert_eq!(approved["data"]["candidate"]["state"], "approved");
    assert_eq!(approved["data"]["skill"]["id"], skill_id);
    assert_eq!(approved["data"]["skill"]["state"], "active");
    assert_eq!(approved["data"]["skill"]["version"], 2);

    let skill_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM skills WHERE name = 'candidate-skill'")
        .fetch_one(&pool)
        .await
        .expect("skill count");
    assert_eq!(skill_count, 1, "approval must not create a parallel active skill");
    let version_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM skill_versions WHERE skill_id = $1")
        .bind(Uuid::parse_str(skill_id).expect("skill uuid"))
        .fetch_one(&pool)
        .await
        .expect("skill version count");
    assert_eq!(version_count, 1);
}
