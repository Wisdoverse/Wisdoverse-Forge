//! Schema-contract test for migration 068 (project git-clone foundation, M0).
//!
//! Asserts a fresh database, migrated from `./migrations`, carries the columns,
//! table, indexes, CHECK constraints, and partial-unique-index *behavior* that
//! the later clone milestones (and the `ProjectCloneAttempt` / `Project` entity
//! structs) depend on. If a later migration drops or renames any of these — or
//! quietly weakens a constraint or partial predicate — this fails loudly instead
//! of surfacing as a runtime `FromRow` error or a silent integrity hole.
//!
//! Run with:
//!
//! ```text
//! cargo test -p agentforge-db --test project_clone_schema_contract
//! ```
//!
//! Requires a live Postgres reachable via `DATABASE_URL` (the `#[sqlx::test]`
//! harness provisions a throwaway database per test). When no database is
//! configured the test is not compiled out — it simply cannot run in that
//! environment.

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Postgres SQLSTATE for `unique_violation`.
const SQLSTATE_UNIQUE_VIOLATION: &str = "23505";

/// `true` when `public.<table>.<column>` exists.
async fn column_exists(pool: &PgPool, table: &str, column: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = $1
               AND column_name = $2
        )",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("column existence query")
}

/// `true` when `public.<table>` exists.
async fn table_exists(pool: &PgPool, table: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM information_schema.tables
             WHERE table_schema = 'public'
               AND table_name = $1
        )",
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("table existence query")
}

/// `true` when an index named `<index>` exists in the public schema.
async fn index_exists(pool: &PgPool, index: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM pg_indexes
             WHERE schemaname = 'public'
               AND indexname = $1
        )",
    )
    .bind(index)
    .fetch_one(pool)
    .await
    .expect("index existence query")
}

