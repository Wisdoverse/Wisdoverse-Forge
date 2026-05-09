//! Participant liveness worker for orchestration agents.
//!
//! Sidecars publish lightweight heartbeats to `sidecar.<agent_id>.heartbeat`.
//! This worker bridges that runtime signal into the authoritative
//! `participants` table by resolving `agent_id -> organization_id` from the
//! `agents` table, upserting the participant, marking stale participants
//! offline, failing expired `working` leases closed, and draining queued work
//! onto available participants. Heartbeats are intentionally non-durable;
//! liveness is recovered by the next heartbeat plus the stale sweeper.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use agentforge_core::orchestration_protocol::{DEFAULT_ASSIGNMENT_LEASE_SECS, TaskAssignment};
use agentforge_db::entities::{OrchestrationTask, Participant};
use anyhow::{Context, Result, anyhow};
use async_nats::Client;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::orchestration_realtime::{
    publish_broadcast, publish_task_update, realtime_projector_enabled, task_instruction,
};

pub const HEARTBEAT_SUBJECT_PREFIX: &str = "sidecar";
pub const HEARTBEAT_SUBJECT_SUFFIX: &str = "heartbeat";
pub const HEARTBEAT_SUBJECT_WILDCARD: &str = "sidecar.*.heartbeat";
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(90);
pub const DEFAULT_STALE_SWEEP_INTERVAL: Duration = Duration::from_secs(30);
pub const LEASE_FAILURE_CODE: &str = "agent_lost";

/// SQL used by heartbeat handling. Kept as a constant so tests can pin the
/// tenant-safe agent lookup and the "busy if a working task exists" rule.
pub(crate) const UPSERT_PARTICIPANT_SQL: &str = r#"WITH agent_row AS (
            SELECT id,
                   organization_id,
                   COALESCE(NULLIF(name, ''), 'agent-' || LEFT(id::text, 8)) AS db_name
              FROM agents
             WHERE id = $1
        ),
        working_task AS (
            SELECT task.assigned_agent_id AS agent_id
              FROM orchestration_tasks task
              JOIN agent_row agent
                ON task.organization_id = agent.organization_id
               AND task.assigned_agent_id = agent.id
             WHERE task.status = 'working'
             LIMIT 1
        )
        INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
        SELECT agent.organization_id,
               agent.id,
               agent.db_name,
               $2,
               CASE WHEN working_task.agent_id IS NULL THEN 'available' ELSE 'busy' END,
               NOW()
          FROM agent_row agent
          LEFT JOIN working_task ON working_task.agent_id = agent.id
        ON CONFLICT (organization_id, agent_id) DO UPDATE
            SET name = EXCLUDED.name,
                capabilities = EXCLUDED.capabilities,
                status = CASE
                    WHEN EXISTS (
                        SELECT 1
                          FROM orchestration_tasks task
                         WHERE task.organization_id = participants.organization_id
                           AND task.assigned_agent_id = participants.agent_id
                           AND task.status = 'working'
                    ) THEN 'busy'
                    ELSE 'available'
                END,
                last_heartbeat_at = NOW()
        RETURNING *"#;

pub(crate) const MARK_STALE_OFFLINE_SQL: &str = r#"UPDATE participants
           SET status = 'offline'
         WHERE status <> 'offline'
           AND (
               last_heartbeat_at IS NULL
               OR last_heartbeat_at < NOW() - ($1::text || ' seconds')::interval
           )
        RETURNING *"#;

pub(crate) const LOCK_PARTICIPANT_SQL: &str = r#"SELECT *
          FROM participants
         WHERE agent_id = $1
         FOR UPDATE"#;

pub(crate) const NEXT_DISPATCHABLE_SQL: &str = r#"SELECT *
          FROM orchestration_tasks
         WHERE organization_id = $1
           AND status IN ('queued', 'blocked')
           AND (blocked_reason IS NULL OR blocked_reason = 'waiting_agent')
           AND assigned_agent_id IS NULL
         ORDER BY
           CASE priority
             WHEN 'urgent' THEN 0
             WHEN 'high'   THEN 1
             WHEN 'normal' THEN 2
             WHEN 'low'    THEN 3
             ELSE 4
           END,
           created_at ASC
         FOR UPDATE SKIP LOCKED
         LIMIT 1"#;

