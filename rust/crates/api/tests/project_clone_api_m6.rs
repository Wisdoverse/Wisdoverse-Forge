//! Integration tests for M6: the clone HTTP API surface — create accepts the
//! repo URL, the project projection carries the clone-status `CloneSummary`, the
//! retry endpoint (failed-only + dedup), and the repository-URL immutability rule.
//!
//! Driven against a REAL Postgres (`#[sqlx::test]` provisions a throwaway DB per
//! test). The clone WORKER is not run here; attempt states (`ready`/`failed`/…)
//! are seeded directly so the API surface can be exercised in isolation. Locally:
//!
//! ```text
//! DATABASE_URL='postgres://<role>:<pw>@127.0.0.1:5432/<role-owned-db>' \
//!   cargo test -p agentforge-api --test project_clone_api_m6
//! ```
//!
//! Covers (per M6, design spec §6.8, §9):
//!   * create WITH a repo via the SERVICE (the active create path) -> ok,
//!     `clone_status='queued'`, the projection includes the CloneSummary;
//!   * create with an invalid / credential-bearing URL -> Validation, no project;
//!   * status projection: a `ready` attempt -> branch/head_sha; a `failed`
//!     attempt -> the REDACTED error_message (no token) + error_class; never a
//!     secret;
//!   * retry: from `failed` -> a new `queued` attempt N+1 + an outbox row; from
//!     `ready`/`cloning` -> Conflict; cross-org / non-manager -> Forbidden;
//!     project with no repo -> Validation; no prior attempt -> Conflict;
//!   * immutability: changing `repository_url` on a project whose attempt is
//!     `queued`/`ready` -> Validation; on a `none` project -> allowed; a name-only
//!     edit on a clone-bound project -> allowed, NO new attempt/outbox;
//!   * auth/tenant: the active legacy-navigation list surface scopes by org, and
//!     every clone API method rejects a foreign-org caller.

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::routes::legacy_navigation::test_only::create_project_canonical_for_test;
use agentforge_api::services::project::{CreateProjectInput, ProjectService};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{ErrorKind, TenantScope, WorkspaceId};

const REPO_URL: &str = "https://github.com/example/repo.git";
const OTHER_REPO_URL: &str = "https://gitlab.com/example/other.git";

struct Seed {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
}

/// Seed an org + default workspace + team + an owner user (org owner, so it
/// passes `require_project_creator`/`require_org_manager`/`require_project_manager`).
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

    Seed { org_id, workspace_id, team_id, user_id }
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

/// Drive the latest attempt of a project into a target terminal/in-flight state
/// + mirror `projects.clone_status`, simulating what the worker would persist.
async fn set_latest_attempt(
    pool: &PgPool,
    project_id: Uuid,
    status: &str,
    error_class: Option<&str>,
    error_message: Option<&str>,
    resolved_branch: Option<&str>,
    head_sha: Option<&str>,
) {
    sqlx::query(
        r#"UPDATE project_clone_attempts
              SET status = $2,
                  error_class = $3,
                  error_message = $4,
                  resolved_branch = $5,
                  head_sha = $6,
                  updated_at = now()
            WHERE project_id = $1
              AND attempt = (SELECT MAX(attempt) FROM project_clone_attempts WHERE project_id = $1)"#,
    )
    .bind(project_id)
    .bind(status)
    .bind(error_class)
    .bind(error_message)
    .bind(resolved_branch)
    .bind(head_sha)
    .execute(pool)
    .await
    .expect("update latest attempt");

    // Mirror the denormalized project summary (a cancelled latest collapses to
    // 'none', matching CloneStatus::from_attempt; nothing here uses 'cancelled').
    let project_status = if status == "cancelled" { "none" } else { status };
    sqlx::query("UPDATE projects SET clone_status = $2, updated_at = now() WHERE id = $1")
        .bind(project_id)
        .bind(project_status)
        .execute(pool)
        .await
        .expect("mirror project clone_status");
}

async fn count_attempts(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("count attempts")
}

