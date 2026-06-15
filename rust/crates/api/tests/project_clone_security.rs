//! Integration-level SECURITY proofs for project-git-clone (M8, design spec §10
//! and §13 step 8). The earlier milestones unit-test each control in isolation:
//! the M1 URL deny-list and `WorkspaceDirName`, the M4 secret-mount ownership and
//! the non-Debug-leaking `RawStderr`/`SecretBytes`, the M5 redaction-before-
//! persist, the M6 cross-org API rejection. This file adds the END-TO-END proofs
//! that those controls actually bite across the create → worker → persist path,
//! and REFERENCES (rather than duplicates) the controls an equivalent integration
//! test already covers in a sibling file.
//!
//! Driven against a REAL Postgres (`#[sqlx::test]` provisions a throwaway DB per
//! test); the clone container is faked behind the M5 `CloneRunner` so no Docker
//! daemon is needed. Locally:
//!
//! ```text
//! DATABASE_URL='postgres://<role>:<pw>@127.0.0.1:5432/<role-owned-db>' \
//!   cargo test -p agentforge-api --test project_clone_security
//! ```
//!
//! Properties proven here (design spec §10):
//!   * PATH TRAVERSAL — a hostile project name (`../../etc`, `..`, `.git`,
//!     control chars, all-symbols, slashes) yields a filesystem-safe
//!     `workspace_dir_name` (no `/`, no `..`, non-empty) AND the worker resolves
//!     the clone target strictly inside the workspace projects root, materializing
//!     the repo at `<projects_root>/<dir>` and nowhere else.
//!   * SSRF FAILS CLOSED (the M4-deferred test) — a create whose repo URL host is
//!     loopback / RFC1918 / `169.254.169.254` / `.local` is REJECTED at create by
//!     the in-app deny-list (the layer the API can enforce); a normal
//!     `https://github.com/...` create is accepted. The runtime egress firewall
//!     (deploy-layer, the M4 runbook) and the in-container pre-resolve are the
//!     2nd/3rd layers and are out of scope for an in-process test.
//!   * NO SECRET IN LOGS — a clone whose `Failed` stderr glues a token into a
//!     redirected URL is driven through the worker; the persisted `error_message`
//!     carries no token (this asserts the redaction boundary end-to-end), AND the
//!     `CloneRunOutcome` `Debug` (what `tracing::warn!(?outcome)` would emit) is
//!     proven to contain no token — the `RawStderr` non-printing `Debug` design.
//!
//! Properties covered elsewhere (referenced, not re-proven here):
//!   * TENANT BOUNDARY (worker): a poisoned job payload naming a foreign org
//!     resolves no attempt and never runs — `project_clone_worker.rs::`
//!     `poisoned_payload_org_mismatch_does_not_read_across_orgs`.
//!   * TENANT BOUNDARY (API): a cross-org caller cannot see another org's clone
//!     status or trigger its retry — `project_clone_api_m6.rs::`
//!     `clone_api_rejects_foreign_org_and_non_manager` +
//!     `legacy_navigation_list_carries_clone_status`. A focused API-level
//!     `get`/`retry` cross-org assertion is included below for completeness.
//!   * REDACTION-AT-PERSIST equality (`error_message == redact(raw)`) —
//!     `project_clone_worker.rs::failed_outcome_redacts_and_schedules_bounded_retry`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use agentforge_api::services::git_credential::GitCredentialService;
use agentforge_api::services::project::{CreateProjectInput, ProjectService};
use agentforge_api::services::project_clone_worker::{
    CloneRunner, CloneWorkerConfig, DEFAULT_MAX_ATTEMPTS, ProjectCloneWorker,
};
use agentforge_api::test_support::{TEST_LLM_ENCRYPTION_KEY, tenant_scope_for_ids};
use agentforge_core::{ErrorKind, TenantScope, WorkspaceId};
use agentforge_platform::{CloneRunOutcome, CloneRunSpec, RawStderr};

const REPO_URL: &str = "https://github.com/example/repo.git";

// ---------------------------------------------------------------------------
// Throwaway workspace root (mirrors the worker-test TempRoot; removed on drop).
// ---------------------------------------------------------------------------

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("agentforge-clone-sec-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp workspace root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Fake CloneRunner — materializes `<staging>/repo` on Ready so the worker's
// atomic rename has a real source; records the staging path it was handed.
// ---------------------------------------------------------------------------

struct FakeRunner {
    outcome: Mutex<Box<dyn Fn() -> CloneRunOutcome + Send + Sync>>,
    staging_paths: Mutex<Vec<PathBuf>>,
}

impl FakeRunner {
    fn new(outcome: impl Fn() -> CloneRunOutcome + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { outcome: Mutex::new(Box::new(outcome)), staging_paths: Mutex::new(Vec::new()) })
    }

    fn last_staging(&self) -> Option<PathBuf> {
        self.staging_paths.lock().unwrap().last().cloned()
    }
}

