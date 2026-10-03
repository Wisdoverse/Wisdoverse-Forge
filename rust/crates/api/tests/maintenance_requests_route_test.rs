//! Real HTTP/auth/SQL contracts with a local GitHub provider; no remote writes.

use std::sync::Arc;
use std::time::Duration;

use agentforge_api::create_router;
use agentforge_api::test_support::{app_state_with_mock_provider, mint_test_jwt, test_app_config};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use httpmock::prelude::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn destination(pool: &PgPool, user: Uuid) -> (Uuid, Uuid) {
    let org = Uuid::new_v4();
    let team = Uuid::new_v4();
    let project = Uuid::new_v4();
    let group = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id,name,slug) VALUES ($1,'Test',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (id,organization_id,name) VALUES ($1,$1,'Test')")
        .bind(org)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO teams (id,organization_id,name,slug) VALUES ($1,$2,'Test',$3)")
        .bind(team)
        .bind(org)
        .bind(team.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO projects (id,organization_id,workspace_id,team_id,name,slug) VALUES ($1,$2,$2,$3,'Test',$4)",
    )
    .bind(project)
    .bind(org)
    .bind(team)
    .bind(project.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO groups (id,organization_id,project_id,name,created_by) VALUES ($1,$2,$3,'Maintenance',$4)",
    )
    .bind(group)
    .bind(org)
    .bind(project)
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
    (org, group)
}

async fn call(app: &Router, jwt: Option<&str>, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(jwt) = jwt {
        request = request.header("authorization", format!("Bearer {jwt}"));
    }
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64_000).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

fn brief(group: Uuid, source: Value) -> Value {
    json!({"groupId":group,"title":"Repair dependency","brief":"Update and verify the dependency.","source":source})
}

fn pr(number: i32, state: &str, merged: bool, sha: char) -> Value {
    json!({"number":number,"state":state,"merged":merged,"head":{"sha":sha.to_string().repeat(40)},
        "base":{"ref":"develop","repo":{"full_name":"acme/widgets"}},"body":"Do not import this PR body", "html_url":"https://example.com"})
}

