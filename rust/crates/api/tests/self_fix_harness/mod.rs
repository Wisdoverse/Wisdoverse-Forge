//! Shared harness for the self-fix merge-gate and head-reconcile tests: an
//! in-process fake GitHub whose calls can be paused, plus fixtures.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agentforge_api::create_router;
use agentforge_api::test_support::{app_state_with_mock_provider, test_app_config};
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

pub fn sha(c: char) -> String {
    c.to_string().repeat(40)
}

/// Paused merge: the fake signals `entered` when the merge call arrives and
/// waits for `release` before merging.
pub type Hold = (oneshot::Sender<()>, oneshot::Receiver<()>);

#[derive(Default)]
pub struct FakeGithub {
    pub heads: Mutex<HashMap<i32, String>>,
    pub merged: Mutex<HashSet<i32>>,
    pub red_heads: Mutex<HashSet<String>>,
    pub failing_comments: Mutex<HashSet<i32>>,
    /// Closed on GitHub without merging.
    pub closed: Mutex<HashSet<i32>>,
    /// The merge call merges but answers 405 (a refusal that raced a merge).
    pub refuse_after_merging: Mutex<HashSet<i32>>,
    pub holds: Mutex<HashMap<i32, Hold>>,
    pub merge_calls: Mutex<HashMap<i32, usize>>,
    pub requests: AtomicUsize,
    /// Base branch per pull request (default `develop`).
    pub bases: Mutex<HashMap<i32, String>>,
    /// The pull-request read answers 500.
    pub failing_pulls: Mutex<HashSet<i32>>,
    /// Changed files per pull request (default `src/lib.rs`).
    pub files: Mutex<HashMap<i32, Vec<String>>>,
    /// The file listing never ends (a `next` link on every page).
    pub endless_files: Mutex<HashSet<i32>>,
    /// The file listing answers 500.
    pub failing_files: Mutex<HashSet<i32>>,
    /// Paused file listings, as `holds` for merges.
    pub file_holds: Mutex<HashMap<i32, Hold>>,
    /// The fake's own origin, for pagination links.
    pub base_url: Mutex<String>,
}

impl FakeGithub {
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
    pub fn merges(&self, pr: i32) -> usize {
        self.merge_calls.lock().unwrap().get(&pr).copied().unwrap_or(0)
    }
    pub fn set_head(&self, pr: i32, head: char) {
        self.heads.lock().unwrap().insert(pr, sha(head));
    }
    /// Pause the next merge of `pr`; returns (entered, release).
    pub fn hold(&self, pr: i32) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        self.holds.lock().unwrap().insert(pr, (entered_tx, release_rx));
        (entered_rx, release_tx)
    }
    /// Pause the next file listing of `pr`; returns (entered, release).
    pub fn hold_files(&self, pr: i32) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        self.file_holds.lock().unwrap().insert(pr, (entered_tx, release_rx));
        (entered_rx, release_tx)
    }
}

pub type Fake = State<Arc<FakeGithub>>;