#[async_trait::async_trait]
impl CloneRunner for FakeRunner {
    async fn run(&self, spec: CloneRunSpec) -> agentforge_core::AppResult<CloneRunOutcome> {
        self.staging_paths.lock().unwrap().push(spec.staging_host_path.clone());
        let outcome = (self.outcome.lock().unwrap())();
        if matches!(outcome, CloneRunOutcome::Ready { .. }) {
            let repo = spec.staging_host_path.join("repo");
            tokio::fs::create_dir_all(&repo).await.expect("create fake repo dir");
            tokio::fs::write(repo.join("README.md"), b"hello").await.expect("write fake file");
        }
        Ok(outcome)
    }

    async fn sweep_orphans(&self) -> agentforge_core::AppResult<usize> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Seed helpers (org + workspace + team + owner user).
// ---------------------------------------------------------------------------

struct Seed {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
    workspace_root: TempRoot,
}

async fn seed(pool: &PgPool) -> Seed {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");

    let team_id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.teams (id, organization_id, name, slug) VALUES ($1, $2, 'Engineering', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("team-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");

    let user_id = Uuid::new_v4();
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
        .expect("seed org membership");

    Seed { org_id, workspace_id, team_id, user_id, workspace_root: TempRoot::new() }
}

fn scope(seed: &Seed) -> TenantScope {
    tenant_scope_for_ids(seed.org_id, seed.user_id)
}

fn make_input(seed: &Seed, name: &str, url: Option<&str>) -> CreateProjectInput {
    CreateProjectInput {
        workspace_id: WorkspaceId::from(seed.workspace_id),
        team_id: Some(seed.team_id.into()),
        name: name.to_string(),
        repository_url: url.map(str::to_string),
    }
}

/// Build a worker over a fake runner with the temp workspace root + a secret root
/// under the same temp dir (mirrors the M5 worker-test harness).
fn worker<R: CloneRunner + 'static>(pool: &PgPool, seed: &Seed, runner: Arc<R>) -> ProjectCloneWorker<R> {
    let secret_root = seed.workspace_root.path().join("clone-secrets");
    let config = CloneWorkerConfig {
        image: "agentforge-clone:test".to_string(),
        workspace_root: seed.workspace_root.path().to_string_lossy().to_string(),
        secret_root,
        timeout: Duration::from_secs(60),
        lease_ttl: Duration::from_secs(120),
        heartbeat_interval: Duration::from_secs(40),
        max_bytes: None,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        retry_backoff: Duration::from_millis(1),
        poll_interval: Duration::from_millis(10),
        reconcile_interval: Duration::from_millis(10),
    };
    let credentials = Arc::new(GitCredentialService::from_pool(pool.clone(), Some(TEST_LLM_ENCRYPTION_KEY)));
    ProjectCloneWorker::new(pool.clone(), credentials, runner, config)
}

/// The canonical projects-root the worker resolves for this seed's tenant. The
/// clone target is exactly `<projects_root>/<workspace_dir_name>`.
fn projects_root(seed: &Seed) -> PathBuf {
    seed.workspace_root
        .path()
        .join("orgs")
        .join(seed.org_id.to_string())
        .join("workspaces")
        .join(seed.workspace_id.to_string())
        .join("projects")
}

async fn project_dir_name(pool: &PgPool, project_id: Uuid) -> String {
    sqlx::query_scalar("SELECT workspace_dir_name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("fetch dir name")
}

async fn attempt_error_message(pool: &PgPool, project_id: Uuid, attempt: i32) -> Option<String> {
    sqlx::query_scalar("SELECT error_message FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2")
        .bind(project_id)
        .bind(attempt)
        .fetch_one(pool)
        .await
        .expect("fetch error_message")
}

/// Create a cloned project directly through the transactional repo path (so
/// attempt 1 + outbox exist) and return its id.
async fn create_cloned_project(pool: &PgPool, seed: &Seed, name: &str, url: &str) -> Uuid {
    let mut tx = pool.begin().await.expect("begin");
    let project = ProjectRepository::create_with_clone_in_tx(
        &mut tx,
        &scope(seed),
        ProjectCreateTx {
            workspace_id: WorkspaceId::from(seed.workspace_id),
            team_id: seed.team_id,
            name: name.to_string(),
            color: None,
            description: None,
            clone: Some(CloneRequest::parse(url).expect("parse url")),
        },
    )
    .await
    .expect("create with clone");
    tx.commit().await.expect("commit");
    project.id.as_uuid()
}

// ===========================================================================
// 1. PATH TRAVERSAL — a hostile name yields a filesystem-safe dir name and the
//    worker resolves + materializes strictly inside the projects root.
// ===========================================================================

/// Every hostile project name maps to a filesystem-safe `workspace_dir_name`
/// (no `/`, no `..`, non-empty, no reserved token) and resolves to a path that
/// is exactly one component under the projects root. This is the create-path
/// half of the traversal proof; the worker half follows in the next test.
#[sqlx::test(migrations = "../db/migrations")]
async fn hostile_project_names_yield_safe_dir_names(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // A battery of traversal / injection shapes. None may produce a dir name that
    // contains a path separator, a parent token, or a reserved name — and each
    // must resolve to a single child of the projects root.
    //
    // NOTE: a literal NUL (`0x00`) is intentionally NOT in this list — Postgres
    // rejects a NUL in ANY `TEXT` value (including the project NAME column) before
    // the dir-name policy is even reached, so it is a DB-level guarantee, not a
    // path-safety concern. The `\u{0001}…\u{007f}` case below proves the
    // derivation still sanitizes non-NUL control characters.
    let hostile = [
        "../../etc",
        "..",
        ".",
        ".git",
        "a/b/c",
        "..\\..\\windows",
        "%2e%2e%2f",
        "....//....//",
        "\u{0001}control\u{007f}\tname\n",
        "!@#$%^&*()",
        "   ",
        "/etc/passwd",
    ];

    let root = projects_root(&seed);
    for (i, name) in hostile.iter().enumerate() {
        // Use a unique suffix so each create gets its own row (the dir-name policy
        // is what we assert, not collision handling).
        let unique = format!("{name}-sec-{i}");
        let created = service.create(&scope(&seed), make_input(&seed, &unique, None)).await.expect("create");
        let dir = created.project.workspace_dir_name;

        assert!(!dir.is_empty(), "dir name must be non-empty for input {name:?}");
        assert!(!dir.contains('/'), "dir name must not contain a forward slash for input {name:?}: {dir:?}");
        assert!(!dir.contains('\\'), "dir name must not contain a backslash for input {name:?}: {dir:?}");
        assert!(!dir.contains(".."), "dir name must not contain a parent token for input {name:?}: {dir:?}");
        assert_ne!(dir, ".", "dir name must not be the current-dir token for input {name:?}");
        assert_ne!(dir, "..", "dir name must not be the parent-dir token for input {name:?}");
        assert_ne!(dir, ".git", "dir name must not be the git metadata dir for input {name:?}");
        assert!(
            dir.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "dir name must be [a-z0-9-] for input {name:?}: {dir:?}"
        );

        // Structural containment: the resolved target is exactly root + one
        // normal component (no `..`/root/prefix token escapes the projects root).
        let target = root.join(&dir);
        let tail = target.strip_prefix(&root).expect("resolved target must live under the projects root");
        let mut comps = tail.components();
        let only = comps.next().expect("exactly one component under root");
        assert!(matches!(only, std::path::Component::Normal(_)), "the single tail component must be Normal: {dir:?}");
        assert!(comps.next().is_none(), "the resolved target must be exactly ONE component under root: {dir:?}");
    }
}

/// The WORKER half: a project created with a path-traversal name clones into the
/// derived safe directory under the projects root — and the staging dir the
/// runner was handed lives under that same root, never at a sibling/parent path.
/// (Proves the worker's `WorkspaceDirName::parse` + `resolve_under` keep the
/// clone inside the tenant's projects root end-to-end.)
#[sqlx::test(migrations = "../db/migrations")]
async fn worker_resolves_hostile_named_clone_inside_projects_root(pool: PgPool) {
    let seed = seed(&pool).await;
    // A traversal name on a real clone: the stored dir name is derived-safe, and
    // the worker must materialize the repo strictly under the projects root.
    let project_id = create_cloned_project(&pool, &seed, "../../etc/passwd", REPO_URL).await;

    let dir = project_dir_name(&pool, project_id).await;
    assert!(!dir.contains('/') && !dir.contains("..") && !dir.is_empty(), "stored dir name must be safe: {dir:?}");

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "abc123".to_string(),
        bytes: 4096,
    });
    let worker = worker(&pool, &seed, runner.clone());
    worker.process_attempt_for_test(seed.org_id, project_id, 1).await.expect("process attempt");