// One test owns the test-only provider-base environment variable. Independent
// integration-test binaries run in separate processes and cannot share it.
#[sqlx::test(migrations = "../db/migrations")]
async fn maintenance_intake_and_trace_preserve_tenant_authority_and_atomic_lineage(pool: PgPool) {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email,is_admin) VALUES ($1,'dev@example.com',true)")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let (org, group) = destination(&pool, user).await;
    let (other_org, other_group) = destination(&pool, user).await;
    let jwt = mint_test_jwt(org, user, "owner");
    let other_jwt = mint_test_jwt(other_org, user, "owner");
    let server = MockServer::start_async().await;
    unsafe {
        std::env::set_var("GITHUB_API_BASE", server.base_url());
    }
    server.mock_async(|when,then| {
        when.method(POST).path("/app/installations/1/access_tokens");
        then.status(201).json_body(json!({"token":"test-installation-token","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339(),"permissions":{"contents":"write","pull_requests":"write","checks":"read"}}));
    }).await;
    let repo_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets");
            then.status(200).json_body(
                json!({"default_branch":"develop","archived":false,"disabled":false,"allow_squash_merge":true}),
            );
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/git/ref/heads/develop");
            then.status(200).json_body(json!({"object":{"sha":"b".repeat(40)}}));
        })
        .await;
    let source_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/42");
            then.status(200).json_body(pr(42, "open", false, 'a'));
        })
        .await;
    let mut state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let mut config = test_app_config("postgres://test");
    config.github_app_id = Some("12345".into());
    config.github_app_installation_id = Some("1".into());
    config.github_app_private_key = Some(include_str!("fixtures/test_rsa_private_key.pem").to_string().into());
    config.github_app_repo = Some("acme/widgets".into());
    state.config = Arc::new(config);
    let app = create_router(state);
    let payload = brief(group, json!({"kind":"pull_request","number":42}));

    assert_eq!(
        call(&app, None, "POST", "/api/v1/self-fix/requests", payload.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE users SET is_admin=false WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    assert_eq!(
        call(&app, Some(&jwt), "POST", "/api/v1/self-fix/requests", payload.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(repo_mock.calls_async().await, 0, "org owner JWT does not confer platform authority");
    sqlx::query("UPDATE users SET is_admin=true WHERE id=$1").bind(user).execute(&pool).await.unwrap();

    let outcomes = futures::future::join_all(
        (0..8).map(|_| call(&app, Some(&jwt), "POST", "/api/v1/self-fix/requests", payload.clone())),
    )
    .await;
    assert!(outcomes.iter().all(|(status, _)| *status == StatusCode::OK));
    let task = outcomes[0].1["data"]["taskId"].as_str().unwrap().to_owned();
    let task_id = Uuid::parse_str(&task).unwrap();
    assert!(outcomes.iter().all(|(_, body)| body["data"]["taskId"] == task));
    assert_eq!(outcomes.iter().filter(|(_, body)| body["data"]["reused"] == false).count(), 1);
    let stored: (String, bool, Option<Uuid>, Value) =
        sqlx::query_as("SELECT status,self_fix,assigned_agent_id,params FROM orchestration_tasks WHERE id=$1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, "backlog");
    assert!(stored.1);
    assert!(stored.2.is_none());
    assert_eq!(stored.3["maintenanceSource"]["startingSha"], "b".repeat(40));
    assert!(!stored.3.to_string().contains("Do not import"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM maintenance_requests").fetch_one(&pool).await.unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orchestration_tasks").fetch_one(&pool).await.unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orchestration_outbox").fetch_one(&pool).await.unwrap(),
        0,
        "submission never dispatches work"
    );

    let trace_path = format!("/api/v1/self-fix/tasks/{task}/trace");
    let calls = source_mock.calls_async().await;
    assert_eq!(call(&app, Some(&other_jwt), "GET", &trace_path, Value::Null).await.0, StatusCode::NOT_FOUND);
    assert_eq!(source_mock.calls_async().await, calls, "foreign task is rejected before GitHub access");
    let (status, body) = call(&app, Some(&jwt), "GET", &trace_path, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["sourceState"]["status"], "observed");
    assert_eq!(body["data"]["producedState"]["status"], "not_created");

    // Same source in a different tenant gets its own task. A foreign place
    // cannot be used even by a platform administrator.
    assert_eq!(
        call(
            &app,
            Some(&other_jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(other_group, json!({"kind":"pull_request","number":42}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            Some(&jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(other_group, json!({"kind":"request","reference":"foreign-place"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let before =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orchestration_tasks").fetch_one(&pool).await.unwrap();

    // Inject a source-write failure: the task insert must roll back with it.
    sqlx::query("CREATE FUNCTION reject_test_source() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test write failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_test_source BEFORE INSERT ON maintenance_requests FOR EACH ROW EXECUTE FUNCTION reject_test_source()").execute(&pool).await.unwrap();
    assert_eq!(
        call(
            &app,
            Some(&jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(group, json!({"kind":"request","reference":"rollback"}))
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orchestration_tasks").fetch_one(&pool).await.unwrap(),
        before
    );
    sqlx::query("DROP TRIGGER reject_test_source ON maintenance_requests").execute(&pool).await.unwrap();
    let replay = call(
        &app,
        Some(&jwt),
        "POST",
        "/api/v1/self-fix/requests",
        brief(group, json!({"kind":"request","reference":" DEP-2026 "})),
    )
    .await;
    assert_eq!(replay.0, StatusCode::OK);
    let manual_task = replay.1["data"]["taskId"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            Some(&jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(group, json!({"kind":"request","reference":"dep-2026"}))
        )
        .await
        .1["data"]["taskId"],
        manual_task
    );
    assert_eq!(
        call(&app, Some(&jwt), "GET", &format!("/api/v1/self-fix/tasks/{manual_task}/trace"), Value::Null).await.1["data"]
            ["sourceState"]["status"],
        "not_applicable"
    );

    // An older task has no source. Use it to prove the tenant FK itself, rather
    // than accidentally failing on a task/source uniqueness constraint first.
    let legacy = Uuid::new_v4();
    sqlx::query("INSERT INTO orchestration_tasks (id,organization_id,title,created_by,self_fix) VALUES ($1,$2,'Older maintenance',$3,true)").bind(legacy).bind(org).bind(user).execute(&pool).await.unwrap();
    let legacy_trace =
        call(&app, Some(&jwt), "GET", &format!("/api/v1/self-fix/tasks/{legacy}/trace"), Value::Null).await;
    assert_eq!(legacy_trace.0, StatusCode::OK);
    assert_eq!(legacy_trace.1["data"], Value::Null);
    let error = sqlx::query("INSERT INTO maintenance_requests (organization_id,task_id,repository,source_kind,source_reference,default_branch,starting_sha) VALUES ($1,$2,'acme/widgets','request','forged','develop','revision')").bind(other_org).bind(legacy).execute(&pool).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(), Some("23503"));
    let error = sqlx::query("INSERT INTO maintenance_requests (organization_id,task_id,repository,source_kind,source_reference,source_head_sha,default_branch,starting_sha) VALUES ($1,$2,'acme/widgets','pull_request','999','head','develop','revision')").bind(org).bind(legacy).execute(&pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514"),
        "NULL cannot bypass the PR-source check"
    );

    source_mock.delete_async().await;
    let changed_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/42");
            then.status(200).json_body(pr(42, "closed", true, 'c'));
        })
        .await;
    let agent = Uuid::new_v4();
    let run = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id,organization_id,workspace_id,user_id,name,status) VALUES ($1,$2,$2,$3,'Test','idle')",
    )
    .bind(agent)
    .bind(org)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO task_runs (id,organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status) VALUES ($1,$2,$2,$3,$4,'attempt-1','completed')").bind(run).bind(org).bind(task_id).bind(agent).execute(&pool).await.unwrap();
    sqlx::query("UPDATE orchestration_tasks SET pr_number=43,pr_head_sha=$2,base_commit_sha=$3 WHERE id=$1")
        .bind(task_id)
        .bind("d".repeat(40))
        .bind("e".repeat(40))
        .execute(&pool)
        .await
        .unwrap();
    server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/43");
            then.status(200).json_body(pr(43, "closed", true, 'f'));
        })
        .await;
    let trace = call(&app, Some(&jwt), "GET", &trace_path, Value::Null).await.1["data"].clone();
    assert_eq!(trace["source"]["submittedHeadSha"], "a".repeat(40));
    assert_eq!(trace["sourceState"]["snapshot"]["state"], "merged");
    assert_eq!(trace["sourceState"]["headChanged"], true);
    assert_eq!(trace["producedState"]["headChanged"], true);
    assert_eq!(trace["producedState"]["snapshot"]["state"], "merged");
    assert_eq!(trace["executions"][0]["runId"], run.to_string());
    assert_eq!(trace["rebuildBaseSha"], "e".repeat(40));
    assert_eq!(trace["startingSha"], "b".repeat(40));

    changed_mock.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/42");
            then.status(503).body("upstream private diagnostic");
        })
        .await;
    let trace = call(&app, Some(&jwt), "GET", &trace_path, Value::Null).await.1["data"].clone();
    assert_eq!(trace["sourceState"]["status"], "unavailable");
    assert_eq!(trace["sourceState"]["snapshot"], Value::Null);
    assert_eq!(trace["source"]["submittedHeadSha"], "a".repeat(40));
    assert!(!trace.to_string().contains("private diagnostic"));
    assert_eq!(
        call(&app, Some(&jwt), "POST", "/api/v1/self-fix/requests", payload.clone()).await.1["data"]["taskId"],
        task,
        "retry succeeds during provider outage"
    );

    server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/44");
            then.status(200).json_body(pr(44, "closed", true, 'a'));
        })
        .await;
    assert_eq!(
        call(
            &app,
            Some(&jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(group, json!({"kind":"pull_request","number":44}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let slow = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/45");
            then.status(200).delay(Duration::from_millis(500)).json_body(pr(45, "open", false, 'a'));
        })
        .await;
    let pending = call(
        &app,
        Some(&jwt),
        "POST",
        "/api/v1/self-fix/requests",
        brief(group, json!({"kind":"pull_request","number":45})),
    );
    let revoke = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while slow.calls_async().await == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        sqlx::query("UPDATE users SET is_admin=false WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    };
    let (result, ()) = tokio::join!(pending, revoke);
    assert_eq!(result.0, StatusCode::FORBIDDEN, "permission revoked during provider I/O cannot commit a task");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM maintenance_requests WHERE source_reference='45'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(call(&app, Some(&jwt), "GET", &trace_path, Value::Null).await.0, StatusCode::FORBIDDEN);
    unsafe {
        std::env::remove_var("GITHUB_API_BASE");
    }
}