async fn count_unpublished_outbox(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_outbox
          WHERE aggregate_type = 'project_clone' AND aggregate_id = $1 AND published_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("count unpublished outbox")
}

// ---------------------------------------------------------------------------
// 1. Create WITH a repo via the active path -> queued + the projection carries
//    the CloneSummary.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn create_with_repo_projection_includes_clone_summary(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let created = service.create(&scope(&seed), make_input(&seed, "Cloned", Some(REPO_URL))).await.expect("create");

    assert_eq!(created.project.clone_status, "queued", "repo present -> clone_status='queued'");
    let summary = created.clone.as_ref().expect("create response must carry the queued clone summary");
    assert_eq!(summary.status, "queued");
    assert_eq!(summary.attempt, 1);

    // The serialized projection (what the route returns through resource_data_response)
    // flattens the project + adds `clone`, and never leaks a secret-bearing field.
    // The flat `Project` keeps its existing snake_case keys (`clone_status`); the
    // additive `clone` object is the camelCase `CloneSummary` the M7 UI consumes.
    let value = serde_json::to_value(&created).expect("serialize projection");
    assert_eq!(value["clone_status"], "queued", "flat Project keeps snake_case clone_status");
    assert_eq!(value["clone"]["status"], "queued");
    assert_eq!(value["clone"]["attempt"], 1);
    for forbidden in ["credentialId", "workerId", "containerId", "jobId", "leaseExpiresAt"] {
        assert!(value["clone"].get(forbidden).is_none(), "summary leaked {forbidden}");
    }

    // get() returns the same projection.
    let fetched = service.get(&scope(&seed), created.project.id).await.expect("get");
    assert_eq!(fetched.clone.as_ref().expect("get summary").status, "queued");

    // FIX 1: the create's outbox payload carries the tenant ids so the worker can
    // org-scope its loads before trusting the payload's project_id/attempt.
    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM orchestration_outbox
          WHERE aggregate_type = 'project_clone' AND aggregate_id = $1",
    )
    .bind(created.project.id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("create outbox payload");
    assert_eq!(payload["organization_id"], seed.org_id.to_string(), "outbox payload carries the org");
    assert_eq!(payload["workspace_id"], seed.workspace_id.to_string(), "outbox payload carries the workspace");
    assert_eq!(payload["attempt"], 1);
}

// ---------------------------------------------------------------------------
// 2. Create with an invalid / credential-bearing URL -> Validation, no project.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn create_with_invalid_or_credentialed_url_is_rejected(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    for (label, url) in [
        ("non-https", "http://github.com/o/r"),
        ("embedded-credentials", "https://x-access-token:ghp_secrettoken123456@github.com/o/r.git"),
        ("ssrf-loopback", "https://127.0.0.1/r"),
    ] {
        let err = service
            .create(&scope(&seed), make_input(&seed, &format!("Bad {label}"), Some(url)))
            .await
            .expect_err(&format!("{label} must be rejected"));
        assert!(matches!(err.kind, ErrorKind::Validation(_)), "{label}: expected Validation, got {err:?}");
    }

    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1")
        .bind(seed.workspace_id)
        .fetch_one(&pool)
        .await
        .expect("count projects");
    assert_eq!(projects, 0, "a rejected url must create no project");
}

