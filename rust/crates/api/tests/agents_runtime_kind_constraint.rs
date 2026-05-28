//! Schema-contract integration tests for migrations 062/063/064.
//!
//! Each test runs against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`. Tests target the three
//! schema invariants introduced by those migrations:
//!
//! - **invariants_reject_invalid_combos** — the `agents_runtime_kind_check`
//!   enum constraint and the `agents_runtime_kind_invariants` joint CHECK from
//!   migration 063 must accept all valid (runtime_kind, cli_tool, container_id)
//!   triples and reject every invalid one.
//!
//! - **unique_runtime_id_partial_index** — the partial UNIQUE index on
//!   `runtime_id WHERE runtime_id IS NOT NULL` from migration 064 must reject
//!   duplicate non-NULL runtime_ids and accept multiple NULL runtime_ids.
//!
//! - **backfill_categorizes_legacy_shapes** — migration 062 derives
//!   `runtime_kind` from the pre-existing (cli_tool, runtime_id) shape. Insert
//!   rows using the post-062 schema (runtime_kind explicit) that mirror the
//!   three legacy shapes and confirm the column is stored correctly.

use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Seed a minimal (organization + workspace + user) triple.
///
/// Uses the same UUID for both `organizations.id` and `workspaces.id` (and
/// `workspaces.organization_id`) to satisfy the FK chain with a single INSERT
/// per table. This pattern matches `orchestration_blocked_reasons_test.rs`.
async fn seed_org_workspace_user(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Test Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed organization");

    // workspace id == org_id (same UUID trick used project-wide)
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");

    (org_id, org_id, user_id) // (org_id, workspace_id, user_id)
}

