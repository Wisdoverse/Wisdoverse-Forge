//! Unit 2.5 coverage for context provenance links and feedback.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::repositories::orchestration::context_link::{ContextLinkRepository, CreateContextLinkRecord};
use agentforge_api::test_support::{mint_test_jwt_with_axes, test_app_with_mock_provider};
use agentforge_core::{OrgId, TenantScope, UserId, WorkspaceId};

struct ContextSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    owner_id: Uuid,
    owner_jwt: String,
    other_org_jwt: String,
    run_id: Uuid,
    second_run_id: Uuid,
    other_org_run_id: Uuid,
    memory_id: Uuid,
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
    let value = if bytes.is_empty() { json!({}) } else { serde_json::from_slice(&bytes).expect("json") };
    (status, value)
}

fn tenant_scope(org_id: Uuid, user_id: Uuid, workspace_id: Uuid) -> TenantScope {
    TenantScope::with_axes(
        OrgId::from(org_id),
        UserId::from(user_id),
        Some(WorkspaceId::from(workspace_id)),
        None,
        None,
    )
}

async fn seed_context(pool: &PgPool) -> ContextSeed {
    let org_id = Uuid::new_v4();
    let other_org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let teammate_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let other_agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let other_task_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let second_run_id = Uuid::new_v4();
    let other_org_run_id = Uuid::new_v4();
    let memory_id = Uuid::new_v4();

    for (id, name) in [(org_id, "Org"), (other_org_id, "Other Org")] {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(format!("{name} {id}"))
            .bind(format!("org-{id}"))
            .execute(pool)
            .await
            .expect("seed org");
    }
    for (id, org, name) in [(workspace_id, org_id, "Default"), (other_workspace_id, other_org_id, "Other")] {
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(org)
            .bind(name)
            .execute(pool)
            .await
            .expect("seed workspace");
    }

    for (user_id, org, role) in
        [(owner_id, org_id, "owner"), (teammate_id, org_id, "member"), (other_user_id, other_org_id, "owner")]
    {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org)
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .expect("seed org member");
    }

    for (agent_id, org, workspace, user_id) in
        [(agent_id, org_id, workspace_id, owner_id), (other_agent_id, other_org_id, other_workspace_id, other_user_id)]
    {
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, cli_tool, status, runtime_kind)
             VALUES ($1, $2, $3, $4, 'codex', 'idle', 'container')",
        )
        .bind(agent_id)
        .bind(org)
        .bind(workspace)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed agent");
    }

    for (task_id, org, creator, agent) in
        [(task_id, org_id, owner_id, agent_id), (other_task_id, other_org_id, other_user_id, other_agent_id)]
    {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by, assigned_agent_id)
             VALUES ($1, $2, 'Ship context', 'completed', $3, $4)",
        )
        .bind(task_id)
        .bind(org)
        .bind(creator)
        .bind(agent)
        .execute(pool)
        .await
        .expect("seed task");
    }

    for (run, org, workspace, task, agent, key) in [
        (run_id, org_id, workspace_id, task_id, agent_id, "seed-run"),
        (second_run_id, org_id, workspace_id, task_id, agent_id, "seed-run-2"),
        (other_org_run_id, other_org_id, other_workspace_id, other_task_id, other_agent_id, "other-run"),
    ] {
        sqlx::query(
            "INSERT INTO task_runs (
                 id, organization_id, workspace_id, orchestration_task_id, agent_id,
                 idempotency_key, status, started_at, finished_at, capability_profile
             )
             VALUES ($1, $2, $3, $4, $5, $6, 'completed', now(), now(), '{}'::jsonb)",
        )
        .bind(run)
        .bind(org)
        .bind(workspace)
        .bind(task)
        .bind(agent)
        .bind(key)
        .execute(pool)
        .await
        .expect("seed task run");
    }

    sqlx::query(
        "INSERT INTO memory_items (
             id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
             title, content, visibility, sensitivity, provenance, state
         )
         VALUES ($1, $2, $3, $4, 'user', $4, 'Deploy memory',
                 'Use make prod-ext after the main pipeline succeeds.',
                 'shared', 'internal', '{}'::jsonb, 'active')",
    )
    .bind(memory_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(owner_id)
    .execute(pool)
    .await
    .expect("seed memory");

    let owner_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(workspace_id), None, None);
    let other_org_jwt =
        mint_test_jwt_with_axes(other_org_id, other_user_id, "owner", Some(other_workspace_id), None, None);

    ContextSeed {
        org_id,
        workspace_id,
        owner_id,
        owner_jwt,
        other_org_jwt,
        run_id,
        second_run_id,
        other_org_run_id,
        memory_id,
    }
}

async fn record_feedback(app: Router, jwt: &str, run_id: Uuid, item_id: Uuid, label: &str) -> (StatusCode, Value) {
    json_request(
        app,
        Method::POST,
        "/api/v1/context/feedback",
        jwt,
        Some(json!({
            "run_id": run_id,
            "item_kind": "memory",
            "item_id": item_id,
            "label": label,
            "note": "verified in test"
        })),
    )
    .await
}