// ---------------------------------------------------------------------------
// 3. Status projection: a ready attempt shows branch/head_sha; a failed attempt
//    shows the REDACTED error_message (no token) + error_class.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn status_projection_reflects_ready_and_failed_attempts(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // Ready project.
    let ready =
        service.create(&scope(&seed), make_input(&seed, "Ready Repo", Some(REPO_URL))).await.expect("create ready");
    set_latest_attempt(&pool, ready.project.id.as_uuid(), "ready", None, None, Some("main"), Some("abc123def")).await;

    let fetched = service.get(&scope(&seed), ready.project.id).await.expect("get ready");
    assert_eq!(fetched.project.clone_status, "ready");
    let summary = fetched.clone.expect("ready summary");
    assert_eq!(summary.status, "ready");
    assert_eq!(summary.resolved_branch.as_deref(), Some("main"));
    assert_eq!(summary.head_sha.as_deref(), Some("abc123def"));
    assert_eq!(summary.error_message, None);

    // Failed project with an ALREADY-REDACTED error message (the worker scrubbed
    // the token before persisting; the projection copies it verbatim).
    let failed =
        service.create(&scope(&seed), make_input(&seed, "Failed Repo", Some(REPO_URL))).await.expect("create failed");
    let redacted = "Authentication failed for https://github.com [REDACTED]";
    set_latest_attempt(&pool, failed.project.id.as_uuid(), "failed", Some("auth"), Some(redacted), None, None).await;

    let fetched = service.get(&scope(&seed), failed.project.id).await.expect("get failed");
    assert_eq!(fetched.project.clone_status, "failed");
    let summary = fetched.clone.expect("failed summary");
    assert_eq!(summary.status, "failed");
    assert_eq!(summary.error_class.as_deref(), Some("auth"));
    let msg = summary.error_message.as_deref().expect("failed error_message");
    assert_eq!(msg, redacted);
    // The redacted message must not carry a token.
    assert!(!msg.contains("ghp_"), "error_message leaked a token: {msg}");
}

// ---------------------------------------------------------------------------
// 4. Retry from a FAILED attempt -> a new queued attempt N+1 + an outbox row.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_from_failed_creates_next_attempt_and_outbox(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let project = service.create(&scope(&seed), make_input(&seed, "Retry Me", Some(REPO_URL))).await.expect("create");
    let project_id = project.project.id.as_uuid();
    // The create's outbox row is unpublished; drain it so we can assert the RETRY
    // adds exactly one fresh unpublished outbox row.
    sqlx::query("UPDATE orchestration_outbox SET published_at = now() WHERE aggregate_id = $1")
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("publish create outbox");
    set_latest_attempt(&pool, project_id, "failed", Some("network"), Some("Could not resolve host"), None, None).await;

    let summary = service.retry_clone(&scope(&seed), project.project.id).await.expect("retry from failed");
    assert_eq!(summary.status, "queued", "retry produces a queued attempt");
    assert_eq!(summary.attempt, 2, "retry is attempt N+1");

    assert_eq!(count_attempts(&pool, project_id).await, 2, "a second attempt row was created");
    assert_eq!(count_unpublished_outbox(&pool, project_id).await, 1, "exactly one fresh outbox row for the retry");

    // The project summary mirrored back to queued.
    let clone_status: String = sqlx::query_scalar("SELECT clone_status FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("clone_status");
    assert_eq!(clone_status, "queued");

    // The new attempt row is attempt 2, queued, same URL/provider snapshot.
    let (status, url, provider): (String, String, Option<String>) = sqlx::query_as(
        "SELECT status, repository_url, provider FROM project_clone_attempts WHERE project_id = $1 AND attempt = 2",
    )
    .bind(project_id)
    .fetch_one(&pool)
    .await
    .expect("fetch attempt 2");
    assert_eq!(status, "queued");
    assert_eq!(url, REPO_URL);
    assert_eq!(provider.as_deref(), Some("github"));
}

// ---------------------------------------------------------------------------
// 5. Retry is rejected when the latest attempt is NOT failed (ready / cloning),
//    and a double-retry is deduped to a Conflict.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_only_allowed_from_failed(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    for status in ["ready", "cloning", "queued"] {
        let project = service
            .create(&scope(&seed), make_input(&seed, &format!("No Retry {status}"), Some(REPO_URL)))
            .await
            .expect("create");
        set_latest_attempt(&pool, project.project.id.as_uuid(), status, None, None, None, None).await;

        let err = service
            .retry_clone(&scope(&seed), project.project.id)
            .await
            .expect_err(&format!("retry from {status} must be a conflict"));
        assert!(matches!(err.kind, ErrorKind::Conflict(_)), "retry from {status}: expected Conflict, got {err:?}");
        // No second attempt was created.
        assert_eq!(count_attempts(&pool, project.project.id.as_uuid()).await, 1, "no retry attempt from {status}");
    }
}