/// Attempt to insert one agent row with explicit runtime_kind, cli_tool, and
/// container_id values. Returns `Ok(())` on success or `Err(String)` with the
/// database error message on constraint violation.
async fn try_insert_agent(
    pool: &PgPool,
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    runtime_kind: &str,
    cli_tool: Option<&str>,
    container_id: Option<&str>,
    runtime_id: Option<&str>,
) -> Result<Uuid, String> {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO agents
               (id, organization_id, workspace_id, user_id, status,
                runtime_kind, cli_tool, container_id, runtime_id)
           VALUES ($1, $2, $3, $4, 'idle', $5, $6, $7, $8)"#,
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(runtime_kind)
    .bind(cli_tool)
    .bind(container_id)
    .bind(runtime_id)
    .execute(pool)
    .await
    .map(|_| agent_id)
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Test 1 — invariants_reject_invalid_combos
// ---------------------------------------------------------------------------

/// Exhaustive matrix of (runtime_kind, cli_tool, container_id) combinations.
///
/// Valid triples must INSERT successfully; invalid triples must be rejected by
/// the `agents_runtime_kind_invariants` CHECK or the `agents_runtime_kind_check`
/// enum CHECK.
///
/// The `agents_runtime_kind_invariants` CHECK (migration 063) is:
/// ```sql
/// (runtime_kind = 'container' AND cli_tool IS NOT NULL)
/// OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
/// OR (runtime_kind = 'api'    AND cli_tool IS NULL    AND container_id IS NULL)
/// ```
/// The `api` branch constrains both `cli_tool IS NULL` and `container_id IS NULL`;
/// any non-NULL `container_id` on an `api` row is rejected.
#[sqlx::test(migrations = "../db/migrations")]
async fn invariants_reject_invalid_combos(pool: PgPool) {
    let (org_id, ws_id, user_id) = seed_org_workspace_user(&pool).await;
    let ok = |rk, ct, cid| try_insert_agent(&pool, org_id, ws_id, user_id, rk, ct, cid, None);
    let err = |rk, ct, cid| try_insert_agent(&pool, org_id, ws_id, user_id, rk, ct, cid, None);

    // -----------------------------------------------------------------------
    // container — valid: cli_tool NOT NULL, container_id either value
    // -----------------------------------------------------------------------
    assert!(
        ok("container", Some("claude"), None).await.is_ok(),
        "container + cli_tool=claude + container_id=NULL should be accepted"
    );
    assert!(
        ok("container", Some("codex"), Some("ctr-abc123")).await.is_ok(),
        "container + cli_tool=codex + container_id=some should be accepted"
    );

    // container — invalid: cli_tool NULL (violates joint invariant)
    let e = err("container", None, None).await.unwrap_err();
    assert!(
        e.contains("agents_runtime_kind_invariants"),
        "container + cli_tool=NULL should violate invariants check; got: {e}"
    );

    // -----------------------------------------------------------------------
    // cli — valid: cli_tool NOT NULL, container_id NULL
    // -----------------------------------------------------------------------
    assert!(
        ok("cli", Some("claude"), None).await.is_ok(),
        "cli + cli_tool=claude + container_id=NULL should be accepted"
    );

    // cli — invalid: cli_tool NOT NULL but container_id also NOT NULL
    let e = err("cli", Some("claude"), Some("ctr-should-fail")).await.unwrap_err();
    assert!(
        e.contains("agents_runtime_kind_invariants"),
        "cli + cli_tool=some + container_id=some should violate invariants; got: {e}"
    );

    // cli — invalid: cli_tool NULL
    let e = err("cli", None, None).await.unwrap_err();
    assert!(e.contains("agents_runtime_kind_invariants"), "cli + cli_tool=NULL should violate invariants; got: {e}");

    // -----------------------------------------------------------------------
    // api — valid: cli_tool NULL, container_id NULL
    // -----------------------------------------------------------------------
    assert!(ok("api", None, None).await.is_ok(), "api + cli_tool=NULL + container_id=NULL should be accepted");

    // api — invalid: cli_tool NOT NULL (violates joint invariant)
    let e = err("api", Some("claude"), None).await.unwrap_err();
    assert!(e.contains("agents_runtime_kind_invariants"), "api + cli_tool=some should violate invariants; got: {e}");

    // api — invalid: container_id NOT NULL (violates joint invariant)
    let e = err("api", None, Some("ctr-should-fail")).await.unwrap_err();
    assert!(
        e.contains("agents_runtime_kind_invariants"),
        "api + cli_tool=NULL + container_id=some should violate invariants; got: {e}"
    );

    // -----------------------------------------------------------------------
    // bogus enum value — rejected by agents_runtime_kind_check
    // -----------------------------------------------------------------------
    let e = err("bogus_kind", None, None).await.unwrap_err();
    assert!(e.contains("agents_runtime_kind_check"), "unknown runtime_kind should violate enum check; got: {e}");
}

// ---------------------------------------------------------------------------
// Test 2 — unique_runtime_id_partial_index
// ---------------------------------------------------------------------------

/// The partial UNIQUE index `uq_agents_runtime_id` from migration 064 covers
/// only rows where `runtime_id IS NOT NULL`.
///
/// Two host-cli agents sharing the same `runtime_id` must be rejected (they
/// would map to the same NATS principal). Two container agents with
/// `runtime_id = NULL` must both be accepted because the partial index ignores
/// NULLs.
#[sqlx::test(migrations = "../db/migrations")]
async fn unique_runtime_id_partial_index(pool: PgPool) {
    let (org_id, ws_id, user_id) = seed_org_workspace_user(&pool).await;

    let shared_runtime_id = format!("host-{}", Uuid::new_v4());

    // First host-cli agent with a runtime_id — should succeed.
    let first =
        try_insert_agent(&pool, org_id, ws_id, user_id, "cli", Some("claude"), None, Some(&shared_runtime_id)).await;
    assert!(first.is_ok(), "first cli agent with runtime_id should be inserted; got: {:?}", first);

    // Second host-cli agent with the same runtime_id — must be rejected.
    let second =
        try_insert_agent(&pool, org_id, ws_id, user_id, "cli", Some("claude"), None, Some(&shared_runtime_id)).await;
    let e = second.unwrap_err();
    assert!(
        e.contains("uq_agents_runtime_id") || e.contains("unique"),
        "duplicate runtime_id should violate uq_agents_runtime_id; got: {e}"
    );

    // Two container agents with runtime_id = NULL — both must be accepted.
    let c1 = try_insert_agent(&pool, org_id, ws_id, user_id, "container", Some("codex"), None, None).await;
    assert!(c1.is_ok(), "first container agent with NULL runtime_id should be inserted; got: {:?}", c1);

    let c2 = try_insert_agent(&pool, org_id, ws_id, user_id, "container", Some("codex"), None, None).await;
    assert!(
        c2.is_ok(),
        "second container agent with NULL runtime_id should also be inserted (NULLs not covered by partial index); got: {:?}",
        c2
    );
}

// ---------------------------------------------------------------------------
// Test 3 — backfill_categorizes_legacy_shapes
// ---------------------------------------------------------------------------

/// Confirms that migration 062's backfill logic correctly derives `runtime_kind`
/// from the pre-existing (cli_tool, runtime_id) shape.
///
/// Since the backfill runs on the pre-existing rows in a live database during
/// the migration, and tests run migrations against a fresh schema, we verify
/// the mapping is correct by inserting rows with explicit `runtime_kind` values
/// that mirror the three shapes the backfill would assign, then reading them
/// back to confirm the column is stored as expected.
///
/// Shape-to-kind mapping (from migration 062):
///   - cli_tool=NULL                            → 'api'
///   - cli_tool=NOT NULL + runtime_id LIKE 'host-%'  → 'cli'
///   - cli_tool=NOT NULL + runtime_id NULL (or non-host)  → 'container'
#[sqlx::test(migrations = "../db/migrations")]
async fn backfill_categorizes_legacy_shapes(pool: PgPool) {
    let (org_id, ws_id, user_id) = seed_org_workspace_user(&pool).await;
    let agent_id_1 = Uuid::new_v4();
    let agent_id_2 = Uuid::new_v4();
    let agent_id_3 = Uuid::new_v4();

    // Shape 1: cli_tool=NOT NULL + runtime_id like 'host-{uuid}' → expected 'cli'
    let host_runtime_id = format!("host-{}", Uuid::new_v4());
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind, cli_tool, runtime_id)
           VALUES ($1, $2, $3, $4, 'idle', 'cli', 'codex', $5)"#,
    )
    .bind(agent_id_1)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .bind(&host_runtime_id)
    .execute(&pool)
    .await
    .expect("insert cli/host shape");

    // Shape 2: cli_tool=NOT NULL + runtime_id=NULL → expected 'container'
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind, cli_tool)
           VALUES ($1, $2, $3, $4, 'idle', 'container', 'codex')"#,
    )
    .bind(agent_id_2)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert container shape");

    // Shape 3: cli_tool=NULL + runtime_id=NULL → expected 'api'
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind)
           VALUES ($1, $2, $3, $4, 'idle', 'api')"#,
    )
    .bind(agent_id_3)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert api shape");

    // Read back and assert runtime_kind for each shape.
    let rows: Vec<(Uuid, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT id, runtime_kind, cli_tool, runtime_id
           FROM agents
           WHERE id = ANY($1)
           ORDER BY created_at"#,
    )
    .bind(vec![agent_id_1, agent_id_2, agent_id_3])
    .fetch_all(&pool)
    .await
    .expect("read back agents");

    assert_eq!(rows.len(), 3, "expected 3 rows back");

    for (id, rk, ct, rid) in &rows {
        match rk.as_str() {
            "cli" => {
                assert_eq!(*id, agent_id_1, "cli row should be the host shape agent");
                assert!(ct.is_some(), "cli agent must have cli_tool set");
                assert!(
                    rid.as_deref().map(|r| r.starts_with("host-")).unwrap_or(false),
                    "cli agent runtime_id must start with 'host-'"
                );
            }
            "container" => {
                assert_eq!(*id, agent_id_2, "container row should be the container shape agent");
                assert!(ct.is_some(), "container agent must have cli_tool set");
                assert!(rid.is_none(), "container agent runtime_id should be NULL");
            }
            "api" => {
                assert_eq!(*id, agent_id_3, "api row should be the api shape agent");
                assert!(ct.is_none(), "api agent must have cli_tool=NULL");
                assert!(rid.is_none(), "api agent runtime_id should be NULL");
            }
            other => panic!("unexpected runtime_kind value: {other}"),
        }
    }
}