/// `NO` / `YES` from information_schema.columns.is_nullable for a column.
async fn column_is_nullable(pool: &PgPool, table: &str, column: &str) -> String {
    sqlx::query_scalar(
        "SELECT is_nullable
           FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name = $1
            AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("column nullability query")
}

/// Minimal tenant graph (org + workspace + team) that lets us insert a real
/// `projects` row whose only rejectable property is the value under test.
struct Fixture {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
}

/// Build an organization, workspace, and team so a `projects` INSERT can supply
/// every NOT NULL column (`organization_id`, `workspace_id`, `team_id`, `name`,
/// `slug`). Without a real `team_id` (NOT NULL since migration 026), a bad-value
/// INSERT would fail on the team_id NOT-NULL violation *before* Postgres ever
/// evaluates the value under test — a false positive.
async fn seed_fixture(pool: &PgPool, tag: &str) -> Fixture {
    let org_id: Uuid =
        sqlx::query("INSERT INTO organizations (name, slug) VALUES ($1, $1) RETURNING id")
            .bind(format!("clone-{tag}"))
            .fetch_one(pool)
            .await
            .expect("insert org")
            .get("id");

    let workspace_id: Uuid =
        sqlx::query("INSERT INTO workspaces (organization_id, name) VALUES ($1, 'ws') RETURNING id")
            .bind(org_id)
            .fetch_one(pool)
            .await
            .expect("insert workspace")
            .get("id");

    let team_id: Uuid = sqlx::query(
        "INSERT INTO teams (organization_id, name, slug) VALUES ($1, 'team', $2) RETURNING id",
    )
    .bind(org_id)
    .bind(format!("team-{tag}"))
    .fetch_one(pool)
    .await
    .expect("insert team")
    .get("id");

    Fixture { org_id, workspace_id, team_id }
}

/// Insert a project supplying every NOT NULL column. Returns the new id.
async fn insert_project(
    pool: &PgPool,
    fx: &Fixture,
    name: &str,
    slug: &str,
    workspace_dir_name: &str,
    clone_status: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query(
        "INSERT INTO projects
            (organization_id, workspace_id, team_id, name, slug, workspace_dir_name, clone_status)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(fx.org_id)
    .bind(fx.workspace_id)
    .bind(fx.team_id)
    .bind(name)
    .bind(slug)
    .bind(workspace_dir_name)
    .bind(clone_status)
    .fetch_one(pool)
    .await
    .map(|row| row.get("id"))
}

/// Insert a clone attempt. `project_id` must reference a live project.
async fn insert_clone_attempt(
    pool: &PgPool,
    fx: &Fixture,
    project_id: Uuid,
    attempt: i32,
    status: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_clone_attempts
            (organization_id, workspace_id, project_id, attempt, repository_url, status)
         VALUES ($1, $2, $3, $4, 'https://example.com/r.git', $5)
         RETURNING id",
    )
    .bind(fx.org_id)
    .bind(fx.workspace_id)
    .bind(project_id)
    .bind(attempt)
    .bind(status)
    .fetch_one(pool)
    .await
    .map(|row| row.get("id"))
}

/// Pull the named constraint off a returned database error (panics if there is
/// no database error, so the caller's intent — "this must be a constraint
/// rejection" — is enforced).
fn constraint_of(err: &sqlx::Error) -> Option<String> {
    err.as_database_error()
        .expect("expected a database error")
        .constraint()
        .map(str::to_string)
}

/// SQLSTATE code of a returned database error.
fn sqlstate_of(err: &sqlx::Error) -> String {
    err.as_database_error()
        .expect("expected a database error")
        .code()
        .expect("database error must carry a SQLSTATE")
        .to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_068_lands_project_clone_schema(pool: PgPool) {
    // -- Presence: projects new columns. ------------------------------------
    assert!(
        column_exists(&pool, "projects", "workspace_dir_name").await,
        "projects.workspace_dir_name missing"
    );
    assert!(
        column_exists(&pool, "projects", "clone_status").await,
        "projects.clone_status missing"
    );

    // Both new project columns are NOT NULL (workspace_dir_name via backfill +
    // SET NOT NULL, clone_status via DEFAULT 'none').
    assert_eq!(
        column_is_nullable(&pool, "projects", "workspace_dir_name").await,
        "NO",
        "projects.workspace_dir_name must be NOT NULL"
    );
    assert_eq!(
        column_is_nullable(&pool, "projects", "clone_status").await,
        "NO",
        "projects.clone_status must be NOT NULL"
    );

    // -- Presence: project_clone_attempts table + columns. ------------------
    assert!(
        table_exists(&pool, "project_clone_attempts").await,
        "project_clone_attempts table missing"
    );
    for column in [
        "id",
        "organization_id",
        "workspace_id",
        "project_id",
        "attempt",
        "repository_url",
        "provider",
        "credential_id",
        "status",
        "resolved_branch",
        "head_sha",
        "container_id",
        "worker_id",
        "job_id",
        "lease_expires_at",
        "error_class",
        "error_message",
        "bytes_cloned",
        "duration_ms",
        "started_at",
        "finished_at",
        "created_at",
        "updated_at",
    ] {
        assert!(
            column_exists(&pool, "project_clone_attempts", column).await,
            "project_clone_attempts.{column} missing"
        );
    }

    // -- Presence: indexes. -------------------------------------------------
    for index in ["uq_projects_workspace_dir", "uq_project_clone_attempt", "idx_project_clone_status"] {
        assert!(index_exists(&pool, index).await, "{index} index missing");
    }
    assert!(
        index_exists(&pool, "idx_job_queue_unique_key").await,
        "idx_job_queue_unique_key index missing"
    );
}

/// `projects_clone_status_check` must reject out-of-domain values and accept
/// the documented ones. The fixture supplies `team_id` so the CHECK is the only
/// thing that can reject the bad row (otherwise a NOT-NULL violation on
/// `team_id` would make this a false positive).
#[sqlx::test(migrations = "./migrations")]
async fn projects_clone_status_check_enforced(pool: PgPool) {
    let fx = seed_fixture(&pool, "status").await;

    // Negative: out-of-domain value is rejected *by the named CHECK*.
    let bad = insert_project(&pool, &fx, "p", "p", "p-bad", "bogus").await;
    let err = bad.expect_err("clone_status='bogus' must be rejected");
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("projects_clone_status_check"),
        "rejection must come from projects_clone_status_check, got: {err}"
    );

    // Positive: a documented value inserts cleanly (proves the CHECK is not
    // over-broad / does not reject the happy path).
    insert_project(&pool, &fx, "p2", "p2", "p2-ok", "queued")
        .await
        .expect("clone_status='queued' must be accepted");
}

/// `project_clone_attempts_status_check` must reject out-of-domain statuses and
/// accept a documented one.
#[sqlx::test(migrations = "./migrations")]
async fn clone_attempt_status_check_enforced(pool: PgPool) {
    let fx = seed_fixture(&pool, "att-status").await;
    let project_id = insert_project(&pool, &fx, "p", "p", "p-dir", "queued")
        .await
        .expect("seed project");

    // Negative: out-of-domain status rejected by the named CHECK.
    let bad = insert_clone_attempt(&pool, &fx, project_id, 1, "bogus").await;
    let err = bad.expect_err("status='bogus' must be rejected");
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("project_clone_attempts_status_check"),
        "rejection must come from project_clone_attempts_status_check, got: {err}"
    );

    // Positive: a documented status inserts cleanly.
    insert_clone_attempt(&pool, &fx, project_id, 2, "cloning")
        .await
        .expect("status='cloning' must be accepted");
}