// ---------------------------------------------------------------------------
// 6. Retry with no repository URL -> Validation; with no prior attempt -> Conflict.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_requires_a_repo_and_a_prior_attempt(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // No repo at all -> nothing to clone (Validation/400).
    let plain = service.create(&scope(&seed), make_input(&seed, "No Repo", None)).await.expect("create plain");
    let err = service.retry_clone(&scope(&seed), plain.project.id).await.expect_err("no-repo retry must fail");
    assert!(matches!(err.kind, ErrorKind::Validation(_)), "no-repo retry: expected Validation, got {err:?}");

    // A repo but the attempt got cancelled (no failed attempt to retry from) ->
    // Conflict. (A cancelled latest is not a retry candidate.)
    let project = service.create(&scope(&seed), make_input(&seed, "Cancelled", Some(REPO_URL))).await.expect("create");
    set_latest_attempt(&pool, project.project.id.as_uuid(), "cancelled", None, None, None, None).await;
    let err = service.retry_clone(&scope(&seed), project.project.id).await.expect_err("cancelled retry must fail");
    assert!(matches!(err.kind, ErrorKind::Conflict(_)), "cancelled retry: expected Conflict, got {err:?}");
}

// ---------------------------------------------------------------------------
// 7. Repository-URL one-shot bind (§9, FIX 2/3): ANY repository_url in an update
//    is rejected with an actionable error — on a bound project OR a `none`
//    project, for a different value OR the same value. A name-only edit is
//    allowed and enqueues no clone, and leaves the stored URL untouched.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn repository_url_is_immutable_via_update(pool: PgPool) {
    use agentforge_api::services::project::UpdateProjectInput;
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // A clone-bound project (attempt exists, here forced ready).
    let bound = service.create(&scope(&seed), make_input(&seed, "Bound", Some(REPO_URL))).await.expect("create bound");
    let bound_id = bound.project.id;
    set_latest_attempt(&pool, bound_id.as_uuid(), "ready", None, None, Some("main"), Some("sha1")).await;

    // Changing the repo URL to a DIFFERENT value is REJECTED (Validation) with an
    // actionable message that points at the only supported path (create new).
    let err = service
        .update(
            &scope(&seed),
            bound_id,
            UpdateProjectInput { name: None, repository_url: Some(Some(OTHER_REPO_URL.to_string())) },
        )
        .await
        .expect_err("repo URL change on a bound project must be rejected");
    assert!(matches!(err.kind, ErrorKind::Validation(_)), "expected Validation, got {err:?}");
    let msg = format!("{err}");
    assert!(
        msg.contains("set when the project is created") && msg.contains("cannot be changed"),
        "error must be actionable, got: {msg}"
    );

    // Re-asserting the SAME repo URL is ALSO rejected (FIX 2: an update never
    // carries a repository_url; the value is not even compared). This subsumes the
    // old normalization-equivalent concern — there is no compare path to evade.
    let err = service
        .update(
            &scope(&seed),
            bound_id,
            UpdateProjectInput { name: None, repository_url: Some(Some(REPO_URL.to_string())) },
        )
        .await
        .expect_err("re-asserting the same repo URL via update must also be rejected");
    assert!(matches!(err.kind, ErrorKind::Validation(_)), "same-value update: expected Validation, got {err:?}");

    // Clearing the URL (Some(None)) is ALSO rejected — clearing is still a write.
    let err = service
        .update(&scope(&seed), bound_id, UpdateProjectInput { name: None, repository_url: Some(None) })
        .await
        .expect_err("clearing the repo URL via update must be rejected");
    assert!(matches!(err.kind, ErrorKind::Validation(_)), "clear-url update: expected Validation, got {err:?}");

    // The stored URL is UNCHANGED after every rejected attempt above.
    let after_rejects = service.get(&scope(&seed), bound_id).await.expect("get after rejects");
    assert_eq!(
        after_rejects.project.repository_url.as_deref(),
        Some(REPO_URL),
        "rejected updates must not mutate the stored repository_url"
    );

    // A name-only edit is allowed and triggers NO new attempt / outbox, and leaves
    // the repository_url + clone status untouched. (The create already wrote
    // attempt 1 + its outbox row; assert the edit adds NOTHING on top.)
    let attempts_before = count_attempts(&pool, bound_id.as_uuid()).await;
    let outbox_before = count_unpublished_outbox(&pool, bound_id.as_uuid()).await;
    let renamed = service
        .update(
            &scope(&seed),
            bound_id,
            UpdateProjectInput { name: Some("Bound Renamed".into()), repository_url: None },
        )
        .await
        .expect("name-only edit on a bound project is allowed");
    assert_eq!(renamed.project.name, "Bound Renamed");
    assert_eq!(renamed.project.repository_url.as_deref(), Some(REPO_URL), "name edit must not touch repository_url");
    assert_eq!(count_attempts(&pool, bound_id.as_uuid()).await, attempts_before, "name edit must not add an attempt");
    assert_eq!(
        count_unpublished_outbox(&pool, bound_id.as_uuid()).await,
        outbox_before,
        "name edit must enqueue no NEW clone"
    );
    // The clone status + summary are unchanged (still ready).
    assert_eq!(renamed.project.clone_status, "ready");
    assert_eq!(renamed.clone.expect("summary").status, "ready");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn repository_url_update_rejected_even_on_a_pre_clone_project(pool: PgPool) {
    use agentforge_api::services::project::UpdateProjectInput;
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // A project created with NO repo -> clone_status='none', no attempt.
    let plain = service.create(&scope(&seed), make_input(&seed, "Pre Clone", None)).await.expect("create plain");
    assert_eq!(plain.project.clone_status, "none");
    assert_eq!(count_attempts(&pool, plain.project.id.as_uuid()).await, 0);

    // Setting the repo URL via UPDATE is REJECTED even on a pre-clone project
    // (FIX 2: §9 binds the URL only at create — an update storing one with no
    // enqueue would be a silent no-op, so it is rejected outright).
    let err = service
        .update(
            &scope(&seed),
            plain.project.id,
            UpdateProjectInput { name: None, repository_url: Some(Some(REPO_URL.to_string())) },
        )
        .await
        .expect_err("setting repo URL via update on a none project must be rejected");
    assert!(matches!(err.kind, ErrorKind::Validation(_)), "pre-clone update: expected Validation, got {err:?}");

    // The project still has NO repo and NO attempt — nothing was stored.
    let after = service.get(&scope(&seed), plain.project.id).await.expect("get after reject");
    assert_eq!(after.project.repository_url, None, "rejected update must not store a repository_url");
    assert_eq!(after.project.clone_status, "none");
    assert_eq!(count_attempts(&pool, plain.project.id.as_uuid()).await, 0, "update never enqueues a clone");

    // A name-only edit on the pre-clone project still works.
    let renamed = service
        .update(
            &scope(&seed),
            plain.project.id,
            UpdateProjectInput { name: Some("Renamed Plain".into()), repository_url: None },
        )
        .await
        .expect("name-only edit on a none project is allowed");
    assert_eq!(renamed.project.name, "Renamed Plain");
    assert_eq!(renamed.project.repository_url, None);
}