pub(crate) const AVAILABLE_PARTICIPANTS_SQL: &str = r#"SELECT agent_id
          FROM participants
         WHERE status = 'available'
         ORDER BY last_heartbeat_at DESC NULLS LAST"#;

pub(crate) const CLAIM_TASK_SQL: &str = r#"UPDATE orchestration_tasks
           SET assigned_agent_id = $3,
               status = 'working',
               blocked_reason = NULL,
               blocked_metadata = NULL,
               started_at = COALESCE(started_at, NOW()),
               attempt = attempt + 1,
               lease_expires_at = NOW() + ($5::text || ' seconds')::interval,
               last_assignment_id = $4,
               failure_code = NULL,
               retryable = FALSE,
               updated_at = NOW()
         WHERE id = $1
           AND organization_id = $2
         RETURNING *"#;

pub(crate) const INSERT_TASK_RUN_SQL: &str = r#"INSERT INTO task_runs
           (id, organization_id, workspace_id, orchestration_task_id, agent_id,
            idempotency_key, status, started_at, capability_profile)
        SELECT $1, $2, agent.workspace_id, $3, $4, $5, 'working',
               COALESCE($6, NOW()), jsonb_build_object('capabilities', $7::text[])
          FROM agents agent
         WHERE agent.id = $4
           AND agent.organization_id = $2
        ON CONFLICT (orchestration_task_id, idempotency_key) DO UPDATE
           SET status = CASE
                   WHEN task_runs.finished_at IS NULL THEN EXCLUDED.status
                   ELSE task_runs.status
               END,
               updated_at = NOW()"#;

pub(crate) const CLOSE_EXPIRED_TASK_RUNS_SQL: &str = r#"UPDATE task_runs
           SET status = 'failed',
               finished_at = COALESCE(finished_at, NOW()),
               updated_at = NOW()
         WHERE orchestration_task_id = ANY($1)
           AND finished_at IS NULL"#;

pub(crate) const SET_PARTICIPANT_STATUS_SQL: &str = r#"UPDATE participants
           SET status = $2,
               last_heartbeat_at = NOW()
         WHERE id = $1
         RETURNING *"#;

pub(crate) const EXPIRE_WORKING_LEASES_SQL: &str = r#"UPDATE orchestration_tasks
           SET status = 'failed',
               error = jsonb_build_object(
                   'message', 'assigned agent lease expired or lost its busy participant before the task completed',
                   'code', 'agent_lost'
               ),
               failure_code = 'agent_lost',
               retryable = FALSE,
               lease_expires_at = NULL,
               completed_at = NOW(),
               updated_at = NOW()
         WHERE status = 'working'
           AND (
               (lease_expires_at IS NOT NULL AND lease_expires_at < NOW())
               OR (
                   lease_expires_at IS NULL
                   AND (
                       assigned_agent_id IS NULL
                       OR NOT EXISTS (
                           SELECT 1
                             FROM participants participant
                            WHERE participant.organization_id = orchestration_tasks.organization_id
                              AND participant.agent_id = orchestration_tasks.assigned_agent_id
                              AND participant.status = 'busy'
                       )
                   )
               )
           )
        RETURNING *"#;

pub(crate) const RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL: &str = r#"UPDATE participants
           SET status = CASE
               WHEN EXISTS (
                   SELECT 1
                     FROM orchestration_tasks task
                    WHERE task.organization_id = participants.organization_id
                      AND task.assigned_agent_id = participants.agent_id
                      AND task.status = 'working'
               ) THEN 'busy'
               WHEN last_heartbeat_at IS NULL
                 OR last_heartbeat_at < NOW() - ($3::text || ' seconds')::interval
               THEN 'offline'
               ELSE 'available'
           END
         WHERE organization_id = $1
           AND agent_id = $2
        RETURNING *"#;

#[derive(Debug, Deserialize)]
struct HeartbeatPayload {
    #[serde(default)]
    agent_id: Option<Uuid>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    cli_tool: Option<String>,
}

impl HeartbeatPayload {
    fn normalized_capabilities(&self) -> Vec<String> {
        if !self.capabilities.is_empty() {
            return self.capabilities.clone();
        }
        self.cli_tool.clone().into_iter().collect()
    }
}

