//! Contract test for workflow re-run node reset (#892 F041).
//!
//! `PgWorkflowStore::reset_nodes` must clear every node's execution state back to
//! `pending` for the org-scoped workflow, and must not touch nodes when called
//! with a different org.

use sqlx::PgPool;

use agentforge_orchestrator::workflow::{PgWorkflowStore, Store};

#[sqlx::test(migrations = "./migrations")]
async fn reset_nodes_clears_execution_state_and_is_org_scoped(pool: PgPool) {
    let org = "org-rerun";

    let participant: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'rerun-user', 'casdoor-rerun', $1)
         RETURNING id::text",
    )
    .bind(org)
    .fetch_one(&pool)
    .await
    .expect("seed participant");

    let workflow_id: String = sqlx::query_scalar(
        "INSERT INTO workflows (name, status, org_id, created_by)
         VALUES ('rerun-wf', 'failed', $1, CAST($2 AS uuid))
         RETURNING id::text",
    )
    .bind(org)
    .bind(&participant)
    .fetch_one(&pool)
    .await
    .expect("seed workflow");

    // A completed node with execution state populated (the prior run).
    sqlx::query(
        "INSERT INTO workflow_nodes (workflow_id, name, type, position, status, started_at, completed_at, error, output)
         VALUES (CAST($1 AS uuid), 'n1', 'agent_task', 0, 'completed', NOW(), NOW(), 'boom', '{\"x\":1}'::jsonb)",
    )
    .bind(&workflow_id)
    .execute(&pool)
    .await
    .expect("seed node");

    let store = PgWorkflowStore::new(pool.clone());

    // Wrong-org reset must be a no-op (org scoping).
    store.reset_nodes(&workflow_id, "org-other").await.expect("wrong-org reset returns ok");
    let untouched: String =
        sqlx::query_scalar("SELECT status FROM workflow_nodes WHERE workflow_id = CAST($1 AS uuid)")
            .bind(&workflow_id)
            .fetch_one(&pool)
            .await
            .expect("fetch node");
    assert_eq!(untouched, "completed", "a reset for a different org must not touch the nodes");

    // Correct-org reset clears all execution state.
    store.reset_nodes(&workflow_id, org).await.expect("reset returns ok");
    let status: String = sqlx::query_scalar("SELECT status FROM workflow_nodes WHERE workflow_id = CAST($1 AS uuid)")
        .bind(&workflow_id)
        .fetch_one(&pool)
        .await
        .expect("fetch node");
    assert_eq!(status, "pending", "node status must reset to pending");

    let cleared: bool = sqlx::query_scalar(
        "SELECT started_at IS NULL AND completed_at IS NULL AND error IS NULL AND output IS NULL
         FROM workflow_nodes WHERE workflow_id = CAST($1 AS uuid)",
    )
    .bind(&workflow_id)
    .fetch_one(&pool)
    .await
    .expect("fetch node nullability");
    assert!(cleared, "started_at/completed_at/error/output must all be cleared on reset");
}