pub async fn token(State(f): Fake) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let expires = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    (
        StatusCode::CREATED,
        axum::Json(json!({"token":"test-installation-token","expires_at":expires,
        "permissions":{"contents":"write","pull_requests":"write","checks":"read"}})),
    )
        .into_response()
}
pub async fn repository(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"default_branch":"develop","archived":false,"disabled":false,"allow_squash_merge":true}))
}
pub async fn default_ref(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({"object":{"sha":sha('b')}}))
}
pub async fn pull(State(f): Fake, Path(n): Path<i32>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let Some(head) = f.heads.lock().unwrap().get(&n).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if f.failing_pulls.lock().unwrap().contains(&n) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let merged = f.merged.lock().unwrap().contains(&n);
    let closed = merged || f.closed.lock().unwrap().contains(&n);
    let base = f.bases.lock().unwrap().get(&n).cloned().unwrap_or_else(|| "develop".into());
    axum::Json(json!({"number":n,"node_id":format!("PR-{n}"),"state":if closed {"closed"} else {"open"},"merged":merged,"draft":false,
        "head":{"sha":head},"base":{"ref":base,"repo":{"full_name":"acme/widgets"}},"html_url":"https://example.com"}))
    .into_response()
}
pub async fn pull_files(
    State(f): Fake,
    Path(n): Path<i32>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let hold = f.file_holds.lock().unwrap().remove(&n);
    if let Some((entered, release)) = hold {
        let _ = entered.send(());
        let _ = release.await;
    }
    if f.failing_files.lock().unwrap().contains(&n) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let page: usize = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    let files = f.files.lock().unwrap().get(&n).cloned().unwrap_or_else(|| vec!["src/lib.rs".into()]);
    let body: Vec<Value> = if page == 1 {
        files.iter().map(|name| json!({"filename": name})).collect()
    } else {
        vec![json!({"filename": format!("src/generated_{page}.rs")})]
    };
    let mut response = axum::Json(body).into_response();
    if f.endless_files.lock().unwrap().contains(&n) {
        let base = f.base_url.lock().unwrap().clone();
        let next = format!("<{base}/repos/acme/widgets/pulls/{n}/files?per_page=100&page={}>; rel=\"next\"", page + 1);
        response.headers_mut().insert(axum::http::header::LINK, next.parse().unwrap());
    }
    response
}
pub async fn check_runs(State(f): Fake, Path(head): Path<String>) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let conclusion = if f.red_heads.lock().unwrap().contains(&head) { "failure" } else { "success" };
    axum::Json(
        json!({"total_count":1,"check_runs":[{"name":"CI","head_sha":head,"status":"completed","conclusion":conclusion}]}),
    )
}
pub async fn statuses(State(f): Fake) -> axum::Json<Value> {
    f.requests.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!([]))
}
pub async fn merge(State(f): Fake, Path(n): Path<i32>) -> Response {
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
pub async fn comment(State(f): Fake, Path(n): Path<i32>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    if f.failing_comments.lock().unwrap().contains(&n) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (StatusCode::CREATED, axum::Json(json!({"id":1}))).into_response()
}

pub async fn start_fake() -> (Arc<FakeGithub>, String) {
    let fake = Arc::new(FakeGithub::default());
    let app = Router::new()
        .route("/app/installations/{id}/access_tokens", post(token))
        .route("/repos/acme/widgets", get(repository))
        .route("/repos/acme/widgets/git/ref/heads/develop", get(default_ref))
        .route("/repos/acme/widgets/pulls/{n}", get(pull))
        .route("/repos/acme/widgets/pulls/{n}/files", get(pull_files))
        .route("/repos/acme/widgets/pulls/{n}/merge", put(merge))
        .route("/repos/acme/widgets/commits/{sha}/check-runs", get(check_runs))
        .route("/repos/acme/widgets/commits/{sha}/statuses", get(statuses))
        .route("/repos/acme/widgets/issues/{n}/comments", post(comment))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    *fake.base_url.lock().unwrap() = base.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, base)
}