/// Consumes sidecar heartbeats and periodically marks stale participants offline.
pub struct ParticipantLivenessWorker {
    client: Client,
    pool: PgPool,
    stale_after: Duration,
    sweep_interval: Duration,
}

impl ParticipantLivenessWorker {
    pub fn new(client: Client, pool: PgPool) -> Self {
        Self { client, pool, stale_after: DEFAULT_STALE_AFTER, sweep_interval: DEFAULT_STALE_SWEEP_INTERVAL }
    }

    pub fn with_stale_after(mut self, stale_after: Duration) -> Self {
        self.stale_after = stale_after;
        self
    }

    pub fn with_sweep_interval(mut self, sweep_interval: Duration) -> Self {
        self.sweep_interval = sweep_interval;
        self
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut subscriber = match self
            .client
            .queue_subscribe(HEARTBEAT_SUBJECT_WILDCARD.to_string(), "participant-liveness-workers".to_string())
            .await
        {
            Ok(sub) => sub,
            Err(err) => {
                tracing::error!(error = %err, subject = HEARTBEAT_SUBJECT_WILDCARD, "Failed to subscribe to sidecar heartbeats");
                return;
            }
        };

        let mut ticker = tokio::time::interval(self.sweep_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        tracing::info!(
            subject = HEARTBEAT_SUBJECT_WILDCARD,
            queue = "participant-liveness-workers",
            stale_after_secs = self.stale_after.as_secs(),
            sweep_interval_secs = self.sweep_interval.as_secs(),
            "Participant liveness worker listening"
        );

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Participant liveness worker shutting down");
                        break;
                    }
                }
                msg = subscriber.next() => {
                    match msg {
                        Some(nats_msg) => {
                            let subject = nats_msg.subject.to_string();
                            if let Err(err) = handle_heartbeat(&self.client, &self.pool, &subject, &nats_msg.payload).await {
                                tracing::warn!(error = %err, %subject, "Dropped sidecar heartbeat");
                            }
                        }
                        None => {
                            tracing::info!("Sidecar heartbeat subscription closed by server");
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    match mark_stale_offline(&self.client, &self.pool, self.stale_after).await {
                        Ok(0) => {}
                        Ok(n) => tracing::warn!(participants = n, "Marked stale orchestration participants offline"),
                        Err(err) => tracing::error!(error = ?err, "Participant stale sweep failed"),
                    }
                    match expire_working_leases(&self.pool, self.stale_after).await {
                        Ok(outcomes) if outcomes.is_empty() => {}
                        Ok(outcomes) => {
                            if let Err(err) = publish_expired_lease_updates(&self.client, &outcomes).await {
                                tracing::warn!(error = %err, "Failed to broadcast expired lease updates");
                            }
                            metrics::counter!("agentforge_orchestration_working_lease_expired_total")
                                .increment(outcomes.len() as u64);
                            tracing::warn!(tasks = outcomes.len(), "Expired working orchestration leases failed closed");
                        }
                        Err(err) => tracing::error!(error = ?err, "Working-lease sweep failed"),
                    }
                    match drain_available_participants(&self.client, &self.pool).await {
                        Ok(0) => {}
                        Ok(n) => tracing::info!(tasks = n, "Drained dispatchable tasks onto available participants"),
                        Err(err) => tracing::error!(error = ?err, "Available-participant drain failed"),
                    }
                }
            }
        }
    }
}

