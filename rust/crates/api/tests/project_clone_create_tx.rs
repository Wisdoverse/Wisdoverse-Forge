//! Integration tests for M2: transactional project-with-repo create + the
//! `project_clone` transactional-outbox → `job_queue` relay.
//!
//! Run against a real Postgres (`#[sqlx::test]` provisions a throwaway DB per
//! test). Locally:
//!
//! ```text
//! DATABASE_URL='postgres://<role>:<pw>@127.0.0.1:5432/<role-owned-db>' \
//!   cargo test -p agentforge-api --test project_clone_create_tx
//! ```
//!
//! Covers (per the M2 spec, §6.1–6.2):
//!   * create WITHOUT repo -> clone_status='none', no attempt, no outbox/job;
//!   * create WITH repo -> exactly one project + one queued attempt + one outbox
//!     row, all visible only after commit;
//!   * a forced failure after the project insert rolls back project+attempt+outbox
//!     (no zombie);
//!   * a foreign-org workspace_id is rejected and inserts nothing;
//!   * two same-name creates in one workspace get two distinct dir names;
//!   * the outbox publisher relays exactly one job_queue row (correct unique_key),
//!     marks the outbox published, and a second publish is a no-op;
//!   * the legacy-navigation surface produces the attempt + outbox + a
//!     filesystem-safe workspace_dir_name (not a raw caller slug).

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use agentforge_api::routes::legacy_navigation::test_only::{
    create_project_canonical_for_test, create_project_with_slug_for_test,
};
use agentforge_api::services::project::{CreateProjectInput, ProjectService};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{ErrorKind, TenantScope, WorkspaceId};
use agentforge_jobs::relay_next_clone_outbox;

const REPO_URL: &str = "https://github.com/example/repo.git";

struct Seed {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
}

/// Seed an org + default workspace + team + an owner user (org owner, so it
/// passes `require_project_creator`/`require_org_manager`).
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

async fn count_attempts(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("count attempts")
}

async fn count_clone_outbox(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_outbox WHERE aggregate_type = 'project_clone' AND aggregate_id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("count outbox")
}

async fn count_clone_jobs(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE queue = 'project_clone'")
        .fetch_one(pool)
        .await
        .expect("count jobs")
}

async fn count_projects_named(pool: &PgPool, workspace_id: Uuid, name: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1 AND name = $2")
        .bind(workspace_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("count projects by name")
}

// ---------------------------------------------------------------------------
// 1. Create WITHOUT a repo -> clone_status='none', no attempt, no outbox/job.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn create_without_repo_has_no_clone_artifacts(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let project = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Plain Project".to_string(),
                repository_url: None,
            },
        )
        .await
        .expect("create without repo");

    let project_id = project.id.as_uuid();
    assert_eq!(project.clone_status, "none", "no repo -> clone_status must be 'none'");
    assert_eq!(project.repository_url, None);
    assert!(!project.workspace_dir_name.is_empty(), "dir name must be allocated");
    assert_eq!(count_attempts(&pool, project_id).await, 0, "no attempt row without a repo");
    assert_eq!(count_clone_outbox(&pool, project_id).await, 0, "no outbox row without a repo");
    assert_eq!(count_clone_jobs(&pool).await, 0, "no job without a repo");

    // The default group must still be created in the same tx.
    let groups: i64 = sqlx::query_scalar("SELECT count(*) FROM groups WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("count groups");
    assert_eq!(groups, 1, "default project group must be created");
}