    let root = projects_root(&seed);
    let target = root.join(&dir);
    // The repo landed at exactly <projects_root>/<safe-dir>, nowhere else.
    assert!(target.join("README.md").exists(), "the clone must materialize at the derived safe path under root");
    assert!(target.starts_with(&root), "the materialized target must live under the projects root");

    // The staging dir the runner was handed is also under the projects root (the
    // container mounts only a per-clone staging dir, never a parent or sibling).
    let staging = runner.last_staging().expect("runner was called");
    assert!(staging.starts_with(&root), "staging dir must be under the projects root, got {staging:?}");
    assert!(!staging.to_string_lossy().contains(".."), "staging path must contain no parent token, got {staging:?}");
}

// ===========================================================================
// 2. SSRF FAILS CLOSED (the M4-deferred test) — an internal-address repo URL is
//    rejected at create; a normal github URL is accepted.
// ===========================================================================

/// A create whose repo URL host is loopback / RFC1918 / link-local-metadata /
/// `.local` / a port-only authority is REJECTED at create time by the in-app
/// `ProjectRepositoryUrl` deny-list, and NOTHING is written (no project, no
/// attempt, no outbox). This is the layer the API itself can enforce; the
/// runtime egress firewall (the M4 `clone-egress-firewall.md` runbook) and the
/// in-container pre-resolve are the 2nd/3rd SSRF layers, out of scope here.
#[sqlx::test(migrations = "../db/migrations")]
async fn ssrf_internal_address_urls_are_rejected_at_create(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // Each host class git would otherwise resolve+connect to internally.
    let blocked = [
        ("loopback-v4", "https://127.0.0.1/r.git"),
        ("loopback-name", "https://localhost/r.git"),
        ("loopback-v6", "https://[::1]/r.git"),
        ("metadata", "https://169.254.169.254/latest/meta-data/"),
        ("link-local", "https://169.254.10.10/r.git"),
        ("rfc1918-10", "https://10.0.0.5/r.git"),
        ("rfc1918-172", "https://172.16.0.9/r.git"),
        ("rfc1918-192", "https://192.168.1.20/r.git"),
        ("mdns-local", "https://printer.local/r.git"),
        ("port-only", "https://:8080/r.git"),
    ];

    for (label, url) in blocked {
        let err = service
            .create(&scope(&seed), make_input(&seed, &format!("SSRF {label}"), Some(url)))
            .await
            .expect_err(&format!("{label}: an internal-address repo URL must be rejected at create"));
        assert!(
            matches!(err.kind, ErrorKind::Validation(_)),
            "{label}: SSRF rejection must be a Validation (400), got {err:?}"
        );
    }

    // Fails closed: not a single row was written for any blocked URL.
    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1")
        .bind(seed.workspace_id)
        .fetch_one(&pool)
        .await
        .expect("count projects");
    assert_eq!(projects, 0, "a blocked SSRF URL must create no project");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts")
        .fetch_one(&pool)
        .await
        .expect("count attempts");
    assert_eq!(attempts, 0, "a blocked SSRF URL must create no clone attempt");
    let outbox: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_outbox WHERE aggregate_type = 'project_clone'")
            .fetch_one(&pool)
            .await
            .expect("count outbox");
    assert_eq!(outbox, 0, "a blocked SSRF URL must enqueue no clone job");
}