pub async fn handle_heartbeat(client: &Client, pool: &PgPool, subject: &str, payload: &[u8]) -> Result<()> {
    let subject_agent = parse_heartbeat_agent_id(subject).ok_or_else(|| anyhow!("bad heartbeat subject {subject}"))?;
    let payload: HeartbeatPayload = serde_json::from_slice(payload).with_context(|| "decode heartbeat payload")?;

    if let Some(payload_agent) = payload.agent_id
        && payload_agent != subject_agent
    {
        return Err(anyhow!("heartbeat subject agent {subject_agent} disagrees with payload agent {payload_agent}"));
    }

    let capabilities = payload.normalized_capabilities();
    let participant = sqlx::query_as::<_, Participant>(UPSERT_PARTICIPANT_SQL)
        .bind(subject_agent)
        .bind(capabilities)
        .fetch_optional(pool)
        .await?;

    let Some(participant) = participant else {
        return Err(anyhow!("no agent row found for heartbeat agent {subject_agent}"));
    };

    if let Err(err) = publish_participant_update(client, &participant, "participant.heartbeat").await {
        tracing::warn!(error = %err, participant_id = %participant.id, "Failed to broadcast participant heartbeat");
    }

    if participant.status == "available"
        && let Some((task, busy_participant)) = claim_next_task_for_participant(pool, subject_agent).await?
    {
        if let Err(err) = publish_participant_update(client, &busy_participant, "participant.claimed").await {
            tracing::warn!(error = %err, participant_id = %busy_participant.id, "Failed to broadcast participant claim");
        }
        if let Err(err) =
            publish_task_update(client, &task, Some(busy_participant.name.as_str()), "task.assigned").await
        {
            tracing::warn!(error = %err, task_id = %task.id, "Failed to broadcast task assignment update");
        }
    }

    Ok(())
}

pub async fn mark_stale_offline(client: &Client, pool: &PgPool, stale_after: Duration) -> Result<u64> {
    let participants = sqlx::query_as::<_, Participant>(MARK_STALE_OFFLINE_SQL)
        .bind(stale_after.as_secs().to_string())
        .fetch_all(pool)
        .await?;
    for participant in &participants {
        if let Err(err) = publish_participant_update(client, participant, "participant.offline").await {
            tracing::warn!(error = %err, participant_id = %participant.id, "Failed to broadcast participant offline update");
        }
    }
    Ok(participants.len() as u64)
}

#[derive(Debug, Clone)]
pub struct ExpiredLeaseOutcome {
    pub task: OrchestrationTask,
    pub participant: Option<Participant>,
}

pub async fn expire_working_leases(pool: &PgPool, stale_after: Duration) -> Result<Vec<ExpiredLeaseOutcome>> {
    let mut tx = pool.begin().await?;
    let expired_tasks = sqlx::query_as::<_, OrchestrationTask>(EXPIRE_WORKING_LEASES_SQL).fetch_all(&mut *tx).await?;
    if !expired_tasks.is_empty() {
        let task_ids: Vec<Uuid> = expired_tasks.iter().map(|task| task.id).collect();
        sqlx::query(CLOSE_EXPIRED_TASK_RUNS_SQL).bind(&task_ids).execute(&mut *tx).await?;
    }

    let mut participants = HashMap::new();
    let mut seen = HashSet::new();
    for task in &expired_tasks {
        let Some(agent_id) = task.assigned_agent_id.map(|id| id.as_uuid()) else {
            continue;
        };
        let key = (task.organization_id.as_uuid(), agent_id);
        if !seen.insert(key) {
            continue;
        }
        if let Some(participant) = sqlx::query_as::<_, Participant>(RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL)
            .bind(task.organization_id.as_uuid())
            .bind(agent_id)
            .bind(stale_after.as_secs().to_string())
            .fetch_optional(&mut *tx)
            .await?
        {
            participants.insert(key, participant);
        }
    }

    tx.commit().await?;

    Ok(expired_tasks
        .into_iter()
        .map(|task| {
            let participant = task
                .assigned_agent_id
                .and_then(|agent_id| participants.get(&(task.organization_id.as_uuid(), agent_id.as_uuid())).cloned());
            ExpiredLeaseOutcome { task, participant }
        })
        .collect())
}

pub fn parse_heartbeat_agent_id(subject: &str) -> Option<Uuid> {
    let mut parts = subject.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(HEARTBEAT_SUBJECT_PREFIX), Some(agent_id), Some(HEARTBEAT_SUBJECT_SUFFIX), None) => {
            Uuid::parse_str(agent_id).ok()
        }
        _ => None,
    }
}