// ---------------------------------------------------------------------------
// 2. Create WITH a repo -> exactly one project + one queued attempt + one outbox.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn create_with_repo_writes_attempt_and_outbox(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let project = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Cloned Project".to_string(),
                repository_url: Some(REPO_URL.to_string()),
            },
        )
        .await
        .expect("create with repo");

    let project_id = project.id.as_uuid();
    assert_eq!(project.clone_status, "queued", "repo present -> clone_status='queued'");
    assert_eq!(project.repository_url.as_deref(), Some(REPO_URL));

    assert_eq!(count_attempts(&pool, project_id).await, 1, "exactly one attempt row");
    assert_eq!(count_clone_outbox(&pool, project_id).await, 1, "exactly one outbox row");

    // The attempt is attempt 1, queued, with the URL + github provider snapshot.
    let (attempt, status, url, provider): (i32, String, String, Option<String>) = sqlx::query_as(
        "SELECT attempt, status, repository_url, provider FROM project_clone_attempts WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&pool)
    .await
    .expect("fetch attempt");
    assert_eq!(attempt, 1);
    assert_eq!(status, "queued");
    assert_eq!(url, REPO_URL);
    assert_eq!(provider.as_deref(), Some("github"));

    // The outbox row carries the {project_id, attempt} payload + discriminators.
    let (agg_type, event_type, payload): (String, String, serde_json::Value) =
        sqlx::query_as("SELECT aggregate_type, event_type, payload FROM orchestration_outbox WHERE aggregate_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("fetch outbox");
    assert_eq!(agg_type, "project_clone");
    assert_eq!(event_type, "clone_requested");
    assert_eq!(payload["project_id"], serde_json::json!(project_id.to_string()));
    assert_eq!(payload["attempt"], serde_json::json!(1));

    // No job yet — the publisher has not run.
    assert_eq!(count_clone_jobs(&pool).await, 0, "no job until the publisher relays the outbox");
}

// ---------------------------------------------------------------------------
// 3. A forced failure after the project insert rolls back the whole tuple.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn failure_after_project_insert_rolls_back_everything(pool: PgPool) {
    let seed = seed(&pool).await;

    // Drive the transactional body directly so we can inject a failure AFTER the
    // project + attempt + outbox inserts but BEFORE commit, then roll back.
    let mut tx = pool.begin().await.expect("begin tx");
    let project = ProjectRepository::create_with_clone_in_tx(
        &mut tx,
        &scope(&seed),
        ProjectCreateTx {
            workspace_id: WorkspaceId::from(seed.workspace_id),
            team_id: seed.team_id,
            name: "Doomed Project".to_string(),
            color: None,
            description: None,
            clone: Some(CloneRequest::parse(REPO_URL).expect("parse url")),
        },
    )
    .await
    .expect("create within tx");
    let project_id = project.id.as_uuid();

    // Inside the same (uncommitted) tx the rows ARE visible...
    let in_tx: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await
        .expect("count in tx");
    assert_eq!(in_tx, 1, "attempt visible inside the open tx");

    // Simulate a later sub-step failure: abandon the transaction (rollback).
    tx.rollback().await.expect("rollback");

    // ...but after rollback NOTHING is committed: no project, no attempt, no outbox.
    let project_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("count project");
    assert_eq!(project_rows, 0, "no zombie project after rollback");
    assert_eq!(count_attempts(&pool, project_id).await, 0, "no zombie attempt after rollback");
    assert_eq!(count_clone_outbox(&pool, project_id).await, 0, "no zombie outbox after rollback");
}

// ---------------------------------------------------------------------------
// 4. A workspace from another org is rejected; nothing is inserted.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn foreign_org_workspace_is_rejected(pool: PgPool) {
    let home = seed(&pool).await;
    // A second org with its own workspace; the first org's creator must not be
    // able to create a project into it.
    let other = seed(&pool).await;

    let service = ProjectService::from_pool(pool.clone());
    let result = service
        .create(
            &scope(&home),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(other.workspace_id), // foreign-org workspace
                team_id: Some(home.team_id.into()),
                name: "Cross Org".to_string(),
                repository_url: Some(REPO_URL.to_string()),
            },
        )
        .await;

    let err = result.expect_err("foreign-org workspace must be rejected");
    use agentforge_core::ErrorKind;
    assert!(
        matches!(err.kind, ErrorKind::NotFound(_) | ErrorKind::Forbidden(_)),
        "foreign workspace must be NotFound/Forbidden, got: {err:?}"
    );

    // Absolutely nothing was inserted for the caller's org against the foreign ws.
    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1")
        .bind(other.workspace_id)
        .fetch_one(&pool)
        .await
        .expect("count projects in foreign ws");
    assert_eq!(projects, 0, "no project inserted into the foreign workspace");
}