#[sqlx::test(migrations = "../db/migrations")]
async fn context_link_records_applied_memory_and_lists_runs(pool: PgPool) {
    let seed = seed_context(&pool).await;
    let repo = ContextLinkRepository::new(pool.clone());
    let scope = tenant_scope(seed.org_id, seed.owner_id, seed.workspace_id);
    let mut tx = pool.begin().await.expect("begin tx");

    let link = ContextLinkRepository::create_in_tx(
        &mut tx,
        &scope,
        CreateContextLinkRecord {
            workspace_id: WorkspaceId::from(seed.workspace_id),
            item_id: seed.memory_id,
            item_kind: "memory",
            ref_id: seed.run_id,
            ref_kind: "run",
            link_type: "applied",
            created_by_user_id: UserId::from(seed.owner_id),
        },
    )
    .await
    .expect("create context link");
    tx.commit().await.expect("commit");
    assert_eq!(link.item_kind, "memory");
    assert_eq!(link.ref_kind, "run");
    assert_eq!(link.link_type, "applied");

    let rows = repo.runs_for_item(&scope, seed.memory_id, "memory", 20, 0).await.expect("runs for item");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].run_id, seed.run_id);
    assert_eq!(rows[0].run_status, "completed");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn context_link_explain_uses_covering_item_index(pool: PgPool) {
    let seed = seed_context(&pool).await;
    let scope = tenant_scope(seed.org_id, seed.owner_id, seed.workspace_id);
    let mut tx = pool.begin().await.expect("begin tx");
    ContextLinkRepository::create_in_tx(
        &mut tx,
        &scope,
        CreateContextLinkRecord {
            workspace_id: WorkspaceId::from(seed.workspace_id),
            item_id: seed.memory_id,
            item_kind: "memory",
            ref_id: seed.run_id,
            ref_kind: "run",
            link_type: "applied",
            created_by_user_id: UserId::from(seed.owner_id),
        },
    )
    .await
    .expect("create context link");
    sqlx::query("SET LOCAL enable_seqscan = off").execute(&mut *tx).await.expect("disable seqscan");
    let plan =
        ContextLinkRepository::explain_runs_for_item_in_tx(&mut tx, seed.memory_id, "memory").await.expect("explain");
    assert!(
        plan.iter().any(|line| line.contains("idx_context_links_item_ref_cover")),
        "expected covering index in plan: {plan:?}"
    );
    tx.rollback().await.expect("rollback");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn useful_feedback_updates_last_verified_and_duplicate_upserts(pool: PgPool) {
    let seed = seed_context(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, first) = record_feedback(app.clone(), &seed.owner_jwt, seed.run_id, seed.memory_id, "useful").await;
    assert_eq!(status, StatusCode::OK, "body: {first}");
    let first_feedback_id = first["data"]["feedback"]["id"].as_str().expect("feedback id").to_string();

    let (status, second) = record_feedback(app, &seed.owner_jwt, seed.run_id, seed.memory_id, "useful").await;
    assert_eq!(status, StatusCode::OK, "body: {second}");
    assert_eq!(second["data"]["feedback"]["id"], first_feedback_id);

    let feedback_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM context_feedback")
        .fetch_one(&pool)
        .await
        .expect("feedback count");
    assert_eq!(feedback_count, 1, "duplicate feedback must upsert");

    let last_verified: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_verified_at FROM memory_items WHERE id = $1")
            .bind(seed.memory_id)
            .fetch_one(&pool)
            .await
            .expect("last_verified");
    assert!(last_verified.is_some(), "useful feedback should verify memory");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn wrong_feedback_threshold_revokes_memory_but_revoked_feedback_still_records(pool: PgPool) {
    let seed = seed_context(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, first_wrong) =
        record_feedback(app.clone(), &seed.owner_jwt, seed.run_id, seed.memory_id, "wrong").await;
    assert_eq!(status, StatusCode::OK, "body: {first_wrong}");
    let state = sqlx::query_scalar::<_, String>("SELECT state FROM memory_items WHERE id = $1")
        .bind(seed.memory_id)
        .fetch_one(&pool)
        .await
        .expect("state after first wrong");
    assert_eq!(state, "active");

    let (status, second_wrong) =
        record_feedback(app.clone(), &seed.owner_jwt, seed.second_run_id, seed.memory_id, "wrong").await;
    assert_eq!(status, StatusCode::OK, "body: {second_wrong}");
    let (state, revoked_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT state, revoked_at FROM memory_items WHERE id = $1")
            .bind(seed.memory_id)
            .fetch_one(&pool)
            .await
            .expect("revoked state");
    assert_eq!(state, "revoked");
    assert!(revoked_at.is_some());

    let third_run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO task_runs (
             id, organization_id, workspace_id, orchestration_task_id, agent_id,
             idempotency_key, status, started_at, finished_at, capability_profile
         )
         SELECT $1, organization_id, workspace_id, orchestration_task_id, agent_id,
                'seed-run-3', 'completed', now(), now(), '{}'::jsonb
           FROM task_runs WHERE id = $2",
    )
    .bind(third_run_id)
    .bind(seed.run_id)
    .execute(&pool)
    .await
    .expect("third run");

    let (status, revoked_feedback) = record_feedback(app, &seed.owner_jwt, third_run_id, seed.memory_id, "wrong").await;
    assert_eq!(status, StatusCode::OK, "body: {revoked_feedback}");
    let feedback_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM context_feedback WHERE item_id = $1")
        .bind(seed.memory_id)
        .fetch_one(&pool)
        .await
        .expect("feedback count");
    assert_eq!(feedback_count, 3);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cross_tenant_feedback_is_forbidden(pool: PgPool) {
    let seed = seed_context(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, body) =
        record_feedback(app, &seed.other_org_jwt, seed.other_org_run_id, seed.memory_id, "useful").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    let feedback_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM context_feedback")
        .fetch_one(&pool)
        .await
        .expect("feedback count");
    assert_eq!(feedback_count, 0);
}