async fn claim_next_task_for_participant(
    pool: &PgPool,
    agent_id: Uuid,
) -> Result<Option<(OrchestrationTask, Participant)>> {
    let mut tx = pool.begin().await?;

    let Some(participant) =
        sqlx::query_as::<_, Participant>(LOCK_PARTICIPANT_SQL).bind(agent_id).fetch_optional(&mut *tx).await?
    else {
        tx.commit().await?;
        return Ok(None);
    };

    if participant.status != "available" {
        tx.commit().await?;
        return Ok(None);
    }

    let Some(task) = sqlx::query_as::<_, OrchestrationTask>(NEXT_DISPATCHABLE_SQL)
        .bind(participant.organization_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };

    let delivery_id = Uuid::now_v7();
    let claimed_task = sqlx::query_as::<_, OrchestrationTask>(CLAIM_TASK_SQL)
        .bind(task.id)
        .bind(participant.organization_id.as_uuid())
        .bind(agent_id)
        .bind(delivery_id)
        .bind(DEFAULT_ASSIGNMENT_LEASE_SECS.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow!("failed to claim task {}", task.id))?;

    let delivery_id = claimed_task
        .last_assignment_id
        .ok_or_else(|| anyhow!("claimed task {} missing assignment id", claimed_task.id))?;
    sqlx::query(INSERT_TASK_RUN_SQL)
        .bind(Uuid::now_v7())
        .bind(participant.organization_id.as_uuid())
        .bind(claimed_task.id)
        .bind(agent_id)
        .bind(delivery_id.to_string())
        .bind(claimed_task.started_at)
        .bind(&participant.capabilities)
        .execute(&mut *tx)
        .await?;

    let busy_participant = sqlx::query_as::<_, Participant>(SET_PARTICIPANT_STATUS_SQL)
        .bind(participant.id)
        .bind("busy")
        .fetch_one(&mut *tx)
        .await?;

    let (task_text, message) = task_instruction(&claimed_task);
    let assignment = TaskAssignment {
        delivery_id: claimed_task.last_assignment_id,
        attempt: Some(claimed_task.attempt),
        lease_expires_at: claimed_task.lease_expires_at,
        task_id: claimed_task.id,
        agent_id,
        title: claimed_task.title.clone(),
        task: task_text,
        message,
        priority: claimed_task.priority.clone(),
        context_envelope: None,
    };
    crate::insert_assignment_outbox_in_tx(&mut tx, participant.organization_id.as_uuid(), claimed_task.id, &assignment)
        .await?;

    tx.commit().await?;
    Ok(Some((claimed_task, busy_participant)))
}

async fn drain_available_participants(client: &Client, pool: &PgPool) -> Result<u64> {
    let participants: Vec<(Uuid,)> = sqlx::query_as(AVAILABLE_PARTICIPANTS_SQL).fetch_all(pool).await?;
    let mut claimed = 0_u64;
    for (agent_id,) in participants {
        if let Some((task, busy_participant)) = claim_next_task_for_participant(pool, agent_id).await? {
            if let Err(err) = publish_participant_update(client, &busy_participant, "participant.claimed").await {
                tracing::warn!(error = %err, participant_id = %busy_participant.id, "Failed to broadcast participant claim");
            }
            if let Err(err) =
                publish_task_update(client, &task, Some(busy_participant.name.as_str()), "task.assigned").await
            {
                tracing::warn!(error = %err, task_id = %task.id, "Failed to broadcast task assignment update");
            }
            claimed += 1;
        }
    }
    Ok(claimed)
}

async fn publish_expired_lease_updates(client: &Client, outcomes: &[ExpiredLeaseOutcome]) -> Result<()> {
    for outcome in outcomes {
        let assigned_agent_name = outcome.participant.as_ref().map(|participant| participant.name.as_str());
        if let Err(err) = publish_task_update(client, &outcome.task, assigned_agent_name, "task.lease_expired").await {
            tracing::warn!(error = %err, task_id = %outcome.task.id, "Failed to broadcast expired task update");
        }
        if let Some(participant) = &outcome.participant {
            let action = match participant.status.as_str() {
                "offline" => "participant.offline",
                "busy" => "participant.busy",
                _ => "participant.available",
            };
            if let Err(err) = publish_participant_update(client, participant, action).await {
                tracing::warn!(
                    error = %err,
                    participant_id = %participant.id,
                    "Failed to broadcast participant lease-release update"
                );
            }
        }
    }
    Ok(())
}