// ---------------------------------------------------------------------------
// 5. Two same-name creates in one workspace get two distinct dir names.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn same_name_creates_allocate_distinct_dir_names(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let input = |name: &str| CreateProjectInput {
        workspace_id: WorkspaceId::from(seed.workspace_id),
        team_id: Some(seed.team_id.into()),
        name: name.to_string(),
        repository_url: None,
    };

    let first = service.create(&scope(&seed), input("My Repo")).await.expect("first create");
    let second = service.create(&scope(&seed), input("My Repo")).await.expect("second create");

    assert_eq!(first.workspace_dir_name, "my-repo", "first takes the bare derived name");
    assert_ne!(first.workspace_dir_name, second.workspace_dir_name, "a same-name sibling must get a distinct dir name");
    assert!(
        second.workspace_dir_name.starts_with("my-repo-"),
        "the collision is resolved with a numeric suffix, got {}",
        second.workspace_dir_name
    );

    // Both are live and the per-workspace dir uniqueness holds.
    let distinct: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT workspace_dir_name) FROM projects WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(seed.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count distinct dirs");
    assert_eq!(distinct, 2, "two distinct live dir names");
}

// ---------------------------------------------------------------------------
// 6. The publisher relays exactly one job; a second publish is a no-op.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn outbox_relay_enqueues_one_job_and_is_idempotent(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let project = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Relayed".to_string(),
                repository_url: Some(REPO_URL.to_string()),
            },
        )
        .await
        .expect("create with repo");
    let project_id = project.id.as_uuid();

    // First relay: one job appears with the attempt-scoped unique key; outbox
    // is marked published.
    let relayed = relay_next_clone_outbox(&pool).await.expect("relay once");
    assert!(relayed.is_some(), "first relay must do work");
    assert_eq!(count_clone_jobs(&pool).await, 1, "exactly one job after relay");

    let (queue, unique_key, payload): (String, Option<String>, serde_json::Value) =
        sqlx::query_as("SELECT queue, unique_key, payload FROM job_queue WHERE queue = 'project_clone'")
            .fetch_one(&pool)
            .await
            .expect("fetch job");
    assert_eq!(queue, "project_clone");
    assert_eq!(unique_key.as_deref(), Some(format!("project_clone:{project_id}:1").as_str()));
    assert_eq!(payload["project_id"], serde_json::json!(project_id.to_string()));
    assert_eq!(payload["attempt"], serde_json::json!(1));

    let published: bool =
        sqlx::query_scalar("SELECT published_at IS NOT NULL FROM orchestration_outbox WHERE aggregate_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("fetch published");
    assert!(published, "outbox row must be marked published after relay");

    // Second relay: the outbox row is already published, so there is nothing to
    // relay and the job count is unchanged (no duplicate).
    let again = relay_next_clone_outbox(&pool).await.expect("relay twice");
    assert!(again.is_none(), "no unpublished rows remain");
    assert_eq!(count_clone_jobs(&pool).await, 1, "second relay must not duplicate the job");

    // Defense in depth: even a (hypothetical) re-publish of the SAME unique key is
    // a no-op against idx_job_queue_unique_key.
    let dup: Option<Uuid> = sqlx::query_scalar(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ('project_clone', '{}'::jsonb, 0, NOW(), $1, 5)
           ON CONFLICT (unique_key) WHERE unique_key IS NOT NULL DO NOTHING
           RETURNING id"#,
    )
    .bind(format!("project_clone:{project_id}:1"))
    .fetch_optional(&pool)
    .await
    .expect("dup insert");
    assert!(dup.is_none(), "a duplicate unique_key insert must be a no-op");
    assert_eq!(count_clone_jobs(&pool).await, 1, "still exactly one job");
}