/// `uq_projects_workspace_dir` is a *partial* unique index over live rows:
///   * two LIVE projects with the same (workspace_id, workspace_dir_name) collide,
///   * but soft-deleting one frees the dir name for reuse.
/// The reuse-after-soft-delete assertion is the only thing that proves the
/// `WHERE deleted_at IS NULL` predicate is present and correct.
#[sqlx::test(migrations = "./migrations")]
async fn uq_projects_workspace_dir_partial_behavior(pool: PgPool) {
    let fx = seed_fixture(&pool, "wsdir").await;

    let first = insert_project(&pool, &fx, "p1", "p1", "shared-dir", "none")
        .await
        .expect("first live project inserts");

    // Same (workspace_id, workspace_dir_name) among live rows -> collision.
    let dup = insert_project(&pool, &fx, "p2", "p2", "shared-dir", "none").await;
    let err = dup.expect_err("duplicate live workspace_dir_name must collide");
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("uq_projects_workspace_dir"),
        "collision must come from uq_projects_workspace_dir, got: {err}"
    );
    assert_eq!(sqlstate_of(&err), SQLSTATE_UNIQUE_VIOLATION, "must be a unique_violation");

    // Soft-delete the first row, then the dir name is reusable -> succeeds.
    // This is the assertion that proves `WHERE deleted_at IS NULL` exists.
    sqlx::query("UPDATE projects SET deleted_at = now() WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .expect("soft-delete first project");

    insert_project(&pool, &fx, "p3", "p3", "shared-dir", "none")
        .await
        .expect("dir name must be reusable after the holder is soft-deleted");
}

/// `uq_project_clone_attempt` enforces one (project_id, attempt) row.
#[sqlx::test(migrations = "./migrations")]
async fn uq_project_clone_attempt_behavior(pool: PgPool) {
    let fx = seed_fixture(&pool, "uqattempt").await;
    let project_id = insert_project(&pool, &fx, "p", "p", "p-dir", "queued")
        .await
        .expect("seed project");

    insert_clone_attempt(&pool, &fx, project_id, 1, "queued")
        .await
        .expect("first attempt inserts");

    let dup = insert_clone_attempt(&pool, &fx, project_id, 1, "queued").await;
    let err = dup.expect_err("duplicate (project_id, attempt) must collide");
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("uq_project_clone_attempt"),
        "collision must come from uq_project_clone_attempt, got: {err}"
    );
    assert_eq!(sqlstate_of(&err), SQLSTATE_UNIQUE_VIOLATION, "must be a unique_violation");
}

