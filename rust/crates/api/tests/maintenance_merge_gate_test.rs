//! #1314: a rejected verdict on the task's current maintenance report blocks
//! the Forge merge, and no verdict can land while a started merge is
//! unconfirmed (`approved`). Real HTTP/auth/SQL against an in-process fake
//! GitHub whose merge call can be paused on a oneshot, so every interleaving
//! below is forced explicitly (no sleeps). Its own test binary because
//! `GITHUB_API_BASE` is process-global.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agentforge_api::create_router;
use agentforge_api::test_support::{app_state_with_mock_provider, mint_test_jwt, test_app_config};
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::oneshot;
use tower::ServiceExt;
use uuid::Uuid;

fn sha(c: char) -> String {
    c.to_string().repeat(40)
}

/// Paused merge: the fake signals `entered` when the merge call arrives and
/// waits for `release` before merging.
type Hold = (oneshot::Sender<()>, oneshot::Receiver<()>);

#[derive(Default)]
struct FakeGithub {
    heads: Mutex<HashMap<i32, String>>,
    merged: Mutex<HashSet<i32>>,
    red_heads: Mutex<HashSet<String>>,
    failing_comments: Mutex<HashSet<i32>>,
    /// Closed on GitHub without merging.
    closed: Mutex<HashSet<i32>>,
    /// The merge call merges but answers 405 (a refusal that raced a merge).
    refuse_after_merging: Mutex<HashSet<i32>>,
    holds: Mutex<HashMap<i32, Hold>>,
    merge_calls: Mutex<HashMap<i32, usize>>,
    requests: AtomicUsize,
}

impl FakeGithub {
    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
    fn merges(&self, pr: i32) -> usize {
        self.merge_calls.lock().unwrap().get(&pr).copied().unwrap_or(0)
    }
    fn set_head(&self, pr: i32, head: char) {
        self.heads.lock().unwrap().insert(pr, sha(head));
    }
    /// Pause the next merge of `pr`; returns (entered, release).
    fn hold(&self, pr: i32) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        self.holds.lock().unwrap().insert(pr, (entered_tx, release_rx));
        (entered_rx, release_tx)
    }
}

type Fake = State<Arc<FakeGithub>>;

async fn token(State(f): Fake) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let expires = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    (
        StatusCode::CREATED,
        axum::Json(json!({"token":"test-installation-token","expires_at":expires,
        "permissions":{"contents":"write","pull_requests":"write","checks":"read"}})),
    )
        .into_response()
}
async fn repository(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"default_branch":"develop","archived":false,"disabled":false,"allow_squash_merge":true}))
}
async fn default_ref(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"object":{"sha":sha('b')}}))
}
async fn pull(State(f): Fake, Path(n): Path<i32>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let Some(head) = f.heads.lock().unwrap().get(&n).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let merged = f.merged.lock().unwrap().contains(&n);
    let closed = merged || f.closed.lock().unwrap().contains(&n);
    axum::Json(json!({"number":n,"node_id":format!("PR-{n}"),"state":if closed {"closed"} else {"open"},"merged":merged,"draft":false,
        "head":{"sha":head},"base":{"ref":"develop","repo":{"full_name":"acme/widgets"}},"html_url":"https://example.com"}))
    .into_response()
}
async fn check_runs(State(f): Fake, Path(head): Path<String>) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let conclusion = if f.red_heads.lock().unwrap().contains(&head) { "failure" } else { "success" };
    axum::Json(
        json!({"total_count":1,"check_runs":[{"name":"CI","head_sha":head,"status":"completed","conclusion":conclusion}]}),
    )
}
async fn statuses(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!([]))
}
async fn merge(State(f): Fake, Path(n): Path<i32>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let hold = f.holds.lock().unwrap().remove(&n);
    if let Some((entered, release)) = hold {
        let _ = entered.send(());
        let _ = release.await;
    }
    *f.merge_calls.lock().unwrap().entry(n).or_default() += 1;
    f.merged.lock().unwrap().insert(n);
    if f.refuse_after_merging.lock().unwrap().contains(&n) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    axum::Json(json!({"merged":true})).into_response()
}
async fn comment(State(f): Fake, Path(n): Path<i32>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    if f.failing_comments.lock().unwrap().contains(&n) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (StatusCode::CREATED, axum::Json(json!({"id":1}))).into_response()
}

