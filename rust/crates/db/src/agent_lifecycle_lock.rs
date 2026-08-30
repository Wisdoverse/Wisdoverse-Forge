use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Serialize task admission with destructive Agent container lifecycle work.
/// The transaction-scoped lock releases on commit, rollback, or connection
/// loss, so it cannot survive a crashed roll worker.
pub async fn lock_agent_lifecycle_in_tx(tx: &mut Transaction<'_, Postgres>, agent_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('agent-lifecycle:' || $1::text, 0))")
        .bind(agent_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Authoritative shared admission predicate for work that needs exclusive use
/// of an Agent. Callers must first hold [`lock_agent_lifecycle_in_tx`] for the
/// same `agent_id` and keep the transaction open through admission.
///
/// `agents.status` is deliberately not an owner: heartbeat mirrors can lag or
/// be overwritten. Ownership is durable and explicit instead: a live
/// interactive lease (browser terminal or MCP), a busy orchestration
/// participant, or a working orchestration task. `None` means the tenant-scoped
/// Agent row does not exist.
pub async fn agent_work_admission_is_idle_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    agent_id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT (agent.interactive_lease_expires_at IS NULL
                       OR agent.interactive_lease_expires_at <= NOW())
                  AND NOT EXISTS (
                        SELECT 1
                          FROM participants participant
                         WHERE participant.organization_id = agent.organization_id
                           AND participant.agent_id = agent.id
                           AND participant.status = 'busy'
                      )
                  AND NOT EXISTS (
                        SELECT 1
                          FROM orchestration_tasks task
                         WHERE task.organization_id = agent.organization_id
                           AND task.assigned_agent_id = agent.id
                           AND task.status = 'working'
                      )
             FROM agents agent
            WHERE agent.id = $1
              AND agent.organization_id = $2"#,
    )
    .bind(agent_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
}

/// Deployment-wide single-flight guard for one CLI image mutation. The updater,
/// local builder, pruner, and roll service share this exact key so a roll cannot
/// observe or apply a partially changed runtime alias across API replicas.
pub async fn try_lock_cli_image_roll_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tool: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended('cli-image-roll:' || $1, 0))")
        .bind(tool)
        .fetch_one(&mut **tx)
        .await
}
