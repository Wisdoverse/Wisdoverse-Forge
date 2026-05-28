//! Unit 3.3a coverage for API-side context envelope fetch.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::create_router;
use agentforge_api::health::ContextFeatureFlags;
use agentforge_api::test_support::{
    app_state_with_mock_provider, mint_test_jwt_with_axes, test_app_with_mock_provider,
};

struct EnvelopeSeed {
    user_id: Uuid,
    agent_id: Uuid,
    task_id: Uuid,
    run_id: Uuid,
    jwt: String,
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

async fn seed_envelope(pool: &PgPool) -> EnvelopeSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(workspace_id)
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
        .expect("seed org member");
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("platform-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(team_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed team member");
    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $3, $4, 'Context', $5)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .bind(format!("context-{project_id}"))
    .execute(pool)
    .await
    .expect("seed project");
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed project member");
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, cli_tool, status, runtime_kind)
         VALUES ($1, $2, $3, $4, $5, 'claude-agent', 'claude', 'idle', 'container')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, description, status, created_by)
         VALUES ($1, $2, 'Deploy governed context', 'Run make prod-ext and use Claude context', 'queued', $3)",
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed task");
    sqlx::query(
        "INSERT INTO task_runs (
             id, organization_id, workspace_id, orchestration_task_id, agent_id,
             idempotency_key, status, started_at, capability_profile
         )
         VALUES ($1, $2, $3, $4, $5, $6, 'working', now(), '{}'::jsonb)",
    )
    .bind(run_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(task_id)
    .bind(agent_id)
    .bind(run_id.to_string())
    .execute(pool)
    .await
    .expect("seed task run");
    sqlx::query(
        r#"INSERT INTO memory_items (
               organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
               title, content, visibility, sensitivity, confidence, last_verified_at, state
           )
           VALUES ($1, $2, $3, 'project', $4, 'prod-ext evidence', $5, 'shared', 'internal', 0.95, $6, 'active')"#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(project_id)
    .bind("After main pipeline succeeds, run make prod-ext and verify health.")
    .bind(Utc::now() + Duration::minutes(1))
    .execute(pool)
    .await
    .expect("seed memory");

    let jwt = mint_test_jwt_with_axes(org_id, user_id, "owner", Some(workspace_id), Some(team_id), Some(project_id));
    EnvelopeSeed { user_id, agent_id, task_id, run_id, jwt }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn context_envelope_fetch_returns_runtime_neutral_payload_with_memory_content(pool: PgPool) {
    let seed = seed_envelope(&pool).await;
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;

    let (status, body) = json_request(
        app,
        Method::POST,
        "/api/v1/context/envelope",
        &seed.jwt,
        Some(json!({
            "agent_id": seed.agent_id,
            "task_id": seed.task_id,
            "run_id": seed.run_id,
            "supported_versions": ["v1"]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["envelope_version"], "v1");
    assert_eq!(body["data"]["run_id"], json!(seed.run_id));
    assert_eq!(body["data"]["agent_id"], json!(seed.agent_id));
    assert_eq!(body["data"]["capability"]["cli_tool"], "claude");
    assert_eq!(body["data"]["applied"][0]["title"], "prod-ext evidence");
    assert_eq!(
        body["data"]["applied"][0]["content"],
        "After main pipeline succeeds, run make prod-ext and verify health."
    );
    assert_eq!(body["data"]["applied"][0]["source"]["source_type"], "memory_item");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn context_envelope_rejects_cross_tenant_agent_fetch(pool: PgPool) {
    let seed = seed_envelope(&pool).await;
    let other_org = Uuid::new_v4();
    let other_workspace = Uuid::new_v4();
    let other_agent = Uuid::new_v4();
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(other_org)
        .bind(format!("Other Org {other_org}"))
        .bind(format!("other-org-{other_org}"))
        .execute(&pool)
        .await
        .expect("seed other org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Other')")
        .bind(other_workspace)
        .bind(other_org)
        .execute(&pool)
        .await
        .expect("seed other workspace");
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, cli_tool, status, runtime_kind)
         VALUES ($1, $2, $3, $4, 'other-agent', 'claude', 'idle', 'container')",
    )
    .bind(other_agent)
    .bind(other_org)
    .bind(other_workspace)
    .bind(seed.user_id)
    .execute(&pool)
    .await
    .expect("seed other agent");

    let (status, body) = json_request(
        app,
        Method::POST,
        "/api/v1/context/envelope",
        &seed.jwt,
        Some(json!({
            "agent_id": other_agent,
            "task_id": seed.task_id,
            "run_id": seed.run_id,
            "supported_versions": ["v1"]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn governed_context_routes_are_disabled_by_default_flags(pool: PgPool) {
    let seed = seed_envelope(&pool).await;
    let mut state = app_state_with_mock_provider(pool, "mock", "unused").await;
    state.context_features = ContextFeatureFlags::default();
    let app = create_router(state);

    let (status, body) = json_request(app.clone(), Method::GET, "/api/v1/context/features", &seed.jwt, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["governance"], false);
    assert_eq!(body["data"]["preview"], false);
    assert_eq!(body["data"]["injection"], false);
    assert_eq!(body["data"]["analytics"], false);

    let disabled_requests = [
        (
            Method::POST,
            "/api/v1/context/envelope",
            Some(json!({
                "agent_id": seed.agent_id,
                "task_id": seed.task_id,
                "run_id": seed.run_id,
                "supported_versions": ["v1"]
            })),
            "context.injection.enabled is disabled",
        ),
        (
            Method::POST,
            "/api/v1/context/previews",
            Some(json!({
                "taskId": seed.task_id,
                "agentId": seed.agent_id
            })),
            "context.preview.enabled is disabled",
        ),
        (Method::GET, "/api/v1/governance/audit", None, "context.governance.enabled is disabled"),
        (Method::GET, "/api/v1/analytics/context-usage", None, "context.analytics.enabled is disabled"),
    ];

    for (method, uri, body, expected_message) in disabled_requests {
        let (status, body) = json_request(app.clone(), method, uri, &seed.jwt, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
        assert!(body["error"]["message"].as_str().unwrap_or_default().contains(expected_message), "{uri}: {body}");
    }
}