async fn start_fake() -> (Arc<FakeGithub>, String) {
    let fake = Arc::new(FakeGithub::default());
    let app = Router::new()
        .route("/app/installations/{id}/access_tokens", post(token))
        .route("/repos/acme/widgets", get(repository))
        .route("/repos/acme/widgets/git/ref/heads/develop", get(default_ref))
        .route("/repos/acme/widgets/pulls/{n}", get(pull))
        .route("/repos/acme/widgets/pulls/{n}/merge", put(merge))
        .route("/repos/acme/widgets/commits/{sha}/check-runs", get(check_runs))
        .route("/repos/acme/widgets/commits/{sha}/statuses", get(statuses))
        .route("/repos/acme/widgets/issues/{n}/comments", post(comment))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, base)
}

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

struct Fixture {
    app: Router,
    pool: PgPool,
    fake: Arc<FakeGithub>,
    org: Uuid,
    user: Uuid,
    group: Uuid,
    jwt: String,
}

impl Fixture {
    /// Maintenance task with a finished run and PR `number` at `head`.
    async fn task(&self, reference: &str, number: i32, head: char) -> Uuid {
        let brief = json!({"groupId":self.group,"title":"Repair dependency","brief":"Update and verify the dependency.",
            "source":{"kind":"request","reference":reference}});
        let (status, body) = call(&self.app, &self.jwt, "POST", "/api/v1/self-fix/requests", brief).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let task = Uuid::parse_str(body["data"]["taskId"].as_str().unwrap()).unwrap();
        self.add_run(task, 0).await;
        sqlx::query("UPDATE orchestration_tasks SET status='completed',base_commit_sha=$2,pr_head_sha=$3,pr_number=$4,review_status='in_review' WHERE id=$1").bind(task).bind(sha('b')).bind(sha(head)).bind(number).execute(&self.pool).await.unwrap();
        self.fake.set_head(number, head);
        task
    }

    /// A finished run `offset_secs` after now; the latest run defines the binding.
    async fn add_run(&self, task: Uuid, offset_secs: i32) {
        let (agent, run) = (Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status,runtime_kind,cli_tool) VALUES ($1,$2,$2,$3,'idle','container','codex')").bind(agent).bind(self.org).bind(self.user).execute(&self.pool).await.unwrap();
        sqlx::query("INSERT INTO task_runs (id,organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,started_at,finished_at) VALUES ($1,$2,$2,$3,$4,$5,'completed',now()+make_interval(secs=>$6),now()+make_interval(secs=>$6))").bind(run).bind(self.org).bind(task).bind(agent).bind(run.to_string()).bind(offset_secs as f64).execute(&self.pool).await.unwrap();
    }