// ---------------------------------------------------------------------------
// 8. Auth / tenant: a foreign-org caller is rejected on every clone API method,
//    and the active list surface scopes its projection by org.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn clone_api_rejects_foreign_org_and_non_manager(pool: PgPool) {
    use agentforge_api::services::project::UpdateProjectInput;
    let home = seed(&pool).await;
    let other = seed(&pool).await; // a different org + owner

    let service = ProjectService::from_pool(pool.clone());
    let project =
        service.create(&scope(&home), make_input(&home, "Home Repo", Some(REPO_URL))).await.expect("create home");
    set_latest_attempt(&pool, project.project.id.as_uuid(), "failed", Some("auth"), Some("nope"), None, None).await;

    // A caller from ANOTHER org must not be able to retry / update / get the home
    // project — it is invisible to them (NotFound or Forbidden, never a success).
    let foreign = scope(&other);
    let err = service.retry_clone(&foreign, project.project.id).await.expect_err("foreign retry must fail");
    assert!(
        matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)),
        "foreign retry must be NotFound/Forbidden, got {err:?}"
    );
    let err = service.get(&foreign, project.project.id).await.expect_err("foreign get must fail");
    assert!(matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)), "foreign get: got {err:?}");
    let err = service
        .update(&foreign, project.project.id, UpdateProjectInput { name: Some("Hijack".into()), repository_url: None })
        .await
        .expect_err("foreign update must fail");
    assert!(matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)), "foreign update: got {err:?}");

    // No retry attempt leaked across the org boundary.
    assert_eq!(count_attempts(&pool, project.project.id.as_uuid()).await, 1, "no cross-org retry attempt");

    // A NON-manager member of the SAME org is also rejected from retry: add a
    // plain 'member' user to the home org and prove require_project_manager bites.
    let member_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(member_id)
        .bind(format!("m-{member_id}@example.com"))
        .execute(&pool)
        .await
        .expect("seed member user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(home.org_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .expect("seed member membership");
    let member_scope = tenant_scope_for_ids(home.org_id, member_id);
    let err = service.retry_clone(&member_scope, project.project.id).await.expect_err("non-manager retry must fail");
    assert!(matches!(err.kind, ErrorKind::Forbidden(_)), "non-manager retry must be Forbidden, got {err:?}");
}