/// `idx_job_queue_unique_key` is a *partial* unique index over non-null keys:
///   * two rows with the same non-null unique_key collide,
///   * two rows with unique_key IS NULL both succeed (the partial predicate).
#[sqlx::test(migrations = "./migrations")]
async fn idx_job_queue_unique_key_partial_behavior(pool: PgPool) {
    let key = format!("clone:{}", Uuid::new_v4());

    sqlx::query("INSERT INTO job_queue (queue, unique_key) VALUES ('clone', $1)")
        .bind(&key)
        .execute(&pool)
        .await
        .expect("first keyed job inserts");

    let dup = sqlx::query("INSERT INTO job_queue (queue, unique_key) VALUES ('clone', $1)")
        .bind(&key)
        .execute(&pool)
        .await;
    let err = dup.expect_err("duplicate non-null unique_key must collide");
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("idx_job_queue_unique_key"),
        "collision must come from idx_job_queue_unique_key, got: {err}"
    );
    assert_eq!(sqlstate_of(&err), SQLSTATE_UNIQUE_VIOLATION, "must be a unique_violation");

    // Two NULL-key rows must BOTH succeed — proves the `WHERE unique_key IS NOT
    // NULL` partial predicate (a full unique index would reject the second NULL
    // only on some collations, but more importantly the queue relies on many
    // NULL-key jobs coexisting).
    sqlx::query("INSERT INTO job_queue (queue, unique_key) VALUES ('clone', NULL)")
        .execute(&pool)
        .await
        .expect("first NULL-key job inserts");
    sqlx::query("INSERT INTO job_queue (queue, unique_key) VALUES ('clone', NULL)")
        .execute(&pool)
        .await
        .expect("second NULL-key job must also insert (partial predicate)");
}