// ---------------------------------------------------------------------------
// 7. The legacy-navigation surface produces attempt + outbox + safe dir name.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_navigation_create_with_repo_funnels_through_tx(pool: PgPool) {
    let seed = seed(&pool).await;

    let project_id =
        create_project_canonical_for_test(&pool, seed.user_id, seed.team_id, "Legacy Cloned!!", Some(REPO_URL))
            .await
            .expect("legacy create with repo");

    // A filesystem-safe dir name was derived — NOT a raw caller slug.
    let dir: String = sqlx::query_scalar("SELECT workspace_dir_name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("fetch dir");
    assert_eq!(dir, "legacy-cloned", "legacy create must derive a filesystem-safe dir name");

    // The clone artifacts exist exactly as on the flat surface.
    assert_eq!(count_attempts(&pool, project_id).await, 1, "legacy create with repo writes one attempt");
    assert_eq!(count_clone_outbox(&pool, project_id).await, 1, "legacy create with repo writes one outbox row");

    let clone_status: String = sqlx::query_scalar("SELECT clone_status FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("fetch clone_status");
    assert_eq!(clone_status, "queued");

    // And the default group is created in the same tx.
    let groups: i64 = sqlx::query_scalar("SELECT count(*) FROM groups WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("count groups");
    assert_eq!(groups, 1, "legacy create still makes the default project group");
}

/// A legacy-navigation create WITHOUT a repo still routes through the shared
/// transactional path (filesystem-safe dir, default group) but writes no clone
/// artifacts — the symmetric negative of the test above.
#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_navigation_create_without_repo_has_no_clone_artifacts(pool: PgPool) {
    let seed = seed(&pool).await;

    let project_id = create_project_canonical_for_test(&pool, seed.user_id, seed.team_id, "Legacy Plain", None)
        .await
        .expect("legacy create without repo");

    let (dir, clone_status): (String, String) =
        sqlx::query_as("SELECT workspace_dir_name, clone_status FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("fetch project");
    assert_eq!(dir, "legacy-plain");
    assert_eq!(clone_status, "none");
    assert_eq!(count_attempts(&pool, project_id).await, 0);
    assert_eq!(count_clone_outbox(&pool, project_id).await, 0);
}

// ===========================================================================
// M2 review — failure-branch + edge-case coverage (F1 + test gaps A–E).
// ===========================================================================

// ---------------------------------------------------------------------------
// A. A REAL post-project-insert sub-step failure inside the PRODUCTION wrapper
//    (`create_with_clone`, which commits) must roll the whole tuple back. We
//    do not abandon a tx by hand here: we drive the committing wrapper with a
//    scope whose user_id references NO real user, so the default-group insert
//    (step 4, `groups.created_by REFERENCES users(id)`) fails with a real FK
//    violation AFTER the project + attempt + outbox inserts. The wrapper must
//    return Err and commit nothing.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn production_wrapper_rolls_back_on_post_project_failure(pool: PgPool) {
    let seed = seed(&pool).await;
    let repo = ProjectRepository::new(pool.clone());

    // A scope on the REAL org but a PHANTOM user id (no users row). Everything up
    // to and including the project + attempt + outbox inserts succeeds; the
    // default-group insert then violates groups.created_by -> users(id).
    let phantom_user = Uuid::new_v4();
    let scope = tenant_scope_for_ids(seed.org_id, phantom_user);

    let result = repo
        .create_with_clone(
            &scope,
            ProjectCreateTx {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: seed.team_id,
                name: "Rollback Victim".to_string(),
                color: None,
                description: None,
                clone: Some(CloneRequest::parse(REPO_URL).expect("parse url")),
            },
        )
        .await;

    // The committing wrapper must surface the failure...
    assert!(result.is_err(), "a post-project sub-step failure must make create_with_clone return Err");

    // ...and commit NOTHING: no project, no attempt, no outbox (the whole tuple
    // rolled back atomically — there is no zombie project row).
    assert_eq!(
        count_projects_named(&pool, seed.workspace_id, "Rollback Victim").await,
        0,
        "the production wrapper must NOT commit the project on a post-project failure"
    );
    let orphan_attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts")
        .fetch_one(&pool)
        .await
        .expect("count all attempts");
    assert_eq!(orphan_attempts, 0, "no attempt may survive the rollback");
    let orphan_outbox: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_outbox WHERE aggregate_type = 'project_clone'")
            .fetch_one(&pool)
            .await
            .expect("count all clone outbox");
    assert_eq!(orphan_outbox, 0, "no outbox row may survive the rollback");
}

// ---------------------------------------------------------------------------
// B. Dir-suffix EXHAUSTION: the loop tries `name`, then `name-2` … `name-16`
//    (16 distinct names). Seed all 16 as LIVE projects, then a 17th create with
//    the same name must fail with `Conflict` (choose-a-different-name) and
//    insert no 17th row.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn dir_suffix_exhaustion_returns_conflict(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let make = |name: &str| CreateProjectInput {
        workspace_id: WorkspaceId::from(seed.workspace_id),
        team_id: Some(seed.team_id.into()),
        name: name.to_string(),
        repository_url: None,
    };

    // Occupy exactly the 16 names the allocation loop can reach: "Exhaust" (bare
    // -> dir "exhaust") plus "Exhaust 2" .. "Exhaust 16" (dirs "exhaust-2" ..
    // "exhaust-16"). Each create takes the bare derived name for its own NAME, so
    // 16 distinct live dir names now occupy the whole reachable suffix space.
    service.create(&scope(&seed), make("Exhaust")).await.expect("seed bare name");
    for n in 2..=16 {
        service.create(&scope(&seed), make(&format!("Exhaust {n}"))).await.expect("seed suffixed name");
    }

    // Sanity: 16 distinct live dir names now exist for this workspace.
    let live_dirs: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT workspace_dir_name) FROM projects WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(seed.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count live dirs");
    assert_eq!(live_dirs, 16, "the 16 reachable dir names must all be occupied");

    // The 17th "Exhaust" create: every reachable suffix collides -> Conflict.
    let err = service.create(&scope(&seed), make("Exhaust")).await.expect_err("17th same-name create must fail");
    assert!(
        matches!(err.kind, ErrorKind::Conflict(_)),
        "dir exhaustion must be a Conflict (choose a different name), got: {err:?}"
    );

    // No 17th row was inserted (the loop never committed a project).
    assert_eq!(
        count_projects_named(&pool, seed.workspace_id, "Exhaust").await,
        1,
        "only the first 'Exhaust' row exists; the exhausted 17th create inserted nothing"
    );
}