    /// A refreshed client: reads the current delivery state, then writes.
    async fn delivery(&self, task: Uuid) -> Value {
        let (status, body) =
            call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/delivery"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }

    async fn report(&self, task: Uuid) -> Uuid {
        let data = self.delivery(task).await;
        let input = json!({"requestKey":Uuid::new_v4(),"expectedVersion":data["taskVersion"],"expectedRevision":data["currentRevision"],
            "runId":data["latestRunId"],"criteria":"The dependency tests pass.","scope":"Dependency constraints only.",
            "environmentNotes":"Same disposable fixture.","cliVersion":"fixture-1","changeSummary":"Updated constraints.",
            "checks":[],"unverified":["production deployment"]});
        let (status, body) =
            call(&self.app, &self.jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/reports"), input).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        Uuid::parse_str(body["data"]["id"].as_str().unwrap()).unwrap()
    }

    async fn verdict(&self, task: Uuid, report: Uuid, verdict: &str) -> (StatusCode, Value) {
        let data = self.delivery(task).await;
        let input = json!({"requestKey":Uuid::new_v4(),"reportId":report,"expectedVersion":data["taskVersion"],
            "expectedRevision":data["currentRevision"],"verdict":verdict,"reason":"Reviewed the exact revision.","humanMinutes":{"review":3}});
        call(&self.app, &self.jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/decisions"), input).await
    }

    async fn approve(&self, task: Uuid) -> (StatusCode, Value) {
        approve(self.app.clone(), self.jwt.clone(), task).await
    }

    async fn rejected_flag(&self, task: Uuid) -> Value {
        call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/review"), json!({})).await.1["data"]
            ["currentRevisionRejected"]
            .clone()
    }

    /// Start an approve, wait until it is paused at the provider merge call
    /// holding this task's merge mutex, cancel it (as when the client goes
    /// away), and wait until the mutex is released. The provider never answers
    /// that merge call; the returned sender keeps it pending.
    async fn cancel_while_merging(&self, task: Uuid, pr: i32) -> oneshot::Sender<()> {
        let (entered, release) = self.fake.hold(pr);
        let attempt = tokio::spawn(approve(self.app.clone(), self.jwt.clone(), task));
        entered.await.unwrap();
        wait_for(&self.pool, &merge_lock_held(self.org, task)).await;
        attempt.abort();
        assert!(attempt.await.unwrap_err().is_cancelled());
        wait_for(&self.pool, &format!("SELECT NOT ({})", merge_lock_held(self.org, task))).await;
        release
    }

    /// (review_status, merge_attempts)
    async fn state(&self, task: Uuid) -> (String, i32) {
        sqlx::query_as("SELECT review_status, merge_attempts FROM orchestration_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

async fn approve(app: Router, jwt: String, task: Uuid) -> (StatusCode, Value) {
    call(&app, &jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/approve"), json!({})).await
}

fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or_default()
}

const REJECTED: &str = "errors.maintenance.merge_rejected";
const IN_PROGRESS: &str = "errors.maintenance.merge_in_progress";
const RUNNING: &str = "errors.self_fix.merge_running";
const UNCONFIRMED: &str = "errors.self_fix.merge_unconfirmed";
const EXHAUSTED_UNCONFIRMED: &str = "errors.self_fix.merge_attempts_exhausted_unconfirmed";

#[sqlx::test(migrations = "../db/migrations")]
async fn rejected_current_report_blocks_forge_merge_and_merges_serialize_with_verdicts(pool: PgPool) {
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

    let (fake, base) = start_fake().await;
    unsafe { std::env::set_var("GITHUB_API_BASE", &base) };
    let f =
        Fixture { app: forge_app(pool.clone()).await, pool: pool.clone(), fake: fake.clone(), org, user, group, jwt };

    // No verdict: merge controls stay independent of acceptance.
    let task = f.task("gate-none", 61, 'c').await;
    f.report(task).await;
    assert_eq!(f.rejected_flag(task).await, false);
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(f.state(task).await, ("merged".into(), 1));

    // Accepted current report merges.
    let task = f.task("gate-accepted", 62, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "accepted").await.0, StatusCode::OK);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // Order 1 — the rejection commits first: typed 409, no provider merge call,
    // attempt not counted. Another tenant gets 404 without any provider read.
    let task = f.task("gate-rejected", 63, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    assert_eq!(f.rejected_flag(task).await, true);
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, REJECTED), "{body}");
    assert!(body["error"]["message"].as_str().unwrap().contains("Record a new verdict on a newer report"));
    assert!(!body.to_string().contains("reconcile"), "{body}");
    assert_eq!(f.fake.merges(63), 0);
    assert_eq!(f.state(task).await, ("in_review".into(), 0));
    let reads = fake.requests();
    assert_eq!(approve(f.app.clone(), other_jwt.clone(), task).await.0, StatusCode::NOT_FOUND);
    let foreign_review =
        call(&f.app, &other_jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/review"), json!({})).await;
    assert_eq!(foreign_review.0, StatusCode::NOT_FOUND);
    assert_eq!(fake.requests(), reads, "a cross-tenant request must not reach the provider");

    // A newer verdict on the same report supersedes the rejection.
    let task = f.task("gate-superseded", 64, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    assert_eq!(f.verdict(task, report, "accepted").await.0, StatusCode::OK);
    assert_eq!(f.rejected_flag(task).await, false);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // R1 rejected, then R2 for the same head with no verdict: not blocked.
    let task = f.task("gate-newer-report", 65, 'c').await;
    let first = f.report(task).await;
    assert_eq!(f.verdict(task, first, "rejected").await.0, StatusCode::OK);
    f.report(task).await;
    assert_eq!(f.rejected_flag(task).await, false);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // A rejected report from an earlier run is not current once the run changed.
    let task = f.task("gate-earlier-run", 66, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    f.add_run(task, 5).await;
    assert_eq!(f.rejected_flag(task).await, false);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // A rejected report whose title/brief binding no longer holds does not block.
    let task = f.task("gate-changed-brief", 67, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    sqlx::query("UPDATE orchestration_tasks SET title='Repair dependency, revised scope' WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(f.rejected_flag(task).await, false);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // A rejection of an older revision does not block the recorded head.
    let task = f.task("gate-old-revision", 68, 'c').await;
    let report = f.report(task).await;
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    sqlx::query("UPDATE orchestration_tasks SET pr_head_sha=$2 WHERE id=$1")
        .bind(task)
        .bind(sha('d'))
        .execute(&pool)
        .await
        .unwrap();
    f.fake.set_head(68, 'd');
    assert_eq!(f.rejected_flag(task).await, false);
    assert_eq!(f.approve(task).await.0, StatusCode::OK);

    // Plain self-fix task without a maintenance request: unchanged.
    let plain = Uuid::new_v4();
    sqlx::query("INSERT INTO orchestration_tasks (id,organization_id,group_id,title,created_by,status,self_fix,pr_number,pr_head_sha,review_status) VALUES ($1,$2,$3,'Plain self-fix',$4,'completed',true,69,$5,'in_review')").bind(plain).bind(org).bind(group).bind(user).bind(sha('c')).execute(&pool).await.unwrap();
    f.fake.set_head(69, 'c');
    assert_eq!(f.rejected_flag(plain).await, false);
    let (status, body) = f.approve(plain).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Crash between the merge decision and the provider call leaves `approved`:
    // verdicts wait, and the next approve proceeds normally.
    let task = f.task("gate-crash", 70, 'c').await;
    let report = f.report(task).await;
    sqlx::query("UPDATE orchestration_tasks SET review_status='approved',merge_attempts=1 WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    // Inside the settle window the open pull request is not retried yet.
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, UNCONFIRMED), "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(70)), (("approved".into(), 1), 0));
    backdate(&pool, task).await;
    assert_eq!(f.approve(task).await.0, StatusCode::OK);
    assert_eq!((f.state(task).await, f.fake.merges(70)), (("merged".into(), 2), 1));

    // Order 2 — the merge decision commits first and the provider is paused: a
    // refreshed client's rejection is refused; after release the task merges
    // once. A verdict after the merge is accepted (merge and acceptance are
    // independent records).
    let task = f.task("gate-paused", 71, 'c').await;
    let report = f.report(task).await;
    let (entered, release) = f.fake.hold(71);
    let merging = tokio::spawn(approve(f.app.clone(), f.jwt.clone(), task));
    entered.await.unwrap();
    assert_eq!(f.state(task).await, ("approved".into(), 1));
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    release.send(()).unwrap();
    let (status, body) = merging.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(71)), (("merged".into(), 1), 1));
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);

    // Order 3 — the merge decision commits first and the provider refuses before
    // merging (red CI): back to review, a rejection can be recorded, and the
    // next approve is refused by the rejection gate.
    let task = f.task("gate-refused", 72, 'e').await;
    f.fake.red_heads.lock().unwrap().insert(sha('e'));
    let report = f.report(task).await;
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::BAD_REQUEST, "errors.self_fix.checks_not_green"), "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(72)), (("in_review".into(), 1), 0));
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, REJECTED), "{body}");
    assert_eq!(f.state(task).await, ("in_review".into(), 1));

    // Order 4 — the provider merges but the audit comment fails (ambiguous): the
    // status stays `approved`, verdicts wait, and a second approve finds the
    // pull request merged and ends `merged` without a second merge call.
    let task = f.task("gate-ambiguous", 73, 'c').await;
    let report = f.report(task).await;
    f.fake.failing_comments.lock().unwrap().insert(73);
    let (status, body) = f.approve(task).await;
    assert!(status.is_server_error(), "{status} {body}");
    assert_eq!((f.state(task).await, f.fake.merges(73)), (("approved".into(), 1), 1));
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    f.fake.failing_comments.lock().unwrap().clear();
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["alreadyMerged"], true, "{body}");
    // Reconciliation counts no attempt.
    assert_eq!((f.state(task).await, f.fake.merges(73)), (("merged".into(), 1), 1));

    // T1 — contention BEFORE the decision commits. The test holds the task row
    // lock (the `SELECT … FOR UPDATE` that `lock_task` takes), so approve must
    // wait inside its decision transaction. A rejection committed by the lock
    // holder is then seen. Fails if the decision stops taking `lock_task`: the
    // wait below never appears, or approve merges.
    let task = f.task("gate-row-lock", 74, 'c').await;
    let report = f.report(task).await;
    let mut hold = pool.begin().await.unwrap();
    let hold_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *hold).await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *hold)
        .await
        .unwrap();
    let merging = tokio::spawn(approve(f.app.clone(), f.jwt.clone(), task));
    // Exactly: some session is blocked by THIS test transaction.
    wait_for(
        &pool,
        &format!("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE {hold_pid} = ANY(pg_blocking_pids(pid)))"),
    )
    .await;
    assert!(!merging.is_finished());
    assert_eq!(f.fake.merges(74), 0);
    sqlx::query("INSERT INTO maintenance_review_decisions (id,organization_id,task_id,report_id,request_key,reviewer_id,verdict,reason,input) VALUES ($1,$2,$3,$4,$5,$6,'rejected','Rejected while the merge waited.',$7)")
        .bind(Uuid::now_v7()).bind(org).bind(task).bind(report).bind(Uuid::new_v4()).bind(user)
        .bind(json!({"requestKey":Uuid::new_v4(),"reportId":report,"expectedVersion":0,"expectedRevision":sha('c'),"verdict":"rejected","reason":"Rejected while the merge waited.","humanMinutes":{}}))
        .execute(&mut *hold).await.unwrap();
    hold.commit().await.unwrap();
    let (status, body) = merging.await.unwrap();
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, REJECTED), "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(74)), (("in_review".into(), 0), 0));

    // T2 — overlapping approvals. A is paused at the provider merge endpoint;
    // B is refused at once by the merge mutex, runs no executor (no provider
    // request at all) and counts no attempt. Fails if the mutex is removed: B
    // would enter the decision, bump the attempt and call the provider.
    let task = f.task("gate-overlap", 75, 'c').await;
    f.report(task).await;
    let (entered, release) = f.fake.hold(75);
    let first = tokio::spawn(approve(f.app.clone(), f.jwt.clone(), task));
    entered.await.unwrap();
    let requests = f.fake.requests();
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{body}");
    assert_eq!(f.fake.requests(), requests, "a refused second approve must not reach the provider");
    assert_eq!(f.state(task).await, ("approved".into(), 1));
    release.send(()).unwrap();
    assert_eq!(first.await.unwrap().0, StatusCode::OK);
    assert_eq!((f.state(task).await, f.fake.merges(75)), (("merged".into(), 1), 1));

    // T3 — the final allowed attempt is in flight. A second approve is refused
    // by the mutex and does NOT hit the cap (no `changes_requested`), so
    // verdicts stay refused until A's merge resolves.
    let task = f.task("gate-final-attempt", 76, 'c').await;
    let report = f.report(task).await;
    set_attempts(&pool, task, CAP - 1).await;
    let (entered, release) = f.fake.hold(76);
    let first = tokio::spawn(approve(f.app.clone(), f.jwt.clone(), task));
    entered.await.unwrap();
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{body}");
    assert_eq!(f.state(task).await, ("approved".into(), CAP));
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    release.send(()).unwrap();
    assert_eq!(first.await.unwrap().0, StatusCode::OK);
    assert_eq!((f.state(task).await, f.fake.merges(76)), (("merged".into(), CAP), 1));

    // T4 — ambiguous final attempt, then reconcile at the cap. The provider
    // merges but the audit comment fails; the next approve reconciles BEFORE
    // the cap check, finds the pull request merged and ends `merged`.
    let task = f.task("gate-final-ambiguous", 77, 'c').await;
    f.report(task).await;
    set_attempts(&pool, task, CAP - 1).await;
    f.fake.failing_comments.lock().unwrap().insert(77);
    assert!(f.approve(task).await.0.is_server_error());
    assert_eq!(f.state(task).await, ("approved".into(), CAP));
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(77)), (("merged".into(), CAP), 1));

    // T5 — rule 1c: the FINAL attempt is cancelled while the provider is still
    // processing its merge request. Retries inside the settle window are
    // refused as unconfirmed; after it, with the pull request still open and no
    // attempts left, the task stays `approved` (verdicts stay refused) instead
    // of `changes_requested`; once the provider reports the merge, the task ends
    // `merged`.
    let task = f.task("gate-cancelled-at-cap", 78, 'c').await;
    let report = f.report(task).await;
    set_attempts(&pool, task, CAP - 1).await;
    let _still_processing = f.cancel_while_merging(task, 78).await;
    assert_eq!(f.state(task).await, ("approved".into(), CAP));
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, UNCONFIRMED), "{body}");
    assert_eq!(f.state(task).await, ("approved".into(), CAP));
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    backdate(&pool, task).await;
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::BAD_REQUEST, EXHAUSTED_UNCONFIRMED), "{body}");
    assert_eq!(f.state(task).await, ("approved".into(), CAP));
    let (status, body) = f.verdict(task, report, "rejected").await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, IN_PROGRESS), "{body}");
    f.fake.merged.lock().unwrap().insert(78);
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(f.state(task).await, ("merged".into(), CAP));

    // T5b — same, but the provider reports the pull request closed without
    // merging: definite, so `changes_requested`, and a verdict can be recorded.
    let task = f.task("gate-cancelled-closed", 80, 'c').await;
    let report = f.report(task).await;
    set_attempts(&pool, task, CAP - 1).await;
    let _still_processing = f.cancel_while_merging(task, 80).await;
    f.fake.closed.lock().unwrap().insert(80);
    assert_eq!(f.approve(task).await.0, StatusCode::CONFLICT);
    assert_eq!(f.state(task).await, ("changes_requested".into(), CAP));
    assert_eq!(f.verdict(task, report, "rejected").await.0, StatusCode::OK);

    // T6 — an approve cancelled while paused at the provider (client gone)
    // leaves `approved` and does not leave the merge mutex held: its connection
    // is closed instead of pooled. The next approve takes the mutex (it gets the
    // settle-window answer, not the running-attempt 409) and, after the window,
    // finishes.
    let task = f.task("gate-cancelled", 79, 'c').await;
    f.report(task).await;
    let _still_processing = f.cancel_while_merging(task, 79).await;
    assert_eq!(f.state(task).await, ("approved".into(), 1));
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, UNCONFIRMED), "{body}");
    backdate(&pool, task).await;
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(f.state(task).await.0, "merged");

    // Rule 1d — the provider refuses the merge call (405) although it merged:
    // the fresh pull-request read wins, so the task ends `merged`, not `in_review`.
    let task = f.task("gate-refused-but-merged", 81, 'c').await;
    f.report(task).await;
    f.fake.refuse_after_merging.lock().unwrap().insert(81);
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["alreadyMerged"], true, "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(81)), (("merged".into(), 1), 1));

    // Atomicity — a DEFERRED constraint trigger (test-only) logs the committed
    // `(merge_attempts, review_status)` of every transaction that touches this
    // task. The decision commit must be exactly `(n+1, approved)`: no committed
    // state may count the attempt while the status is still `in_review`. Fails
    // if `approved` is written outside the decision transaction.
    let task = f.task("gate-atomic", 82, 'c').await;
    f.report(task).await;
    for statement in [
        "CREATE TABLE merge_commit_log (seq bigserial PRIMARY KEY, attempts int NOT NULL, status text)".to_string(),
        "CREATE FUNCTION log_merge_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
         INSERT INTO merge_commit_log (attempts, status) SELECT merge_attempts, review_status FROM orchestration_tasks WHERE id = NEW.id; \
         RETURN NULL; END $$"
            .to_string(),
        format!(
            "CREATE CONSTRAINT TRIGGER log_merge_commit AFTER UPDATE ON orchestration_tasks DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN (NEW.id = '{task}') EXECUTE FUNCTION log_merge_commit()"
        ),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let committed: Vec<(i32, Option<String>)> =
        sqlx::query_as("SELECT attempts, status FROM merge_commit_log ORDER BY seq").fetch_all(&pool).await.unwrap();
    for statement in [
        "DROP TRIGGER log_merge_commit ON orchestration_tasks",
        "DROP FUNCTION log_merge_commit()",
        "DROP TABLE merge_commit_log",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    assert_eq!(committed.first().map(|(a, s)| (*a, s.as_deref())), Some((1, Some("approved"))), "{committed:?}");
    assert!(
        committed.iter().all(|(a, s)| *a == 0 || matches!(s.as_deref(), Some("approved" | "merged"))),
        "no committed state may count the attempt outside `approved`: {committed:?}"
    );

    // Pool starvation — with a pool of ONE connection, an approve completes end
    // to end: the mutex, the decision transaction and every status write share
    // the mutex's connection.
    let task = f.task("gate-one-connection", 83, 'c').await;
    f.report(task).await;
    let single = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let (status, body) = approve(forge_app(single).await, f.jwt.clone(), task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(83)), (("merged".into(), 1), 1));

    // Decision budget (5 s): the decision transaction's `approved` timestamp is
    // its START time, so the whole transaction is bounded. Each case below holds
    // a row the decision needs for longer than the budget (the real 5 s; there
    // is no test seam): approve returns the busy 409 with status, attempts and
    // merge calls unchanged, and succeeds once the row is free.
    for (case, pr, row) in [
        ("task row", 84, "SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE"),
        (
            "user row (authority check)",
            85,
            "SELECT 1 FROM users WHERE id=(SELECT created_by FROM orchestration_tasks WHERE id=$1) FOR UPDATE",
        ),
        ("latest-run row", 86, "SELECT 1 FROM task_runs WHERE orchestration_task_id=$1 FOR UPDATE"),
    ] {
        let task = f.task(&format!("gate-budget-{pr}"), pr, 'c').await;
        f.report(task).await;
        let mut hold = pool.begin().await.unwrap();
        sqlx::query(row).bind(task).execute(&mut *hold).await.unwrap();
        // Bounded so that a lock wait outside the decision fails instead of hanging.
        let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(60), f.approve(task))
            .await
            .unwrap_or_else(|_| panic!("{case}: approve did not finish"));
        assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{case}: {body}");
        assert_eq!((f.state(task).await, f.fake.merges(pr)), (("in_review".into(), 0), 0), "{case}");
        hold.rollback().await.unwrap();
        let (status, body) = f.approve(task).await;
        assert_eq!(status, StatusCode::OK, "{case}: {body}");
        assert_eq!((f.state(task).await, f.fake.merges(pr)), (("merged".into(), 1), 1), "{case}");
    }

    // Age check — every single lock wait succeeds inside the lock timeout, but
    // the transaction as a whole outlives the budget: the task row is released
    // after ~3 s, then the latest-run row after ~3 s more. The final age check
    // refuses the commit; nothing is written. (No production seam exists to age
    // the transaction directly, so this uses held locks released in time.)
    let task = f.task("gate-budget-age", 87, 'c').await;
    f.report(task).await;
    let mut task_hold = pool.begin().await.unwrap();
    let task_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *task_hold).await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *task_hold)
        .await
        .unwrap();
    let mut run_hold = pool.begin().await.unwrap();
    let run_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *run_hold).await.unwrap();
    sqlx::query("SELECT 1 FROM task_runs WHERE orchestration_task_id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *run_hold)
        .await
        .unwrap();
    let blocked_by =
        |pid: i32| format!("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE {pid} = ANY(pg_blocking_pids(pid)))");
    let deciding = tokio::spawn(approve(f.app.clone(), f.jwt.clone(), task));
    wait_for(&pool, &blocked_by(task_pid)).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    task_hold.rollback().await.unwrap();
    wait_for(&pool, &blocked_by(run_pid)).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    run_hold.rollback().await.unwrap();
    let (status, body) = deciding.await.unwrap();
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{body}");
    assert_eq!((f.state(task).await, f.fake.merges(87)), (("in_review".into(), 0), 0));
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

const CAP: i32 = 5; // test_app_config: self_fix_max_merge_attempts

async fn forge_app(pool: PgPool) -> Router {
    let mut state = app_state_with_mock_provider(pool, "mock", "ok").await;
    let mut config = test_app_config("postgres://test");
    config.github_app_id = Some("12345".into());
    config.github_app_installation_id = Some("1".into());
    config.github_app_private_key = Some(include_str!("fixtures/test_rsa_private_key.pem").to_string().into());
    config.github_app_repo = Some("acme/widgets".into());
    state.config = Arc::new(config);
    create_router(state)
}

/// Whether THIS task's merge mutex is held: the exact advisory key
/// (`hashtextextended('self-fix-merge:<org>:<task>', 0)`, a single bigint key
/// split into classid/objid), granted, in this database.
fn merge_lock_held(org: Uuid, task: Uuid) -> String {
    let key = format!("hashtextextended('self-fix-merge:{org}:{task}', 0)");
    format!(
        "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND granted AND objsubid=1 \
         AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) \
         AND classid::bigint=(({key}>>32)&4294967295) AND objid::bigint=({key}&4294967295))"
    )
}

/// Move the task row's last write 31 s into the past (past the settle window)
/// without waiting: the `updated_at` trigger is paused for this one update.
async fn backdate(pool: &PgPool, task: Uuid) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE orchestration_tasks DISABLE TRIGGER orchestration_tasks_updated_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE orchestration_tasks SET updated_at = now() - interval '31 seconds' WHERE id=$1")
        .bind(task)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE orchestration_tasks ENABLE TRIGGER orchestration_tasks_updated_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn set_attempts(pool: &PgPool, task: Uuid, attempts: i32) {
    sqlx::query("UPDATE orchestration_tasks SET merge_attempts=$2 WHERE id=$1")
        .bind(task)
        .bind(attempts)
        .execute(pool)
        .await
        .unwrap();
}

/// Wait for an observable database condition (bounded; a timeout fails the
/// test). Polling only observes state; it never decides an outcome.
async fn wait_for(pool: &PgPool, condition: &str) {
    for _ in 0..1000 {
        if sqlx::query_scalar::<_, bool>(condition).fetch_one(pool).await.unwrap() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("condition not reached: {condition}");
}