async fn publish_participant_update(client: &Client, participant: &Participant, action: &str) -> Result<()> {
    if !realtime_projector_enabled() {
        tracing::debug!(%action, participant_id = %participant.id, "orchestration realtime projector disabled");
        return Ok(());
    }

    publish_broadcast(
        client,
        participant.organization_id,
        json!({
            "type": "orchestration:participant_update",
            "payload": {
                "action": action,
                "eventId": Uuid::now_v7(),
                "participant": {
                    "id": participant.id,
                    "agentId": participant.agent_id.as_uuid(),
                    "name": participant.name,
                    "status": participant_status_for_ws(&participant.status),
                }
            }
        }),
    )
    .await
}

fn participant_status_for_ws(status: &str) -> &str {
    match status {
        "busy" => "busy",
        "offline" => "offline",
        _ => "online",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_heartbeat_subject() {
        let id = Uuid::now_v7();
        assert_eq!(parse_heartbeat_agent_id(&format!("sidecar.{id}.heartbeat")), Some(id));
    }

    #[test]
    fn rejects_bad_heartbeat_subject_shapes() {
        assert_eq!(parse_heartbeat_agent_id("sidecar.heartbeat"), None);
        assert_eq!(parse_heartbeat_agent_id("sidecar.not-a-uuid.heartbeat"), None);
        assert_eq!(parse_heartbeat_agent_id("sidecar.00000000-0000-0000-0000-000000000000.heartbeat.extra"), None);
        assert_eq!(parse_heartbeat_agent_id("events.ingest.00000000-0000-0000-0000-000000000000"), None);
    }

    #[test]
    fn heartbeat_payload_falls_back_to_cli_tool_capability() {
        let payload: HeartbeatPayload =
            serde_json::from_value(serde_json::json!({"agent_id": Uuid::now_v7(), "cli_tool": "codex"})).unwrap();
        assert_eq!(payload.normalized_capabilities(), vec!["codex".to_string()]);
    }

    #[test]
    fn heartbeat_payload_preserves_explicit_capabilities() {
        let payload: HeartbeatPayload =
            serde_json::from_value(serde_json::json!({"capabilities": ["rust", "codex"], "cli_tool": "claude"}))
                .unwrap();
        assert_eq!(payload.normalized_capabilities(), vec!["rust".to_string(), "codex".to_string()]);
    }

    #[test]
    fn upsert_sql_resolves_tenant_from_agents_and_restores_busy_from_working_task() {
        assert!(UPSERT_PARTICIPANT_SQL.contains("FROM agents"));
        assert!(UPSERT_PARTICIPANT_SQL.contains("agent.organization_id"));
        assert!(UPSERT_PARTICIPANT_SQL.contains("agent.db_name"));
        assert!(UPSERT_PARTICIPANT_SQL.contains("task.status = 'working'"));
        assert!(UPSERT_PARTICIPANT_SQL.contains("ON CONFLICT (organization_id, agent_id) DO UPDATE"));
    }

    #[test]
    fn stale_sql_only_marks_non_offline_rows() {
        assert!(MARK_STALE_OFFLINE_SQL.contains("status <> 'offline'"));
        assert!(MARK_STALE_OFFLINE_SQL.contains("last_heartbeat_at < NOW()"));
    }

    #[test]
    fn expired_lease_sql_fails_working_tasks_closed() {
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("status = 'failed'"));
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("failure_code = 'agent_lost'"));
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("lease_expires_at < NOW()"));
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("lease_expires_at IS NULL"));
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("participant.status = 'busy'"));
        assert!(EXPIRE_WORKING_LEASES_SQL.contains("retryable = FALSE"));
    }

    #[test]
    fn participant_release_sql_restores_available_or_offline() {
        assert!(RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL.contains("task.status = 'working'"));
        assert!(RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL.contains("THEN 'offline'"));
        assert!(RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL.contains("ELSE 'available'"));
    }

    #[test]
    fn available_participants_sql_only_selects_available_rows() {
        assert!(AVAILABLE_PARTICIPANTS_SQL.contains("status = 'available'"));
        assert!(AVAILABLE_PARTICIPANTS_SQL.contains("ORDER BY last_heartbeat_at DESC"));
    }

    #[test]
    fn participant_status_maps_to_ws_contract() {
        assert_eq!(participant_status_for_ws("available"), "online");
        assert_eq!(participant_status_for_ws("busy"), "busy");
        assert_eq!(participant_status_for_ws("offline"), "offline");
        assert_eq!(participant_status_for_ws("unknown"), "online");
    }
}