// ---------------------------------------------------------------------------
// C. Relay in-tx ON CONFLICT path: a pre-existing job_queue row with the exact
//    unique_key makes the relay's INSERT a no-op, but the relay must STILL mark
//    the outbox row published (so it is not stranded for re-relay forever) and
//    must not create a duplicate job.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn relay_marks_published_even_when_enqueue_is_a_conflict_noop(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    let project = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Conflict Relay".to_string(),
                repository_url: Some(REPO_URL.to_string()),
            },
        )
        .await
        .expect("create with repo");
    let project_id = project.id.as_uuid();

    // Pre-insert a job_queue row with the EXACT unique_key the relay will use,
    // so the relay's INSERT ... ON CONFLICT DO NOTHING is a no-op.
    let unique_key = format!("project_clone:{project_id}:1");
    sqlx::query(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ('project_clone', '{}'::jsonb, 0, NOW(), $1, 5)"#,
    )
    .bind(&unique_key)
    .execute(&pool)
    .await
    .expect("pre-insert the colliding job");
    assert_eq!(count_clone_jobs(&pool).await, 1, "exactly the pre-inserted job exists");

    // Relay: the enqueue is a conflict no-op, but the outbox row MUST still be
    // marked published (otherwise the head-of-line row is stranded forever) and
    // the relay must report that it did work.
    let relayed = relay_next_clone_outbox(&pool).await.expect("relay over a conflict");
    assert!(relayed.is_some(), "the relay must report work even when enqueue was a no-op");
    assert_eq!(count_clone_jobs(&pool).await, 1, "no duplicate job — the unique_key dedups the enqueue");

    let published: bool =
        sqlx::query_scalar("SELECT published_at IS NOT NULL FROM orchestration_outbox WHERE aggregate_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("fetch published");
    assert!(published, "the outbox row MUST be marked published even when the enqueue was a conflict no-op");

    // And there is nothing left to relay (no infinite re-selection of the row).
    let again = relay_next_clone_outbox(&pool).await.expect("second relay");
    assert!(again.is_none(), "the published row is not re-selected");
}

