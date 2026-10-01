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
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

fn brief(group: Uuid, source: Value) -> Value {
    json!({"groupId":group,"title":"Repair dependency","brief":"Update and verify the dependency.","source":source})
}

fn pr(number: i32, state: &str, merged: bool, sha: char) -> Value {
    json!({"number":number,"node_id":format!("PR-{number}"),"state":state,"merged":merged,"head":{"sha":sha.to_string().repeat(40)},
        "base":{"ref":"develop","repo":{"full_name":"acme/widgets"}},"body":"Do not import this PR body", "html_url":"https://example.com"})
}

async fn run(pool: &PgPool, org: Uuid, user: Uuid, task: Uuid, cli: &str, number: i32) -> Uuid {
    let agent = Uuid::new_v4();
    let run = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status,runtime_kind,cli_tool) VALUES ($1,$2,$2,$3,'idle','container',$4)").bind(agent).bind(org).bind(user).bind(cli).execute(pool).await.unwrap();
    let profile = json!({"runtime_kind":"container","cli_tool":cli,"image":{"source":"platform","imageId":format!("sha256:{}","1".repeat(64)),"manifestDigest":format!("sha256:{}","2".repeat(64)),"version":"fixture","versionSource":"label"}});
    sqlx::query("INSERT INTO task_runs (id,organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,finished_at,capability_profile) VALUES ($1,$2,$2,$3,$4,$5,'completed',now(),$6)").bind(run).bind(org).bind(task).bind(agent).bind(run.to_string()).bind(profile).execute(pool).await.unwrap();
    sqlx::query("UPDATE orchestration_tasks SET status='completed',base_commit_sha=$2,pr_head_sha=$3,pr_number=$4,review_status='in_review' WHERE id=$1").bind(task).bind("b".repeat(40)).bind("c".repeat(40)).bind(number).execute(pool).await.unwrap();
    run
}
fn report_input(data: &Value, run: Option<Uuid>) -> Value {
    json!({"requestKey":Uuid::new_v4(),"expectedVersion":data["taskVersion"],"expectedRevision":data["currentRevision"],"runId":run,
        "criteria":"The dependency tests pass.","scope":"Dependency constraints only.","environmentNotes":"Same disposable fixture.","cliVersion":"fixture-1",
        "changeSummary":"Updated constraints. Production remains unverified.","checks":[{"name":"dependency test","command":"npm run test:unit","status":"passed","evidence":"Reported by the reviewer."}],"unverified":["production deployment"],"comparisonKey":"equivalent-task-1"})
}
fn decision_input(data: &Value, report: &Value, verdict: &str) -> Value {
    json!({"requestKey":Uuid::new_v4(),"reportId":report["id"],"expectedVersion":data["taskVersion"],"expectedRevision":data["currentRevision"],"verdict":verdict,"reason":"Reviewed the exact revision; merge checks remain separate.","humanMinutes":{"setup":0,"handling":2,"review":3,"recovery":0,"rework":0,"operation":0},"baselineMinutes":10})
}