/// The positive control: a normal public `https://github.com/...` create IS
/// accepted (queued + one attempt), proving the deny-list above rejected the
/// internal hosts specifically and is not blanket-failing every URL.
#[sqlx::test(migrations = "../db/migrations")]
async fn ssrf_normal_https_github_url_is_accepted(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let created = service
        .create(&scope(&seed), make_input(&seed, "Legit", Some(REPO_URL)))
        .await
        .expect("a normal https github URL must be accepted");
    assert_eq!(created.project.clone_status, "queued", "a public github URL clones (queued)");

    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(created.project.id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count attempts");
    assert_eq!(attempts, 1, "the accepted URL writes exactly one clone attempt");
}

// ===========================================================================
// 3. NO SECRET IN LOGS — a glued-token Failed stderr is driven through the
//    worker; neither the persisted error_message NOR the outcome's Debug (what
//    a `tracing::warn!(?outcome)` line would emit) leaks the token.
// ===========================================================================

/// The token a malicious/redirecting git remote could glue into a 403 stderr.
const GLUED_TOKEN: &str = "ghp_supersecrettoken0123456789ABCDEF";

#[sqlx::test(migrations = "../db/migrations")]
async fn failed_clone_leaks_no_token_to_persisted_message_or_logs(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "TokenLeak", REPO_URL).await;

    // The exact leak shape: a token glued into a redirected-auth URL in stderr.
    let raw_tail = format!("fatal: unable to access 'https://x-access-token:{GLUED_TOKEN}@github.com/x.git': 403");

    // (a) LOG-LINE PROOF (the new assertion, via the leak-safe Debug design):
    // the `RawStderr` newtype prints only a byte count, so the `Debug` of the
    // outcome the worker logs (`tracing::warn!(?outcome)`) can never spill the
    // token. We assert that directly on the value the worker would log.
    let outcome = CloneRunOutcome::Failed { exit_code: 128, stderr_tail: RawStderr::new(raw_tail.clone()) };
    let debug_line = format!("{outcome:?}");
    assert!(
        !debug_line.contains(GLUED_TOKEN),
        "the outcome Debug (what tracing logs) must NOT contain the token: {debug_line}"
    );
    assert!(
        debug_line.contains("RawStderr(<") && debug_line.contains("bytes, unredacted>"),
        "the Failed stderr must render as a non-printing byte count, got: {debug_line}"
    );

    // (b) PERSIST PROOF (end-to-end): drive the SAME stderr through the worker and
    // assert the stored error_message carries no token. (The exact
    // `error_message == redact(raw)` equality is proven in the M5 worker test
    // `failed_outcome_redacts_and_schedules_bounded_retry`; here we prove the
    // boundary holds in this independent run too.)
    let runner = FakeRunner::new(move || CloneRunOutcome::Failed {
        exit_code: 128,
        stderr_tail: RawStderr::new(raw_tail.clone()),
    });
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(seed.org_id, project_id, 1).await.expect("process attempt");

    let stored = attempt_error_message(&pool, project_id, 1).await.expect("error_message persisted");
    assert!(!stored.contains(GLUED_TOKEN), "the persisted error_message leaked the token: {stored}");
    assert!(!stored.contains("ghp_"), "the persisted error_message leaked a token prefix: {stored}");
    // And the redactor agrees: the persisted value equals redact(raw).
    let expected = agentforge_api::domain::project_clone::redact(&format!(
        "fatal: unable to access 'https://x-access-token:{GLUED_TOKEN}@github.com/x.git': 403"
    ))
    .into_string();
    assert_eq!(stored, expected, "persisted error_message must equal redact(raw).into_string()");
}

