//! #1314: a rejected verdict on the current maintenance revision blocks the
//! Forge merge. Real HTTP/auth/SQL with a local GitHub mock; no remote writes.
//! Its own test binary because `GITHUB_API_BASE` is process-global.

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

const PRS: [i32; 7] = [61, 62, 63, 64, 65, 66, 67];

async fn destination(pool: &PgPool, user: Uuid) -> (Uuid, Uuid) {
    let (org, team, project, group) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
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

async fn call(app: &Router, jwt: &str, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {jwt}"));
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)}));
    (status, value)
}

fn sha(c: char) -> String {
    c.to_string().repeat(40)
}

fn pr(number: i32, head: char) -> Value {
    json!({"number":number,"node_id":format!("PR-{number}"),"state":"open","merged":false,"draft":false,"head":{"sha":sha(head)},
        "base":{"ref":"develop","repo":{"full_name":"acme/widgets"}},"html_url":"https://example.com"})
}

struct Fixture {
    app: Router,
    pool: PgPool,
    org: Uuid,
    user: Uuid,
    group: Uuid,
    jwt: String,
}

impl Fixture {
    /// Maintenance task with a finished run, PR `number` at head `c…`, and one report.
    async fn task(&self, reference: &str, number: i32) -> (Uuid, Value, Value) {
        let brief = json!({"groupId":self.group,"title":"Repair dependency","brief":"Update and verify the dependency.",
            "source":{"kind":"request","reference":reference}});
        let (status, body) = call(&self.app, &self.jwt, "POST", "/api/v1/self-fix/requests", brief).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let task = Uuid::parse_str(body["data"]["taskId"].as_str().unwrap()).unwrap();
        let (agent, run) = (Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status,runtime_kind,cli_tool) VALUES ($1,$2,$2,$3,'idle','container','codex')").bind(agent).bind(self.org).bind(self.user).execute(&self.pool).await.unwrap();
        sqlx::query("INSERT INTO task_runs (id,organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,finished_at) VALUES ($1,$2,$2,$3,$4,$5,'completed',now())").bind(run).bind(self.org).bind(task).bind(agent).bind(run.to_string()).execute(&self.pool).await.unwrap();
        sqlx::query("UPDATE orchestration_tasks SET status='completed',base_commit_sha=$2,pr_head_sha=$3,pr_number=$4,review_status='in_review' WHERE id=$1").bind(task).bind(sha('b')).bind(sha('c')).bind(number).execute(&self.pool).await.unwrap();
        let data = self.delivery(task).await;
        let input = json!({"requestKey":Uuid::new_v4(),"expectedVersion":data["taskVersion"],"expectedRevision":data["currentRevision"],"runId":run,
            "criteria":"The dependency tests pass.","scope":"Dependency constraints only.","environmentNotes":"Same disposable fixture.","cliVersion":"fixture-1",
            "changeSummary":"Updated constraints.","checks":[],"unverified":["production deployment"]});
        let (status, report) =
            call(&self.app, &self.jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/reports"), input).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        (task, data, report["data"].clone())
    }

    async fn delivery(&self, task: Uuid) -> Value {
        call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/delivery"), json!({})).await.1["data"]
            .clone()
    }

    async fn verdict(&self, task: Uuid, data: &Value, report: &Value, verdict: &str) -> (StatusCode, Value) {
        let input = json!({"requestKey":Uuid::new_v4(),"reportId":report["id"],"expectedVersion":data["taskVersion"],
            "expectedRevision":data["currentRevision"],"verdict":verdict,"reason":"Reviewed the exact revision.","humanMinutes":{"review":3}});
        call(&self.app, &self.jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/decisions"), input).await
    }

    async fn approve(&self, jwt: &str, task: Uuid) -> (StatusCode, Value) {
        call(&self.app, jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/approve"), json!({})).await
    }

    async fn rejected_flag(&self, task: Uuid) -> Value {
        call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/review"), json!({})).await.1["data"]
            ["currentRevisionRejected"]
            .clone()
    }

    async fn merge_state(&self, task: Uuid) -> (Option<String>, i32) {
        sqlx::query_as("SELECT review_status, merge_attempts FROM orchestration_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rejected_current_revision_blocks_forge_merge(pool: PgPool) {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email,is_admin) VALUES ($1,'dev@example.com',true)")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let (org, group) = destination(&pool, user).await;
    let (other_org, _) = destination(&pool, user).await;
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
            then.status(200).json_body(json!({"object":{"sha":sha('b')}}));
        })
        .await;
    for head in ['c', 'd'] {
        server.mock_async(|when,then| {when.method(GET).path(format!("/repos/acme/widgets/commits/{}/check-runs",sha(head)));then.status(200).json_body(json!({"total_count":1,"check_runs":[{"name":"CI","head_sha":sha(head),"status":"completed","conclusion":"success"}]}));}).await;
        server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/repos/acme/widgets/commits/{}/statuses", sha(head)));
                then.status(200).json_body(json!([]));
            })
            .await;
    }
    let mut merges = std::collections::HashMap::new();
    for number in PRS {
        // PR 65 is the "rejected an older revision" case: its live head moved to `d…`.
        let head = if number == 65 { 'd' } else { 'c' };
        server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/repos/acme/widgets/pulls/{number}"));
                then.status(200).json_body(pr(number, head));
            })
            .await;
        let merge = server
            .mock_async(|when, then| {
                when.method(PUT).path(format!("/repos/acme/widgets/pulls/{number}/merge"));
                then.status(200).json_body(json!({"merged":true}));
            })
            .await;
        merges.insert(number, merge);
        server
            .mock_async(|when, then| {
                when.method(POST).path(format!("/repos/acme/widgets/issues/{number}/comments"));
                then.status(201).json_body(json!({"id":1}));
            })
            .await;
    }

    let mut state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let mut config = test_app_config("postgres://test");
    config.github_app_id = Some("12345".into());
    config.github_app_installation_id = Some("1".into());
    config.github_app_private_key = Some(include_str!("fixtures/test_rsa_private_key.pem").to_string().into());
    config.github_app_repo = Some("acme/widgets".into());
    state.config = Arc::new(config);
    let f = Fixture { app: create_router(state), pool: pool.clone(), org, user, group, jwt: jwt.clone() };

    // 1. No verdict: merge controls stay independent of acceptance.
    let (none, _, _) = f.task("gate-none", 61).await;
    assert_eq!(f.rejected_flag(none).await, false);
    let (status, body) = f.approve(&jwt, none).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(f.merge_state(none).await.0.as_deref(), Some("merged"));

    // 2. Accepted current revision merges.
    let (accepted, data, report) = f.task("gate-accepted", 62).await;
    assert_eq!(f.verdict(accepted, &data, &report, "accepted").await.0, StatusCode::OK);
    assert_eq!(f.approve(&jwt, accepted).await.0, StatusCode::OK);

    // 3. Rejected current revision: typed 409, nothing merged, attempt not counted,
    //    and another tenant cannot even see the task.
    let (rejected, data, report) = f.task("gate-rejected", 63).await;
    assert_eq!(f.verdict(rejected, &data, &report, "rejected").await.0, StatusCode::OK);
    assert_eq!(f.rejected_flag(rejected).await, true);
    let (status, body) = f.approve(&jwt, rejected).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "errors.maintenance.merge_rejected");
    assert!(body["error"]["message"].as_str().unwrap().contains("Record a new verdict"), "{body}");
    assert_eq!(f.approve(&other_jwt, rejected).await.0, StatusCode::NOT_FOUND);
    assert_eq!(merges[&63].calls_async().await, 0);
    assert_eq!(f.merge_state(rejected).await, (Some("in_review".into()), 0));

    // 4. A newer verdict on the same revision supersedes the rejection.
    let (superseded, data, report) = f.task("gate-superseded", 64).await;
    assert_eq!(f.verdict(superseded, &data, &report, "rejected").await.0, StatusCode::OK);
    assert_eq!(f.verdict(superseded, &data, &report, "accepted").await.0, StatusCode::OK);
    assert_eq!(f.rejected_flag(superseded).await, false);
    assert_eq!(f.approve(&jwt, superseded).await.0, StatusCode::OK);

    // 5. A rejection bound to an older revision does not block the new head.
    let (old, data, report) = f.task("gate-old-revision", 65).await;
    assert_eq!(f.verdict(old, &data, &report, "rejected").await.0, StatusCode::OK);
    sqlx::query("UPDATE orchestration_tasks SET pr_head_sha=$2 WHERE id=$1")
        .bind(old)
        .bind(sha('d'))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(f.rejected_flag(old).await, false);
    let (status, body) = f.approve(&jwt, old).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 6. Plain self-fix task without a maintenance request: unchanged.
    let plain = Uuid::new_v4();
    sqlx::query("INSERT INTO orchestration_tasks (id,organization_id,group_id,title,created_by,status,self_fix,pr_number,pr_head_sha,review_status) VALUES ($1,$2,$3,'Plain self-fix',$4,'completed',true,66,$5,'in_review')").bind(plain).bind(org).bind(group).bind(user).bind(sha('c')).execute(&pool).await.unwrap();
    assert_eq!(f.rejected_flag(plain).await, false);
    let (status, body) = f.approve(&jwt, plain).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 7. Concurrent reject + merge both queue on the task-row lock; exactly one wins.
    let (race, data, report) = f.task("gate-race", 67).await;
    let mut hold = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(race)
        .execute(&mut *hold)
        .await
        .unwrap();
    let release = async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        hold.commit().await.unwrap();
    };
    let ((reject, _), (merge, merge_body), ()) =
        tokio::join!(f.verdict(race, &data, &report, "rejected"), f.approve(&jwt, race), release);
    let rejections: i64 =
        sqlx::query_scalar("SELECT count(*) FROM maintenance_review_decisions WHERE task_id=$1 AND verdict='rejected'")
            .bind(race)
            .fetch_one(&pool)
            .await
            .unwrap();
    let merged = f.merge_state(race).await.0.as_deref() == Some("merged");
    match (reject, merge) {
        (StatusCode::OK, StatusCode::CONFLICT) => {
            assert_eq!(merge_body["error"]["code"], "errors.maintenance.merge_rejected");
            assert!(!merged && rejections == 1);
            assert_eq!(merges[&67].calls_async().await, 0);
        }
        (StatusCode::CONFLICT, StatusCode::OK) => assert!(merged && rejections == 0),
        other => panic!("reject and merge must serialize, got {other:?}"),
    }
}