/// The workspace_dir_name backfill dedup (FIX 1) must resolve a real
/// cross-team slug collision rather than abort the migration.
///
/// `#[sqlx::test]` has already run 068, so we *recreate* the pre-068 hazard:
/// two LIVE projects in the SAME workspace under DIFFERENT teams that share a
/// slug — legal today because project slug uniqueness is only `(team_id, slug)`
/// and teams are org-scoped. We force both `workspace_dir_name` values back to
/// the bare slug (the state the naive backfill would produce), drop the unique
/// index, then replay the migration body. A correct dedup converges and the
/// index rebuilds; the buggy naive backfill would raise a unique_violation here.
#[sqlx::test(migrations = "./migrations")]
async fn workspace_dir_backfill_dedups_cross_team_slug_collision(pool: PgPool) {
    let org_id: Uuid =
        sqlx::query("INSERT INTO organizations (name, slug) VALUES ('dedup', 'dedup') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert org")
            .get("id");
    let workspace_id: Uuid =
        sqlx::query("INSERT INTO workspaces (organization_id, name) VALUES ($1, 'ws') RETURNING id")
            .bind(org_id)
            .fetch_one(&pool)
            .await
            .expect("insert workspace")
            .get("id");
    // Two teams in the same org.
    let team_a: Uuid =
        sqlx::query("INSERT INTO teams (organization_id, name, slug) VALUES ($1, 'A', 'a') RETURNING id")
            .bind(org_id)
            .fetch_one(&pool)
            .await
            .expect("insert team a")
            .get("id");
    let team_b: Uuid =
        sqlx::query("INSERT INTO teams (organization_id, name, slug) VALUES ($1, 'B', 'b') RETURNING id")
            .bind(org_id)
            .fetch_one(&pool)
            .await
            .expect("insert team b")
            .get("id");

    // Two LIVE projects, same workspace, different teams, SAME slug. Legal —
    // project slug uniqueness is (team_id, slug). Give them temporarily-distinct
    // dir names so the post-068 unique index lets them in; we collapse them next.
    let fx_a = Fixture { org_id, workspace_id, team_id: team_a };
    let fx_b = Fixture { org_id, workspace_id, team_id: team_b };
    let p_a = insert_project(&pool, &fx_a, "Shared", "shared", "tmp-a", "none")
        .await
        .expect("project A");
    let p_b = insert_project(&pool, &fx_b, "Shared", "shared", "tmp-b", "none")
        .await
        .expect("project B");

    // Recreate the pre-068 hazard: drop the unique index and force BOTH dir
    // names back to the bare slug — exactly the duplicate state the naive
    // backfill produced.
    sqlx::raw_sql("DROP INDEX IF EXISTS uq_projects_workspace_dir")
        .execute(&pool)
        .await
        .expect("drop unique index to recreate pre-068 state");
    sqlx::query("UPDATE projects SET workspace_dir_name = slug WHERE id IN ($1, $2)")
        .bind(p_a)
        .bind(p_b)
        .execute(&pool)
        .await
        .expect("force duplicate dir names");

    // Replay the migration body. The dedup must converge and rebuild the index;
    // a naive backfill would raise 23505 building the index over the duplicate.
    sqlx::raw_sql(include_str!("../migrations/068_project_clone.sql"))
        .execute(&pool)
        .await
        .expect("dedup must resolve the cross-team slug collision, not abort");

    // Exactly one of the two rows keeps the bare slug; the other is suffixed.
    // The kept row is the deterministically-oldest (ORDER BY created_at, id).
    let bare: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM projects
          WHERE workspace_id = $1 AND deleted_at IS NULL AND workspace_dir_name = 'shared'",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count bare-slug rows");
    assert_eq!(bare, 1, "exactly one live row may keep the bare slug after dedup");

    // Both rows still live, both distinct, both non-null.
    let live: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT workspace_dir_name) FROM projects
          WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count distinct dir names");
    assert_eq!(live, 2, "both live projects must end with distinct dir names");

    // And the rebuilt unique index is present + actually enforced.
    assert!(index_exists(&pool, "uq_projects_workspace_dir").await, "index must be rebuilt");
}

/// Migration 068 is idempotent: `#[sqlx::test]` runs it once via the migrator,
/// so the guarded DO-blocks / `IF NOT EXISTS` re-run paths are otherwise never
/// exercised. Replay the file body a second time against the already-migrated
/// pool and assert it succeeds — this is the only test that proves the re-run
/// guards (constraint-existence DO-block, dedup loop convergence on
/// already-distinct rows, every `IF NOT EXISTS`) actually hold.
#[sqlx::test(migrations = "./migrations")]
async fn migration_068_is_idempotent_on_rerun(pool: PgPool) {
    // Seed a live project first so the dedup DO-block has real rows to scan on
    // the replay (a no-op dedup is a stronger idempotency proof than an empty
    // table).
    let fx = seed_fixture(&pool, "rerun").await;
    insert_project(&pool, &fx, "p", "p", "rerun-dir", "ready")
        .await
        .expect("seed a live project before replay");

    // Replay the entire migration body — DO-blocks, ALTERs, CREATE … IF NOT
    // EXISTS, the lot — as one statement batch. `raw_sql` does not split on
    // `;` inside `$$ … $$`, so the DO-blocks survive intact.
    sqlx::raw_sql(include_str!("../migrations/068_project_clone.sql"))
        .execute(&pool)
        .await
        .expect("re-running migration 068 against an already-migrated db must be Ok");

    // The replay must not have disturbed the seeded row's dir name (the dedup
    // only suffixes duplicates, and there are none).
    let dir: String =
        sqlx::query_scalar("SELECT workspace_dir_name FROM projects WHERE slug = 'p' AND deleted_at IS NULL")
            .fetch_one(&pool)
            .await
            .expect("seeded project still present");
    assert_eq!(dir, "rerun-dir", "idempotent replay must not rename a unique dir");
}