pub async fn destination(pool: &PgPool, user: Uuid) -> (Uuid, Uuid) {
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

pub async fn call(app: &Router, jwt: &str, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
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

pub struct Fixture {
    pub app: Router,
    pub pool: PgPool,
    pub fake: Arc<FakeGithub>,
    pub org: Uuid,
    pub user: Uuid,
    pub group: Uuid,
    pub jwt: String,
}

impl Fixture {
    /// Maintenance task with a finished run and PR `number` at `head`.
    pub async fn task(&self, reference: &str, number: i32, head: char) -> Uuid {
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
    pub async fn add_run(&self, task: Uuid, offset_secs: i32) {
        let (agent, run) = (Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status,runtime_kind,cli_tool) VALUES ($1,$2,$2,$3,'idle','container','codex')").bind(agent).bind(self.org).bind(self.user).execute(&self.pool).await.unwrap();
        sqlx::query("INSERT INTO task_runs (id,organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,started_at,finished_at) VALUES ($1,$2,$2,$3,$4,$5,'completed',now()+make_interval(secs=>$6),now()+make_interval(secs=>$6))").bind(run).bind(self.org).bind(task).bind(agent).bind(run.to_string()).bind(offset_secs as f64).execute(&self.pool).await.unwrap();
    }

    /// A refreshed client: reads the current delivery state, then writes.
    pub async fn delivery(&self, task: Uuid) -> Value {
        let (status, body) =
            call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/delivery"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }

    pub async fn report(&self, task: Uuid) -> Uuid {
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

    pub async fn verdict(&self, task: Uuid, report: Uuid, verdict: &str) -> (StatusCode, Value) {
        let data = self.delivery(task).await;
        let input = json!({"requestKey":Uuid::new_v4(),"reportId":report,"expectedVersion":data["taskVersion"],
            "expectedRevision":data["currentRevision"],"verdict":verdict,"reason":"Reviewed the exact revision.","humanMinutes":{"review":3}});
        call(&self.app, &self.jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/decisions"), input).await
    }

    pub async fn approve(&self, task: Uuid) -> (StatusCode, Value) {
        approve(self.app.clone(), self.jwt.clone(), task).await
    }

    pub async fn rejected_flag(&self, task: Uuid) -> Value {
        call(&self.app, &self.jwt, "GET", &format!("/api/v1/self-fix/tasks/{task}/review"), json!({})).await.1["data"]
            ["currentRevisionRejected"]
            .clone()
    }

    /// Start an approve, wait until it is paused at the provider merge call
    /// holding this task's merge mutex, cancel it (as when the client goes
    /// away), and wait until the mutex is released. The provider never answers
    /// that merge call; the returned sender keeps it pending.
    pub async fn cancel_while_merging(&self, task: Uuid, pr: i32) -> oneshot::Sender<()> {
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
    pub async fn state(&self, task: Uuid) -> (String, i32) {
        sqlx::query_as("SELECT review_status, merge_attempts FROM orchestration_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

pub async fn approve(app: Router, jwt: String, task: Uuid) -> (StatusCode, Value) {
    call(&app, &jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/approve"), json!({})).await
}

pub fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or_default()
}

pub const CAP: i32 = 5; // test_app_config: self_fix_max_merge_attempts

pub async fn forge_app(pool: PgPool) -> Router {
    let mut state = app_state_with_mock_provider(pool, "mock", "ok").await;
    let mut config = test_app_config("postgres://test");
    config.github_app_id = Some("12345".into());
    config.github_app_installation_id = Some("1".into());
    config.github_app_private_key = Some(include_str!("../fixtures/test_rsa_private_key.pem").to_string().into());
    config.github_app_repo = Some("acme/widgets".into());
    state.config = Arc::new(config);
    create_router(state)
}

/// Whether THIS task's merge mutex is held: the exact advisory key
/// (`hashtextextended('self-fix-merge:<org>:<task>', 0)`, a single bigint key
/// split into classid/objid), granted, in this database.
pub fn merge_lock_held(org: Uuid, task: Uuid) -> String {
    let key = format!("hashtextextended('self-fix-merge:{org}:{task}', 0)");
    format!(
        "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND granted AND objsubid=1 \
         AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) \
         AND classid::bigint=(({key}>>32)&4294967295) AND objid::bigint=({key}&4294967295))"
    )
}

/// Move the task row's last write 31 s into the past (past the settle window)
/// without waiting: the `updated_at` trigger is paused for this one update.
pub async fn backdate(pool: &PgPool, task: Uuid) {
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

pub async fn set_attempts(pool: &PgPool, task: Uuid, attempts: i32) {
    sqlx::query("UPDATE orchestration_tasks SET merge_attempts=$2 WHERE id=$1")
        .bind(task)
        .bind(attempts)
        .execute(pool)
        .await
        .unwrap();
}

/// Wait for an observable database condition (bounded; a timeout fails the
/// test). Polling only observes state; it never decides an outcome.
pub async fn wait_for(pool: &PgPool, condition: &str) {
    for _ in 0..1000 {
        if sqlx::query_scalar::<_, bool>(condition).fetch_one(pool).await.unwrap() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("condition not reached: {condition}");
}