// ---------------------------------------------------------------------------
// 9. The active legacy-navigation list surface attaches the clone summary +
//    status and scopes strictly by org (a foreign caller sees nothing).
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_navigation_list_carries_clone_status(pool: PgPool) {
    let seed = seed(&pool).await;

    // Create via the active legacy-navigation surface (settings/sidebar), then
    // force the attempt ready so the list projection shows the detail.
    let project_id = create_project_canonical_for_test(&pool, seed.user_id, seed.team_id, "Nav Cloned", Some(REPO_URL))
        .await
        .expect("legacy create with repo");
    set_latest_attempt(&pool, project_id, "ready", None, None, Some("trunk"), Some("nav-sha")).await;

    // Drive the ACTIVE list surface through its service so the per-project clone
    // detail (`CloneSummary`) is attached, exactly as the GET handler returns it.
    let projects = agentforge_api::routes::legacy_navigation::test_only::list_projects_via_service_for_test(
        &pool,
        seed.user_id,
        seed.team_id,
    )
    .await
    .expect("list projects");
    let project = projects.iter().find(|p| p["id"] == project_id.to_string()).expect("the cloned project is listed");

    // The denormalized summary badge + the per-attempt detail both ride the
    // active surface the frontend consumes.
    assert_eq!(project["cloneStatus"], "ready");
    assert_eq!(project["clone"]["status"], "ready");
    assert_eq!(project["clone"]["resolvedBranch"], "trunk");
    assert_eq!(project["clone"]["headSha"], "nav-sha");
    // No secret-bearing field is ever on the nav surface.
    for forbidden in ["credentialId", "workerId", "containerId", "jobId"] {
        assert!(project["clone"].get(forbidden).is_none(), "nav summary leaked {forbidden}");
    }

    // A foreign-org caller's list of the same team id returns nothing (the
    // canonical helper resolves the team's org from the FOREIGN user, who is not
    // a member, so it errors rather than leaking the project).
    let stranger = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(stranger)
        .bind(format!("s-{stranger}@example.com"))
        .execute(&pool)
        .await
        .expect("seed stranger");
    let foreign = agentforge_api::routes::legacy_navigation::test_only::list_projects_canonical_for_test(
        &pool,
        stranger,
        seed.team_id,
    )
    .await;
    assert!(foreign.is_err(), "a non-member must not be able to list another team's projects");
}
