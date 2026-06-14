//! Schema-contract test for migration 068 (project git-clone foundation, M0).
//!
//! Asserts a fresh database, migrated from `./migrations`, carries the columns,
//! table, and indexes that the later clone milestones (and the `ProjectClone-
//! Attempt` / `Project` entity structs) depend on. If a later migration drops or
//! renames any of these, this fails loudly instead of surfacing as a runtime
//! `FromRow` error.
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

#[sqlx::test(migrations = "./migrations")]
async fn migration_068_lands_project_clone_schema(pool: PgPool) {
    // projects: new columns.
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

    // project_clone_attempts table + a representative set of its columns
    // (mirroring the ProjectCloneAttempt entity).
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

    // The three new indexes.
    for index in ["uq_projects_workspace_dir", "uq_project_clone_attempt", "idx_project_clone_status"] {
        assert!(index_exists(&pool, index).await, "{index} index missing");
    }

    // The job_queue unique-key index the queue's ON CONFLICT path assumes.
    assert!(
        index_exists(&pool, "idx_job_queue_unique_key").await,
        "idx_job_queue_unique_key index missing"
    );

    // The clone_status enum CHECK constraint must reject out-of-domain values.
    // (Insert needs a real workspace + project; build the minimal graph.)
    let org_id: uuid::Uuid =
        sqlx::query("INSERT INTO organizations (name, slug) VALUES ('clone-test', 'clone-test') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert org")
            .get("id");
    let ws_id: uuid::Uuid = sqlx::query(
        "INSERT INTO workspaces (organization_id, name) VALUES ($1, 'ws') RETURNING id",
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("insert workspace")
    .get("id");

    let bad = sqlx::query(
        "INSERT INTO projects (organization_id, workspace_id, name, slug, workspace_dir_name, clone_status)
         VALUES ($1, $2, 'p', 'p', 'p', 'bogus')",
    )
    .bind(org_id)
    .bind(ws_id)
    .execute(&pool)
    .await;
    assert!(bad.is_err(), "projects_clone_status_check must reject an out-of-domain clone_status");
}