// ---------------------------------------------------------------------------
// D. An invalid / blocked repo URL is rejected by the SERVICE before any write:
//    no project, no attempt, no outbox. Covers a non-https URL and an SSRF
//    literal-host URL.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn invalid_repo_url_via_service_inserts_nothing(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    for (label, url) in [("non-https", "http://github.com/o/r"), ("ssrf-loopback", "https://127.0.0.1/r")] {
        let result = service
            .create(
                &scope(&seed),
                CreateProjectInput {
                    workspace_id: WorkspaceId::from(seed.workspace_id),
                    team_id: Some(seed.team_id.into()),
                    name: format!("Bad URL {label}"),
                    repository_url: Some(url.to_string()),
                },
            )
            .await;
        let err = result.expect_err(&format!("{label} url must be rejected"));
        assert!(
            matches!(err.kind, ErrorKind::Validation(_)),
            "{label}: a bad repo url must be a Validation error, got: {err:?}"
        );
    }

    // Nothing was written for EITHER attempt.
    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE workspace_id = $1")
        .bind(seed.workspace_id)
        .fetch_one(&pool)
        .await
        .expect("count projects");
    assert_eq!(projects, 0, "a rejected url must create no project");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts")
        .fetch_one(&pool)
        .await
        .expect("count attempts");
    assert_eq!(attempts, 0, "a rejected url must create no attempt");
    let outbox: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_outbox WHERE aggregate_type = 'project_clone'")
            .fetch_one(&pool)
            .await
            .expect("count outbox");
    assert_eq!(outbox, 0, "a rejected url must create no outbox row");
}

// ---------------------------------------------------------------------------
// E. The legacy-navigation create DISCARDS a hostile caller slug: the on-disk
//    identity (`workspace_dir_name`) is derived from the NAME alone, so a
//    `../../etc` caller slug has ZERO influence on the directory name.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_create_discards_hostile_caller_slug(pool: PgPool) {
    let seed = seed(&pool).await;

    // A path-traversal caller slug must never reach the on-disk identity.
    let project_id =
        create_project_with_slug_for_test(&pool, seed.user_id, seed.team_id, "Safe Name", Some("../../etc"), None)
            .await
            .expect("legacy create with a hostile slug");

    let dir: String = sqlx::query_scalar("SELECT workspace_dir_name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("fetch dir");
    // Derived from the NAME ("Safe Name" -> "safe-name"), NOT from "../../etc".
    assert_eq!(dir, "safe-name", "the caller slug must have zero influence on the on-disk identity");
    assert!(!dir.contains('/'), "no slash from the caller slug may survive");
    assert!(!dir.contains(".."), "no traversal token from the caller slug may survive");
}