// All provider mutation is confined to this process and test. No real GitHub,
// agent container, reviewer approval or vendor execution is represented here.
#[sqlx::test(migrations = "../db/migrations")]
async fn maintenance_reports_reviews_recovery_comparison_and_full_cohorts(pool: PgPool) {
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
    unsafe { std::env::set_var("GITHUB_API_BASE", server.base_url()) };
    server.mock_async(|when,then| {when.method(POST).path("/app/installations/1/access_tokens");then.status(201).json_body(json!({"token":"test-installation-token","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339(),"permissions":{"contents":"write","pull_requests":"write","checks":"read"}}));}).await;
    server
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
    let mut pr_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/47");
            then.status(200).json_body(pr(47, "open", false, 'c'));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/48");
            then.status(200).json_body(pr(48, "open", false, 'c'));
        })
        .await;
    server.mock_async(|when,then| {when.method(GET).path(format!("/repos/acme/widgets/commits/{}/check-runs","c".repeat(40)));then.status(200).json_body(json!({"total_count":1,"check_runs":[{"name":"CI failure","head_sha":"c".repeat(40),"status":"completed","conclusion":"failure","details_url":"https://example.com/private"}]}));}).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/repos/acme/widgets/commits/{}/statuses", "c".repeat(40)));
            then.status(200).json_body(json!([]));
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

    for path in ["/api/v1/self-fix/outcomes", "/api/v1/self-fix/comparison?reportIds=invalid"] {
        assert_eq!(call(&app, None, "GET", path, json!({})).await.0, StatusCode::UNAUTHORIZED);
    }
    let mut ids = vec![];
    for reference in ["report-one", "report-two"] {
        let (status, body) = call(
            &app,
            Some(&jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(group, json!({"kind":"request","reference":reference})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        ids.push(Uuid::parse_str(body["data"]["taskId"].as_str().unwrap()).unwrap());
    }
    let task = ids[0];
    let delivery_path = format!("/api/v1/self-fix/tasks/{task}/delivery");
    let reports_path = format!("/api/v1/self-fix/tasks/{task}/reports");
    let decisions_path = format!("/api/v1/self-fix/tasks/{task}/decisions");
    let handoff_path = format!("/api/v1/self-fix/tasks/{task}/handoffs");
    assert_eq!(call(&app, Some(&other_jwt), "GET", &delivery_path, json!({})).await.0, StatusCode::NOT_FOUND);
    let before = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await.1["data"].clone();
    let missing = report_input(&before, None);
    assert_eq!(call(&app, Some(&jwt), "POST", &reports_path, missing.clone()).await.0, StatusCode::BAD_REQUEST);
    let mut explained = missing;
    explained["noArtifactReason"] = json!("Execution has not started.");
    let (status, body) = call(&app, Some(&jwt), "POST", &reports_path, explained).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        call(&app, Some(&jwt), "POST", &decisions_path, decision_input(&before, &body["data"], "accepted")).await.0,
        StatusCode::BAD_REQUEST
    );

    let first_run = run(&pool, org, user, task, "codex", 47).await;
    let second_run = run(&pool, org, user, ids[1], "claude", 48).await;
    let mut data = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await.1["data"].clone();
    assert_eq!(data["currentChecks"]["checks"][0]["conclusion"], "failure");
    assert!(data.to_string().find("details_url").is_none());
    let input = report_input(&data, Some(first_run));
    let concurrent =
        futures::future::join_all((0..8).map(|_| call(&app, Some(&jwt), "POST", &reports_path, input.clone()))).await;
    assert!(concurrent.iter().all(|(status, _)| *status == StatusCode::OK), "{concurrent:?}");
    let first = concurrent[0].1["data"].clone();
    assert!(concurrent.iter().all(|(_, body)| body["data"]["id"] == first["id"]));
    let mut changed = input.clone();
    changed["criteria"] = json!("Different criteria");
    assert_eq!(call(&app, Some(&jwt), "POST", &reports_path, changed).await.0, StatusCode::CONFLICT);
    let mut stale = input.clone();
    stale["requestKey"] = json!(Uuid::new_v4());
    stale["expectedVersion"] = json!(0);
    assert_eq!(call(&app, Some(&jwt), "POST", &reports_path, stale).await.0, StatusCode::CONFLICT);
    let verdict_input = decision_input(&data, &first, "accepted");
    let (status, verdict) = call(&app, Some(&jwt), "POST", &decisions_path, verdict_input.clone()).await;
    assert_eq!(status, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["data"]["totalMinutes"], 5);
    assert_eq!(verdict["data"]["github"]["status"], "observed");
    assert_eq!(
        call(&app, Some(&jwt), "POST", &decisions_path, verdict_input).await.1["data"]["id"],
        verdict["data"]["id"]
    );
    // Human acceptance never relaxes the existing automated merge checks.
    assert_eq!(
        call(&app, Some(&jwt), "POST", &format!("/api/v1/self-fix/tasks/{task}/approve"), json!({})).await.0,
        StatusCode::BAD_REQUEST
    );
    data = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await.1["data"].clone();
    let second_data = call(&app, Some(&jwt), "GET", &format!("/api/v1/self-fix/tasks/{}/delivery", ids[1]), json!({}))
        .await
        .1["data"]
        .clone();
    let (status, second) = call(
        &app,
        Some(&jwt),
        "POST",
        &format!("/api/v1/self-fix/tasks/{}/reports", ids[1]),
        report_input(&second_data, Some(second_run)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let compare_path = format!(
        "/api/v1/self-fix/comparison?reportIds={},{}",
        first["id"].as_str().unwrap(),
        second["data"]["id"].as_str().unwrap()
    );
    let comparison = call(&app, Some(&jwt), "GET", &compare_path, json!({})).await;
    assert_eq!(comparison.0, StatusCode::OK, "{:?}", comparison.1);
    assert_eq!(comparison.1["data"]["conditionsMatch"], true);
    assert_eq!(comparison.1["data"]["distinctClis"], 2);
    assert_eq!(call(&app, Some(&other_jwt), "GET", &compare_path, json!({})).await.0, StatusCode::BAD_REQUEST);
    let mut cross = decision_input(&data, &second["data"], "rework");
    cross["requestKey"] = json!(Uuid::new_v4());
    assert_eq!(call(&app, Some(&jwt), "POST", &decisions_path, cross).await.0, StatusCode::BAD_REQUEST);

    pr_mock.delete_async().await;
    pr_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/47");
            then.status(200).json_body(pr(47, "open", false, 'd'));
        })
        .await;
    assert_eq!(
        call(&app, Some(&jwt), "POST", &decisions_path, decision_input(&data, &first, "accepted")).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut partial = decision_input(&data, &first, "reopened");
    partial["humanMinutes"] = json!({"review":0});
    partial["baselineMinutes"] = json!(0);
    let reopened = call(&app, Some(&jwt), "POST", &decisions_path, partial).await;
    assert_eq!(reopened.0, StatusCode::OK, "{:?}", reopened.1);
    assert!(reopened.1["data"]["totalMinutes"].is_null());
    pr_mock.delete_async().await;
    pr_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/47");
            then.status(503).body("private provider failure");
        })
        .await;
    let unavailable = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await;
    assert_eq!(unavailable.0, StatusCode::OK);
    assert_eq!(unavailable.1["data"]["currentChecks"]["status"], "unavailable");
    assert!(!unavailable.1.to_string().contains("private provider failure"));
    assert_eq!(
        call(&app, Some(&jwt), "POST", &decisions_path, decision_input(&data, &first, "accepted")).await.0,
        StatusCode::BAD_REQUEST
    );

    // A role revoked while a slow provider response is in flight cannot write.
    pr_mock.delete_async().await;
    let slow = server
        .mock_async(|when, then| {
            when.method(GET).path("/repos/acme/widgets/pulls/47");
            then.status(200).delay(Duration::from_millis(700)).json_body(pr(47, "open", false, 'c'));
        })
        .await;
    let writing = call(&app, Some(&jwt), "POST", &reports_path, report_input(&data, Some(first_run)));
    let revoking = async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        sqlx::query("UPDATE users SET is_admin=false WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    };
    let (blocked, _) = tokio::join!(writing, revoking);
    assert_eq!(blocked.0, StatusCode::FORBIDDEN);
    assert!(slow.calls_async().await > 0);
    assert_eq!(call(&app, Some(&jwt), "GET", "/api/v1/self-fix/outcomes", json!({})).await.0, StatusCode::FORBIDDEN);
    sqlx::query("UPDATE users SET is_admin=true WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    slow.delete_async().await;

    sqlx::query(
        "UPDATE orchestration_tasks SET status='blocked',blocked_reason='waiting_input',retryable=false WHERE id=$1",
    )
    .bind(task)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO job_queue (queue,payload,unique_key,attempts,max_attempts,run_at) VALUES ('self_fix_pr',$1,$2,4,5,now()+interval '1 minute')").bind(json!({"org_id":org,"task_id":task})).bind(task.to_string()).execute(&pool).await.unwrap();
    let current = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await.1["data"].clone();
    assert_eq!(current["recovery"]["bridge"]["attempts"], 4);
    assert_eq!(current["recovery"]["bridge"]["limit"], 5);
    assert_eq!(current["recovery"]["manualRetryAllowed"], false);
    let handoff = json!({"requestKey":Uuid::new_v4(),"expectedVersion":current["taskVersion"],"expectedRevision":current["currentRevision"],"reason":"Need a person to resolve the blocker.","nextStep":"Inspect the preserved run and refresh the repository state."});
    let saved = call(&app, Some(&jwt), "POST", &handoff_path, handoff.clone()).await;
    assert_eq!(saved.0, StatusCode::OK, "{:?}", saved.1);
    assert_eq!(saved.1["data"]["snapshot"]["runId"], first_run.to_string());
    assert_eq!(call(&app, Some(&jwt), "POST", &handoff_path, handoff).await.1["data"]["id"], saved.1["data"]["id"]);
    sqlx::query("UPDATE orchestration_tasks SET status='queued',blocked_reason=NULL WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    let queued = call(&app, Some(&jwt), "GET", &delivery_path, json!({})).await.1["data"].clone();
    let mut active = json!({"requestKey":Uuid::new_v4(),"expectedVersion":queued["taskVersion"],"expectedRevision":queued["currentRevision"],"reason":"still active","nextStep":"stop first"});
    assert_eq!(call(&app, Some(&jwt), "POST", &handoff_path, active.take()).await.0, StatusCode::CONFLICT);

    // Explicit runtime removal keeps the immutable report and recorded run ID.
    sqlx::query("DELETE FROM agents WHERE id=(SELECT agent_id FROM task_runs WHERE id=$1)")
        .bind(first_run)
        .execute(&pool)
        .await
        .unwrap();
    let retained = call(
        &app,
        Some(&jwt),
        "GET",
        &format!("/api/v1/self-fix/reports/{}", first["id"].as_str().unwrap()),
        json!({}),
    )
    .await;
    assert_eq!(retained.0, StatusCode::OK);
    assert_eq!(retained.1["data"]["report"]["runId"], first_run.to_string());
    assert_eq!(retained.1["data"]["decision"]["verdict"], "reopened");

    // Migration 094 already retains runs after actor deletion. Explicit run
    // cleanup clears only our live link, preserving the captured report.
    sqlx::query("DELETE FROM task_runs WHERE id=$1").bind(first_run).execute(&pool).await.unwrap();
    let after_cleanup = call(
        &app,
        Some(&jwt),
        "GET",
        &format!("/api/v1/self-fix/reports/{}", first["id"].as_str().unwrap()),
        json!({}),
    )
    .await;
    assert_eq!(after_cleanup.1["data"]["report"], retained.1["data"]["report"]);
    let remaining_runs = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_runs WHERE orchestration_task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining_runs, 0);
    let current_projection:(Option<Uuid>,Option<Uuid>,bool,bool)=sqlx::query_as("SELECT r.run_id,run.id,r.run_id IS NOT DISTINCT FROM run.id,COALESCE(r.revision=COALESCE(t.pr_head_sha,t.base_commit_sha,m.starting_sha) AND r.run_id IS NOT DISTINCT FROM run.id AND r.snapshot->>'title'=t.title AND r.snapshot->'brief' IS NOT DISTINCT FROM COALESCE(to_jsonb(t.description),'null'::jsonb),false) FROM orchestration_tasks t JOIN maintenance_requests m ON m.task_id=t.id LEFT JOIN LATERAL (SELECT * FROM maintenance_verification_reports WHERE task_id=t.id ORDER BY created_at DESC,id DESC LIMIT 1) r ON true LEFT JOIN LATERAL (SELECT id FROM task_runs WHERE orchestration_task_id=t.id ORDER BY started_at DESC,created_at DESC,id DESC LIMIT 1) run ON true WHERE t.id=$1").bind(task).fetch_one(&pool).await.unwrap();
    assert!(!current_projection.3, "projection={current_projection:?}");

    // More than one page, including unreviewed failures/cancellations, plus a
    // foreign submission: denominator and tenant isolation must survive paging.
    for i in 0..105 {
        let id = Uuid::now_v7();
        let status = if i % 3 == 0 {
            "failed"
        } else if i % 3 == 1 {
            "canceled"
        } else {
            "backlog"
        };
        sqlx::query("INSERT INTO orchestration_tasks (id,organization_id,group_id,title,created_by,status,self_fix) VALUES ($1,$2,$3,'Unreviewed cohort task',$4,$5,true)").bind(id).bind(org).bind(group).bind(user).bind(status).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO maintenance_requests (organization_id,task_id,repository,source_kind,source_reference,default_branch,starting_sha) VALUES ($1,$2,'acme/widgets','request',$3,'develop',$4)").bind(org).bind(id).bind(format!("cohort-{i}")).bind("b".repeat(40)).execute(&pool).await.unwrap();
    }
    assert_eq!(
        call(
            &app,
            Some(&other_jwt),
            "POST",
            "/api/v1/self-fix/requests",
            brief(other_group, json!({"kind":"request","reference":"foreign-cohort"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let first_page = call(&app, Some(&jwt), "GET", "/api/v1/self-fix/outcomes", json!({})).await;
    assert_eq!(first_page.0, StatusCode::OK, "{:?}", first_page.1);
    let outcomes = &first_page.1["data"];
    assert_eq!(outcomes["summary"]["submitted"], 107);
    assert_eq!(outcomes["summary"]["staleReviews"], 1, "outcomes={outcomes}");
    assert_eq!(outcomes["summary"]["awaitingReview"], 106);
    assert_eq!(outcomes["summary"]["failed"], 35);
    assert_eq!(outcomes["summary"]["canceled"], 35);
    assert_eq!(outcomes["summary"]["completeEffortTasks"], 0);
    assert!(outcomes["summary"]["humanMinutes"].is_null());
    assert_eq!(outcomes["summary"]["acceptedInPeriod"], 1);
    assert_eq!(outcomes["summary"]["reopenedInPeriod"], 1);
    assert_eq!(outcomes["tasks"].as_array().unwrap().len(), 100);
    let params = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("from", outcomes["from"].as_str().unwrap())
        .append_pair("to", outcomes["to"].as_str().unwrap())
        .append_pair("cursor", outcomes["nextCursor"].as_str().unwrap())
        .finish();
    let last_page = call(&app, Some(&jwt), "GET", &format!("/api/v1/self-fix/outcomes?{params}"), json!({})).await;
    assert_eq!(last_page.0, StatusCode::OK, "{:?}", last_page.1);
    assert_eq!(last_page.1["data"]["summary"], outcomes["summary"]);
    assert_eq!(last_page.1["data"]["tasks"].as_array().unwrap().len(), 7);
    assert!(last_page.1["data"]["nextCursor"].is_null());
    let first_ids: std::collections::HashSet<&str> =
        outcomes["tasks"].as_array().unwrap().iter().map(|t| t["taskId"].as_str().unwrap()).collect();
    assert!(
        last_page.1["data"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| !first_ids.contains(t["taskId"].as_str().unwrap()))
    );
    let foreign = call(&app, Some(&other_jwt), "GET", "/api/v1/self-fix/outcomes", json!({})).await;
    assert_eq!(foreign.1["data"]["summary"]["submitted"], 1);
}