// ===========================================================================
// 4. TENANT BOUNDARY (API) — a cross-org caller cannot read a foreign project's
//    clone status or trigger its retry. (The exhaustive matrix lives in
//    project_clone_api_m6.rs::clone_api_rejects_foreign_org_and_non_manager;
//    this is a focused, self-contained restatement of the get + retry edges so
//    the security suite stands alone.)
// ===========================================================================

#[sqlx::test(migrations = "../db/migrations")]
async fn cross_org_caller_cannot_read_or_retry_a_foreign_clone(pool: PgPool) {
    let home = seed(&pool).await;
    let other = seed(&pool).await; // a different org + owner
    let service = ProjectService::from_pool(pool.clone());

    let project =
        service.create(&scope(&home), make_input(&home, "Home Repo", Some(REPO_URL))).await.expect("create home");
    // Force the latest attempt failed so a retry WOULD be valid for the owner —
    // proving the foreign rejection is the tenant guard, not a state conflict.
    sqlx::query(
        "UPDATE project_clone_attempts SET status = 'failed', error_class = 'auth', error_message = 'nope'
          WHERE project_id = $1 AND attempt = (SELECT MAX(attempt) FROM project_clone_attempts WHERE project_id = $1)",
    )
    .bind(project.project.id.as_uuid())
    .execute(&pool)
    .await
    .expect("force failed");
    sqlx::query("UPDATE projects SET clone_status = 'failed' WHERE id = $1")
        .bind(project.project.id.as_uuid())
        .execute(&pool)
        .await
        .expect("mirror failed");

    let foreign = scope(&other);

    // The foreign org cannot GET the home project's clone status...
    let err = service.get(&foreign, project.project.id).await.expect_err("foreign get must fail");
    assert!(
        matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)),
        "foreign get must be NotFound/Forbidden, got {err:?}"
    );

    // ...nor trigger its retry.
    let err = service.retry_clone(&foreign, project.project.id).await.expect_err("foreign retry must fail");
    assert!(
        matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)),
        "foreign retry must be NotFound/Forbidden, got {err:?}"
    );

    // No retry attempt leaked across the org boundary (still exactly attempt 1).
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(project.project.id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count attempts");
    assert_eq!(attempts, 1, "a foreign retry must not create an attempt across the org boundary");
}