// ---------------------------------------------------------------------------
// F (optional). A self-hosted git host (no known provider) still persists the
//    attempt + outbox, with a NULL provider snapshot.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn self_hosted_host_persists_attempt_with_null_provider(pool: PgPool) {
    let seed = seed(&pool).await;
    let service = ProjectService::from_pool(pool.clone());

    // A self-hosted host matches no known SaaS provider -> provider stays NULL.
    let project = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Self Hosted".to_string(),
                repository_url: Some("https://git.example.com/team/repo.git".to_string()),
            },
        )
        .await
        .expect("create with a self-hosted repo");
    let project_id = project.id.as_uuid();

    assert_eq!(project.clone_status, "queued");
    assert_eq!(count_attempts(&pool, project_id).await, 1, "self-hosted create still writes one attempt");
    assert_eq!(count_clone_outbox(&pool, project_id).await, 1, "self-hosted create still writes one outbox row");

    let provider: Option<String> =
        sqlx::query_scalar("SELECT provider FROM project_clone_attempts WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .expect("fetch provider");
    assert_eq!(provider, None, "an unknown host must record a NULL provider, not a guessed one");
}

// ---------------------------------------------------------------------------
// F5. A POISON outbox row (structurally-undecodable payload) must be
//     DEAD-LETTERED by the relay — marked published WITHOUT enqueueing a job —
//     so it cannot stall the relay head forever, and a NEWER valid row behind it
//     still relays.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn relay_dead_letters_a_poison_row_and_keeps_draining(pool: PgPool) {
    let seed = seed(&pool).await;

    // Insert an OLDER project_clone outbox row whose payload can never decode into
    // CloneOutboxPayload (missing required fields). It has the earliest created_at,
    // so the relay's `ORDER BY created_at ASC` selects it FIRST.
    sqlx::query(
        r#"INSERT INTO orchestration_outbox
               (id, organization_id, aggregate_type, aggregate_id, event_type, payload, created_at)
           VALUES (gen_random_uuid(), $1, 'project_clone', $2, 'clone_requested',
                   '{"not_a_clone_payload": true}'::jsonb, NOW() - INTERVAL '1 hour')"#,
    )
    .bind(seed.org_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("insert poison outbox row");

    // A NEWER, VALID project-with-repo create -> a decodable outbox row behind the
    // poison one.
    let service = ProjectService::from_pool(pool.clone());
    let good = service
        .create(
            &scope(&seed),
            CreateProjectInput {
                workspace_id: WorkspaceId::from(seed.workspace_id),
                team_id: Some(seed.team_id.into()),
                name: "Behind The Poison".to_string(),
                repository_url: Some(REPO_URL.to_string()),
            },
        )
        .await
        .expect("create the valid row behind the poison one");
    let good_id = good.id.as_uuid();

    // First relay hits the POISON row: it must be dead-lettered (marked published)
    // and NO job enqueued for it.
    let first = relay_next_clone_outbox(&pool).await.expect("relay over the poison row");
    assert!(first.is_some(), "the relay must report work (it dead-lettered the poison row)");
    assert_eq!(count_clone_jobs(&pool).await, 0, "a poison row must NOT enqueue a job");

    let poison_published: bool = sqlx::query_scalar(
        "SELECT published_at IS NOT NULL FROM orchestration_outbox
          WHERE aggregate_type = 'project_clone' AND payload ? 'not_a_clone_payload'",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch poison published");
    assert!(poison_published, "the poison row must be marked published so it cannot stall the head");

    // Second relay reaches the VALID row behind it and enqueues exactly one job —
    // proving the poison row did not block newer rows (no head-of-line stall).
    let second = relay_next_clone_outbox(&pool).await.expect("relay the valid row behind the poison one");
    assert!(second.is_some(), "the valid row behind the poison one must relay");
    assert_eq!(count_clone_jobs(&pool).await, 1, "exactly the valid row's job is enqueued");

    let (unique_key,): (Option<String>,) =
        sqlx::query_as("SELECT unique_key FROM job_queue WHERE queue = 'project_clone'")
            .fetch_one(&pool)
            .await
            .expect("fetch the relayed job");
    assert_eq!(
        unique_key.as_deref(),
        Some(format!("project_clone:{good_id}:1").as_str()),
        "the enqueued job is the VALID row's, not the poison row's"
    );
}
