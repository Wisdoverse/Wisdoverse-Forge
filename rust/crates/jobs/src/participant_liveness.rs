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

use agentforge_core::RuntimeKind;
use agentforge_core::orchestration_protocol::{
    DEFAULT_ASSIGNMENT_LEASE_SECS, TaskAssignment, container_generation_fingerprint,
};
use agentforge_core::orchestration_view::TaskInstruction;
use agentforge_core::ws_protocol::{
    OrchestrationParticipantBrief, OrchestrationParticipantUpdatePayload, ServerMessage,
};
use agentforge_db::entities::{OrchestrationTask, Participant};
use anyhow::{Context, Result, anyhow};
use async_nats::Client;
use futures::StreamExt;
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::orchestration_realtime::{publish_broadcast, publish_task_update, realtime_projector_enabled};
use crate::presence_store::{PresenceBackend, RedisRecord};

pub const HEARTBEAT_SUBJECT_PREFIX: &str = "sidecar";
pub const HEARTBEAT_SUBJECT_SUFFIX: &str = "heartbeat";
pub const HEARTBEAT_SUBJECT_WILDCARD: &str = "sidecar.*.heartbeat";
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(90);
pub const DEFAULT_STALE_SWEEP_INTERVAL: Duration = Duration::from_secs(30);
pub const LEASE_FAILURE_CODE: &str = "agent_lost";

/// Hot-path heartbeat write (ADR 0008). Heartbeats are an UPDATE-first /
/// INSERT-on-miss pair rather than a single upsert so the common case — a beat
/// for an already-registered participant — is a plain single-row UPDATE with no
/// correlated subquery.
///
/// A steady-state beat only restamps `last_heartbeat_at` (+ refreshes
/// name/capabilities). `busy`/`available` is maintained event-driven elsewhere
/// (claim -> busy, task result -> available, lease expiry -> recomputed), so it
/// is NOT recomputed per beat. The correlated subquery runs ONLY when the row is
/// currently `offline` — a resurrection, where a returning agent that still owns
/// a `working` task (leases are 900s, far longer than the 90s offline window)
/// must come back as `busy`, not `available`, to avoid double-assignment. `CASE`
/// short-circuits the subquery on the common (non-offline) path. `participants.*`
/// is the OLD row inside an `UPDATE` SET, which is the resurrection input.
pub(crate) const TOUCH_PARTICIPANT_SQL: &str = r#"UPDATE participants
            SET capabilities = $2,
                name = COALESCE(NULLIF(agent.name, ''), 'agent-' || LEFT(agent.id::text, 8)),
                last_heartbeat_at = NOW(),
                status = CASE
                    WHEN participants.status = 'offline' THEN
                        CASE
                            WHEN EXISTS (
                                SELECT 1
                                  FROM orchestration_tasks task
                                 WHERE task.organization_id = participants.organization_id
                                   AND task.assigned_agent_id = participants.agent_id
                                   AND task.status = 'working'
                            ) THEN 'busy'
                            ELSE 'available'
                        END
                    ELSE participants.status
                END
           FROM agents agent
          WHERE participants.agent_id = $1
            AND agent.id = $1
        RETURNING participants.*"#;

/// First-seen INSERT, reached only when `TOUCH_PARTICIPANT_SQL` matched no row
/// (a brand-new participant, or — defensively — a row that was hard-deleted out
/// from under a live task). Unlike the hot path this DOES compute the initial
/// `busy`/`available` from the agent's `working` task, so a first beat can never
/// leave a task-owning agent wrongly `available`. The cost is paid once per
/// participant lifetime, not per beat. `ON CONFLICT` collapses the
/// insert-vs-insert race (a concurrent first beat) into a heartbeat touch; the
/// row it conflicts with was just created with a correct status.
pub(crate) const INSERT_PARTICIPANT_SQL: &str = r#"INSERT INTO participants
            (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
        SELECT agent.organization_id,
               agent.id,
               COALESCE(NULLIF(agent.name, ''), 'agent-' || LEFT(agent.id::text, 8)),
               $2,
               CASE
                   WHEN EXISTS (
                       SELECT 1
                         FROM orchestration_tasks task
                        WHERE task.organization_id = agent.organization_id
                          AND task.assigned_agent_id = agent.id
                          AND task.status = 'working'
                   ) THEN 'busy'
                   ELSE 'available'
               END,
               NOW()
          FROM agents agent
         WHERE agent.id = $1
        ON CONFLICT (organization_id, agent_id) DO UPDATE
            SET last_heartbeat_at = NOW()
        RETURNING *"#;

/// Mirror participant liveness onto `agents.status` without stealing an active
/// interactive work lease. Hook/MCP owners choose their own bounded deadline;
/// an ordinary available heartbeat may preserve that owner, but never extends
/// it. This avoids turning one lost stop event into a permanent lease.
pub(crate) const UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL: &str = r#"UPDATE agents
           SET status = CASE
                   WHEN $3::agent_status = 'idle'
                    AND interactive_lease_expires_at > NOW()
                   THEN 'working'::agent_status
                   ELSE $3::agent_status
               END,
               interactive_lease_expires_at = CASE
                   WHEN $3::agent_status = 'idle'
                    AND interactive_lease_expires_at > NOW()
                   THEN interactive_lease_expires_at
                   WHEN $3::agent_status = 'idle' THEN NULL
                   ELSE interactive_lease_expires_at
               END,
               updated_at = NOW()
         WHERE id = $1
           AND organization_id = $2
           AND status IS DISTINCT FROM CASE
                   WHEN $3::agent_status = 'idle'
                    AND interactive_lease_expires_at > NOW()
                   THEN 'working'::agent_status
                   ELSE $3::agent_status
               END"#;

pub(crate) const MARK_STALE_OFFLINE_SQL: &str = r#"UPDATE participants
           SET status = 'offline'
         WHERE status <> 'offline'
           AND (
               last_heartbeat_at IS NULL
               OR last_heartbeat_at < NOW() - ($1::text || ' seconds')::interval
           )
        RETURNING *"#;

/// Phase 2 (ADR 0008): non-offline participants the Redis sweep must probe for
/// liveness (key existence). The Redis presence key, not `last_heartbeat_at`,
/// decides offline in Redis mode.
pub(crate) const NON_OFFLINE_PARTICIPANT_AGENTS_SQL: &str =
    r#"SELECT agent_id FROM participants WHERE status <> 'offline'"#;

/// Phase 2 (ADR 0008): mark the supplied agents offline (those whose Redis
/// presence key expired). Tenant-safe by construction — the agent ids come from
/// the participants table; the WHERE keeps it idempotent against concurrent
/// transitions.
pub(crate) const MARK_OFFLINE_BY_AGENT_IDS_SQL: &str = r#"UPDATE participants
           SET status = 'offline'
         WHERE status <> 'offline'
           AND agent_id = ANY($1)
        RETURNING *"#;

/// Candidate locks for the orphaned-busy reconcile. The first snapshot may be
/// stale after waiting for a concurrent release/claim, so it only chooses and
/// locks rows; the update below rechecks ownership in a fresh statement.
pub(crate) const LOCK_ORPHANED_BUSY_CANDIDATES_SQL: &str = r#"SELECT participant.id
          FROM participants participant
          JOIN agents agent
            ON agent.id = participant.agent_id
           AND agent.organization_id = participant.organization_id
         WHERE participant.status = 'busy'
           AND NOT EXISTS (
               SELECT 1
                 FROM orchestration_tasks task
                WHERE task.organization_id = participant.organization_id
                  AND task.assigned_agent_id = participant.agent_id
                  AND task.status = 'working'
           )
         ORDER BY participant.id
         FOR UPDATE OF participant, agent"#;

/// Recheck the locked candidates in a new READ COMMITTED snapshot before
/// releasing them. Claims use the same participant/agent locks, so ownership
/// cannot change between this check and commit.
pub(crate) const RECONCILE_ORPHANED_BUSY_SQL: &str = r#"UPDATE participants
           SET status = 'available'
         WHERE id = ANY($1)
           AND status = 'busy'
           AND NOT EXISTS (
               SELECT 1
                 FROM orchestration_tasks task
                WHERE task.organization_id = participants.organization_id
                  AND task.assigned_agent_id = participants.agent_id
                  AND task.status = 'working'
           )
        RETURNING *"#;

pub(crate) const LOCK_PARTICIPANT_SQL: &str = r#"SELECT participant.*
          FROM participants participant
          JOIN agents agent
            ON agent.id = participant.agent_id
           AND agent.organization_id = participant.organization_id
         WHERE participant.agent_id = $1
           AND (agent.runtime_kind <> 'container' OR (
               agent.container_id IS NOT NULL
               AND agent.container_image_identity IS NOT NULL
           ))
         FOR UPDATE OF participant, agent"#;

pub(crate) const NEXT_DISPATCHABLE_SQL: &str = r#"SELECT task.*
          FROM orchestration_tasks task
          JOIN groups task_group
            ON task_group.id = task.group_id
           AND task_group.organization_id = task.organization_id
           AND task_group.deleted_at IS NULL
          JOIN projects task_project
            ON task_project.id = task_group.project_id
           AND task_project.organization_id = task.organization_id
           AND task_project.deleted_at IS NULL
          JOIN workspaces task_workspace
            ON task_workspace.id = task_project.workspace_id
           AND task_workspace.organization_id = task.organization_id
           AND task_workspace.deleted_at IS NULL
          JOIN participants participant
            ON participant.agent_id = $2
           AND participant.organization_id = task.organization_id
          JOIN agents agent
            ON agent.id = participant.agent_id
           AND agent.organization_id = participant.organization_id
         WHERE task.organization_id = $1
           AND task.status IN ('queued', 'blocked')
           AND (task.blocked_reason IS NULL OR task.blocked_reason = 'waiting_agent')
           AND task.assigned_agent_id IS NULL
           AND task.requires_approval = FALSE
           AND (
                 task.parent_task_id IS NULL
                 OR EXISTS (
                      SELECT 1
                        FROM orchestration_tasks parent
                       WHERE parent.id = task.parent_task_id
                         AND parent.organization_id = task.organization_id
                         AND parent.status = 'completed'
                 )
               )
           AND (
                 task.params->'dependency_ids' IS NULL
                 OR jsonb_typeof(task.params->'dependency_ids') = 'array'
               )
           AND COALESCE(
                 CASE WHEN jsonb_typeof(task.params->'dependency_ids') = 'array'
                      THEN jsonb_array_length(task.params->'dependency_ids')
                      ELSE 0 END,
                 0) <= 10
           AND NOT EXISTS (
                 SELECT 1
                   FROM jsonb_array_elements_text(
                            CASE
                              WHEN jsonb_typeof(task.params->'dependency_ids') = 'array'
                                THEN task.params->'dependency_ids'
                              ELSE '[]'::jsonb
                            END
                        ) declared(id)
                   LEFT JOIN orchestration_tasks prerequisite
                     ON prerequisite.organization_id = task.organization_id
                    AND prerequisite.id::text = declared.id
                  WHERE prerequisite.status IS DISTINCT FROM 'completed'
               )
           AND NOT EXISTS (
                 SELECT 1
                   FROM jsonb_array_elements_text(
                            CASE
                              WHEN jsonb_typeof(COALESCE(
                                     task.params->'requiredInputs',
                                     task.params->'required_inputs'
                                   )) = 'array'
                                THEN COALESCE(task.params->'requiredInputs', task.params->'required_inputs')
                              ELSE '[]'::jsonb
                            END
                        ) required(name)
                  WHERE NOT EXISTS (
                        SELECT 1
                          FROM (VALUES
                                  (task.params),
                                  (task.params->'inputs'),
                                  (task.params->'env'),
                                  (task.params->'apiKeys'),
                                  (task.params->'api_keys')
                               ) container(value)
                          JOIN LATERAL jsonb_each(
                               CASE WHEN jsonb_typeof(container.value) = 'object'
                                    THEN container.value ELSE '{}'::jsonb END
                          ) supplied(name, value) ON supplied.name = required.name
                         WHERE supplied.value <> 'null'::jsonb
                           AND (
                                 jsonb_typeof(supplied.value) <> 'string'
                                 OR btrim(supplied.value #>> '{}') <> ''
                               )
                  )
               )
           AND participant.status = 'available'
           AND agent.workspace_id = task_project.workspace_id
           AND EXISTS (
                 SELECT 1
                   FROM unnest(participant.capabilities) capability
                  WHERE btrim(capability) <> ''
               )
           -- Image tasks are bound to a specific vision-capable container agent
           -- and need server-side image materialization that this self-claim
           -- lane cannot perform; never auto-dispatch one here (it would run the
           -- CLI without its images and bypass the vision/workspace gates).
           AND COALESCE(
                 CASE WHEN jsonb_typeof(task.params -> 'imageAttachmentIds') = 'array'
                      THEN jsonb_array_length(task.params -> 'imageAttachmentIds')
                      ELSE 0 END,
                 0) = 0
         ORDER BY
           CASE WHEN agent.project_id = task_project.id THEN 0 ELSE 1 END,
           CASE task.priority
             WHEN 'urgent' THEN 0
             WHEN 'high'   THEN 1
             WHEN 'normal' THEN 2
             WHEN 'low'    THEN 3
             ELSE 4
           END,
           task.created_at ASC
         FOR UPDATE OF task SKIP LOCKED
         LIMIT 1"#;

pub(crate) const AVAILABLE_PARTICIPANTS_SQL: &str = r#"SELECT participant.agent_id
          FROM participants participant
          JOIN agents agent
            ON agent.id = participant.agent_id
           AND agent.organization_id = participant.organization_id
         WHERE participant.status = 'available'
           AND (agent.interactive_lease_expires_at IS NULL
                OR agent.interactive_lease_expires_at <= NOW())
           AND NOT EXISTS (
                 SELECT 1
                   FROM orchestration_tasks active_task
                  WHERE active_task.organization_id = agent.organization_id
                    AND active_task.assigned_agent_id = agent.id
                    AND active_task.status = 'working'
               )
           AND (agent.runtime_kind <> 'container' OR (
               agent.container_id IS NOT NULL
               AND agent.container_image_identity IS NOT NULL
           ))
           AND EXISTS (
                 SELECT 1
                   FROM unnest(participant.capabilities) capability
                  WHERE btrim(capability) <> ''
               )
         ORDER BY last_heartbeat_at DESC NULLS LAST"#;

pub(crate) const CLAIM_TASK_SQL: &str = r#"UPDATE orchestration_tasks task
           SET assigned_agent_id = $3,
               status = 'working',
               blocked_reason = NULL,
               blocked_metadata = NULL,
               started_at = COALESCE(task.started_at, NOW()),
               attempt = task.attempt + 1,
               lease_expires_at = NOW() + ($5::text || ' seconds')::interval,
               last_assignment_id = $4,
               failure_code = NULL,
               retryable = FALSE,
               updated_at = NOW()
          FROM groups task_group,
               projects task_project,
               workspaces task_workspace,
               agents agent,
               participants participant
         WHERE task.id = $1
           AND task.organization_id = $2
           AND task.status IN ('queued', 'blocked')
           AND (task.blocked_reason IS NULL OR task.blocked_reason = 'waiting_agent')
           AND task.assigned_agent_id IS NULL
           AND task.requires_approval = FALSE
           AND task_group.id = task.group_id
           AND task_group.organization_id = task.organization_id
           AND task_group.deleted_at IS NULL
           AND task_project.id = task_group.project_id
           AND task_project.organization_id = task.organization_id
           AND task_project.deleted_at IS NULL
           AND task_workspace.id = task_project.workspace_id
           AND task_workspace.organization_id = task.organization_id
           AND task_workspace.deleted_at IS NULL
           AND agent.id = $3
           AND agent.organization_id = task.organization_id
           AND agent.workspace_id = task_project.workspace_id
           AND participant.agent_id = agent.id
           AND participant.organization_id = task.organization_id
           AND participant.status = 'available'
           AND EXISTS (
                 SELECT 1
                   FROM unnest(participant.capabilities) capability
                  WHERE btrim(capability) <> ''
               )
           AND COALESCE(
                 CASE WHEN jsonb_typeof(task.params -> 'imageAttachmentIds') = 'array'
                      THEN jsonb_array_length(task.params -> 'imageAttachmentIds')
                      ELSE 0 END,
                 0) = 0
         RETURNING task.*"#;

pub(crate) const INSERT_TASK_RUN_SQL: &str = r#"INSERT INTO task_runs
           (id, organization_id, workspace_id, orchestration_task_id, agent_id,
            idempotency_key, status, started_at, capability_profile)
        SELECT $1, $2, agent.workspace_id, $3, $4, $5, 'working',
               COALESCE($6, NOW()),
               jsonb_build_object('capabilities', $7::text[]) || CASE
                   WHEN agent.container_image_identity IS NULL THEN '{}'::jsonb
                   ELSE jsonb_build_object('image', agent.container_image_identity)
               END
          FROM agents agent
         WHERE agent.id = $4
           AND agent.organization_id = $2
           AND (agent.runtime_kind <> 'container' OR (
               agent.container_id IS NOT NULL
               AND agent.container_image_identity IS NOT NULL
           ))
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

/// Match claim/result lock order before expiring tasks and releasing owners.
pub(crate) const LOCK_EXPIRING_PARTICIPANTS_SQL: &str = r#"SELECT participant.id
          FROM participants participant
          JOIN agents agent
            ON agent.id = participant.agent_id
           AND agent.organization_id = participant.organization_id
         WHERE EXISTS (
               SELECT 1
                 FROM orchestration_tasks task
                WHERE task.organization_id = participant.organization_id
                  AND task.assigned_agent_id = participant.agent_id
                  AND task.status = 'working'
                  AND (
                      (task.lease_expires_at IS NOT NULL AND task.lease_expires_at < NOW())
                      OR (task.lease_expires_at IS NULL AND participant.status <> 'busy')
                  )
               )
         ORDER BY participant.id
         FOR UPDATE OF participant, agent"#;

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
               WHEN participants.status = 'offline' THEN 'offline'
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

pub(crate) const UPDATE_AGENT_STATUS_OFFLINE_SQL: &str = r#"UPDATE agents
           SET status = CASE
                   WHEN interactive_lease_expires_at > NOW()
                   THEN 'working'::agent_status
                   ELSE 'offline'::agent_status
               END,
               interactive_lease_expires_at = CASE
                   WHEN interactive_lease_expires_at > NOW()
                   THEN interactive_lease_expires_at
                   ELSE NULL
               END,
               interactive_owner_session_id = CASE
                   WHEN interactive_lease_expires_at > NOW()
                   THEN interactive_owner_session_id
                   ELSE NULL
               END,
               updated_at = CASE
                   WHEN status IS DISTINCT FROM CASE
                       WHEN interactive_lease_expires_at > NOW()
                       THEN 'working'::agent_status
                       ELSE 'offline'::agent_status
                   END THEN NOW()
                   ELSE updated_at
               END
         WHERE id = $1
           AND organization_id = $2
           AND EXISTS (
                 SELECT 1
                   FROM participants participant
                  WHERE participant.organization_id = agents.organization_id
                    AND participant.agent_id = agents.id
                    AND participant.status = 'offline'
               )"#;

#[derive(Debug, Deserialize)]
struct HeartbeatPayload {
    #[serde(default)]
    agent_id: Option<Uuid>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    cli_tool: Option<String>,
    #[serde(default)]
    container_generation_fingerprint: Option<String>,
    #[serde(default)]
    active_hook_session: Option<String>,
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
    presence: PresenceBackend,
}

impl ParticipantLivenessWorker {
    pub fn new(client: Client, pool: PgPool) -> Self {
        Self {
            client,
            pool,
            stale_after: DEFAULT_STALE_AFTER,
            sweep_interval: DEFAULT_STALE_SWEEP_INTERVAL,
            presence: PresenceBackend::postgres_only(DEFAULT_STALE_AFTER),
        }
    }

    pub fn with_stale_after(mut self, stale_after: Duration) -> Self {
        self.stale_after = stale_after;
        self
    }

    pub fn with_sweep_interval(mut self, sweep_interval: Duration) -> Self {
        self.sweep_interval = sweep_interval;
        self
    }

    /// Install the ADR 0008 Phase 2 presence backend. When the supplied backend
    /// is Redis-enabled, steady-state heartbeats are served from Redis instead
    /// of a PostgreSQL write; it degrades to the PG path on any Redis problem.
    pub fn with_presence(mut self, presence: PresenceBackend) -> Self {
        self.presence = presence;
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
                            if let Err(err) = handle_heartbeat(&self.client, &self.pool, &self.presence, &subject, &nats_msg.payload).await {
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
                    match sweep_offline(&self.client, &self.pool, &self.presence, self.stale_after).await {
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
                    // Reconcile backstop (ADR 0008): with the per-beat busy/available
                    // recompute gone, a `busy` participant whose task already left
                    // `working` (e.g. a best-effort post-commit release that failed)
                    // would otherwise stay busy forever. This periodic set-based
                    // sweep restores that self-heal at sweep cadence, before the
                    // drain so freed agents pick up work the same tick.
                    match reconcile_orphaned_busy(&self.client, &self.pool).await {
                        Ok(0) => {}
                        Ok(n) => tracing::warn!(participants = n, "Reconciled busy participants with no working task back to available"),
                        Err(err) => tracing::error!(error = ?err, "Busy-participant reconcile failed"),
                    }
                    match drain_available_participants(&self.client, &self.pool).await {
                        Ok(0) => {}
                        Ok(n) => {
                            metrics::counter!("agentforge_orchestration_participant_tasks_claimed_total")
                                .increment(n);
                            tracing::info!(tasks = n, "Drained dispatchable tasks onto available participants");
                        }
                        Err(err) => tracing::error!(error = ?err, "Available-participant drain failed"),
                    }
                }
            }
        }
    }
}

/// Describe and materialise this module's metrics at zero so dashboards and
/// `rate()` alerts have a series before the first heartbeat arrives (ADR 0008
/// gate inputs). Called from `crate::register_metrics`.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_participant_heartbeats_total",
        "Sidecar heartbeats applied to the participants table (one per beat)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_participant_status_transitions_total",
        "Heartbeats that actually changed agents.status (the non-steady-state minority)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_participant_reconciled_total",
        "Busy participants flipped back to available by the reconcile backstop (a failed event-driven release)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_participant_tasks_claimed_total",
        "Tasks claimed onto participants — by the per-heartbeat single claim and the drain backstop."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_agent_degraded_heartbeats_total",
        "Sidecar heartbeats carrying health.degraded=true (WAL backpressure or dropped events)."
    );
    metrics::counter!("agentforge_orchestration_participant_heartbeats_total").increment(0);
    metrics::counter!("agentforge_orchestration_participant_status_transitions_total").increment(0);
    metrics::counter!("agentforge_orchestration_participant_reconciled_total").increment(0);
    metrics::counter!("agentforge_orchestration_participant_tasks_claimed_total").increment(0);
    metrics::counter!("agentforge_orchestration_agent_degraded_heartbeats_total").increment(0);
}

/// Parse the optional `health` field from a raw heartbeat JSON payload.
///
/// Returns `Some((true, reason))` when the sidecar reports `health.degraded =
/// true` and provides a non-empty reason string. Returns `None` when the
/// `health` object is absent (backward compat — older sidecars omit it). The
/// `Some((false, _))` case is intentionally not returned: a non-degraded beat
/// is the steady state and requires no action.
fn heartbeat_is_degraded(payload: &serde_json::Value) -> Option<(bool, String)> {
    let health = payload.get("health")?;
    let degraded = health.get("degraded")?.as_bool()?;
    if !degraded {
        return None;
    }
    let reason = health.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
    Some((true, reason))
}

/// DB-only core of heartbeat handling (ADR 0008): upsert the participant
/// liveness row and mirror it onto `agents.status`. Returns the refreshed
/// participant plus whether `agents.status` actually changed, or `None` when no
/// agent row exists for `agent_id`. Kept free of NATS so the liveness write
/// contract is integration-testable against a pool; `handle_heartbeat` layers
/// metrics, broadcasts, and auto-dispatch on top.
pub async fn apply_heartbeat(
    pool: &PgPool,
    agent_id: Uuid,
    capabilities: Vec<String>,
) -> Result<Option<(Participant, bool)>> {
    // Hot path: touch the existing row. Common case, single-row UPDATE.
    let participant = match sqlx::query_as::<_, Participant>(TOUCH_PARTICIPANT_SQL)
        .bind(agent_id)
        .bind(capabilities.as_slice())
        .fetch_optional(pool)
        .await?
    {
        Some(participant) => participant,
        // First-seen (or a vanished row): INSERT with a correctly-derived status.
        None => match sqlx::query_as::<_, Participant>(INSERT_PARTICIPANT_SQL)
            .bind(agent_id)
            .bind(capabilities.as_slice())
            .fetch_optional(pool)
            .await?
        {
            Some(participant) => participant,
            None => return Ok(None), // no agent row exists for this id
        },
    };
    let status_changed = update_agent_status_from_participant(pool, &participant).await?;
    Ok(Some((participant, status_changed)))
}

/// Stable fingerprint of an advertised capability set, used as the Redis
/// presence value. A changed set (e.g. a sidecar that now reports `image_input`
/// after a rolling-deploy restart) yields a different fingerprint, so the beat
/// is treated as a transition and forced through the PostgreSQL capability write
/// rather than suppressed under the still-live presence key.
fn capability_fingerprint(capabilities: &[String]) -> String {
    let mut sorted = capabilities.to_vec();
    sorted.sort();
    sorted.join(",")
}

pub async fn handle_heartbeat(
    client: &Client,
    pool: &PgPool,
    presence: &PresenceBackend,
    subject: &str,
    payload: &[u8],
) -> Result<()> {
    let subject_agent = parse_heartbeat_agent_id(subject).ok_or_else(|| anyhow!("bad heartbeat subject {subject}"))?;
    // Parse as raw JSON first so we can inspect optional fields (e.g. `health`)
    // that are not part of the typed HeartbeatPayload struct.
    let raw: serde_json::Value = serde_json::from_slice(payload).with_context(|| "decode heartbeat payload")?;
    let payload: HeartbeatPayload =
        serde_json::from_value(raw.clone()).with_context(|| "deserialize heartbeat payload fields")?;

    if let Some(payload_agent) = payload.agent_id
        && payload_agent != subject_agent
    {
        return Err(anyhow!("heartbeat subject agent {subject_agent} disagrees with payload agent {payload_agent}"));
    }

    // Surface degraded relay health reported by the sidecar (issue #808).
    // Does NOT change participant status or dispatcher logic — metric + warn only.
    if let Some((_, reason)) = heartbeat_is_degraded(&raw) {
        metrics::counter!("agentforge_orchestration_agent_degraded_heartbeats_total").increment(1);
        tracing::warn!(%subject, reason, "sidecar reported degraded relay health");
    }

    metrics::counter!("agentforge_orchestration_participant_heartbeats_total").increment(1);

    // Interactive owner renewal is deliberately outside the Redis presence
    // fast-path. Every current sidecar heartbeat must refresh long-running CLI
    // work in Postgres, but only under the Agent lifecycle lock and only when
    // the exact current container generation + session epoch still own a live
    // lease. An old sidecar/monitor therefore cannot revive a timed-out owner
    // or touch a newly admitted prompt.
    if let (Some(session), Some(generation)) = (
        payload.active_hook_session.as_deref().map(str::trim).filter(|value| !value.is_empty()),
        payload.container_generation_fingerprint.as_deref().map(str::trim).filter(|value| !value.is_empty()),
    ) {
        renew_interactive_owner_from_heartbeat(pool, subject_agent, session, generation).await?;
    }

    // ADR 0008 Phase 2: when Redis presence is active and the agent is already
    // live, the beat is recorded entirely in Redis — no PostgreSQL write,
    // broadcast, or auto-dispatch. A transition (first-seen / TTL resurrection /
    // CHANGED capabilities) or any Redis fallback still runs the PostgreSQL path
    // below. The capability fingerprint ensures a sidecar that newly advertises a
    // capability (e.g. `image_input` after a rolling-deploy restart) forces a PG
    // write instead of being suppressed under its still-live Redis key.
    let capabilities = payload.normalized_capabilities();
    if presence.record(subject_agent, &capability_fingerprint(&capabilities)).await == RedisRecord::SteadyState {
        metrics::counter!("agentforge_orchestration_presence_redis_steady_total").increment(1);
        return Ok(());
    }
    let Some((participant, status_changed)) = apply_heartbeat(pool, subject_agent, capabilities).await? else {
        // The Redis `SET` already wrote the presence key (Transition), but there
        // is no agent row to back it. Drop the key so the next beat retries the
        // PG write instead of reading SteadyState and suppressing it for the TTL.
        presence.forget(subject_agent).await;
        return Err(anyhow!("no agent row found for heartbeat agent {subject_agent}"));
    };
    if presence.redis_enabled() {
        metrics::counter!("agentforge_orchestration_presence_redis_transition_total").increment(1);
    }

    // Attribution metric (ADR 0008): beats that actually moved agents.status.
    if status_changed {
        metrics::counter!("agentforge_orchestration_participant_status_transitions_total").increment(1);
    }

    if let Err(err) = publish_participant_update(client, &participant, "participant.heartbeat").await {
        tracing::warn!(error = %err, participant_id = %participant.id, "Failed to broadcast participant heartbeat");
    }

    if participant.status == "available"
        && let Some((task, busy_participant)) = claim_next_task_for_participant(pool, subject_agent).await?
    {
        metrics::counter!("agentforge_orchestration_participant_tasks_claimed_total").increment(1);
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

/// Renew the hook-owned crash-backstop lease for one exact owner epoch.
/// Returns false for stale generation/session, expired leases, and missing
/// agents. All are terminal no-ops for this heartbeat rather than retries that
/// could resurrect superseded work.
pub async fn renew_interactive_owner_from_heartbeat(
    pool: &PgPool,
    agent_id: Uuid,
    session_id: &str,
    generation_fingerprint: &str,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id).await?;
    let current_secret: Option<Option<String>> = sqlx::query_scalar("SELECT hmac_secret FROM agents WHERE id = $1")
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(Some(current_secret)) = current_secret else {
        tx.commit().await?;
        return Ok(false);
    };
    if container_generation_fingerprint(current_secret.as_bytes()) != generation_fingerprint {
        tx.commit().await?;
        return Ok(false);
    }
    let result = sqlx::query(
        r#"UPDATE agents
              SET interactive_lease_expires_at = NOW() + INTERVAL '2 minutes'
            WHERE id = $1
              AND hmac_secret = $2
              AND interactive_owner_session_id = $3
              AND interactive_lease_expires_at > NOW()"#,
    )
    .bind(agent_id)
    .bind(&current_secret)
    .bind(session_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

/// Offline sweep dispatcher (ADR 0008). In Redis-presence mode, liveness is the
/// presence key's existence: non-offline participants whose key has expired are
/// marked offline. On any Redis unavailability it falls back to the Phase 1
/// `last_heartbeat_at` sweep — but skips it during the post-fallback grace
/// window, because `last_heartbeat_at` is stale until PG-path beats repopulate
/// it (otherwise live agents would be wrongly marked offline).
pub async fn sweep_offline(
    client: &Client,
    pool: &PgPool,
    presence: &PresenceBackend,
    stale_after: Duration,
) -> Result<u64> {
    if presence.redis_enabled() {
        let candidates: Vec<Uuid> = sqlx::query_scalar(NON_OFFLINE_PARTICIPANT_AGENTS_SQL).fetch_all(pool).await?;
        if let Some(dead) = presence.dead_agents(&candidates).await {
            if dead.is_empty() {
                return Ok(0);
            }
            return mark_offline_by_agent_ids(client, pool, &dead).await;
        }
        // Redis went unavailable mid-sweep; presence armed the grace window.
        // Fall through to the (graced) PostgreSQL sweep below.
    }

    if presence.pg_sweep_within_grace() {
        tracing::debug!("Skipping PostgreSQL stale sweep during post-fallback grace window");
        return Ok(0);
    }

    mark_stale_offline(client, pool, stale_after).await
}

/// Mark the given agents offline (Redis-mode TTL expiry) and broadcast, reusing
/// the same agents-mirror + WS path as the timestamp-based stale sweep.
async fn mark_offline_by_agent_ids(client: &Client, pool: &PgPool, agent_ids: &[Uuid]) -> Result<u64> {
    let participants =
        sqlx::query_as::<_, Participant>(MARK_OFFLINE_BY_AGENT_IDS_SQL).bind(agent_ids).fetch_all(pool).await?;
    for participant in &participants {
        update_agent_status_offline(pool, participant).await?;
        if let Err(err) = publish_participant_update(client, participant, "participant.offline").await {
            tracing::warn!(error = %err, participant_id = %participant.id, "Failed to broadcast participant offline update");
        }
    }
    Ok(participants.len() as u64)
}

pub async fn mark_stale_offline(client: &Client, pool: &PgPool, stale_after: Duration) -> Result<u64> {
    let participants = sqlx::query_as::<_, Participant>(MARK_STALE_OFFLINE_SQL)
        .bind(stale_after.as_secs().to_string())
        .fetch_all(pool)
        .await?;
    for participant in &participants {
        update_agent_status_offline(pool, participant).await?;
        if let Err(err) = publish_participant_update(client, participant, "participant.offline").await {
            tracing::warn!(error = %err, participant_id = %participant.id, "Failed to broadcast participant offline update");
        }
    }
    Ok(participants.len() as u64)
}

/// DB-only core of the reconcile backstop (ADR 0008): flip `busy` participants
/// that no longer own a `working` task back to `available` and mirror
/// `agents.status`. Returns the reconciled rows so the caller can broadcast.
/// NATS-free so the reconcile contract is integration-testable against a pool.
/// Candidate locking plus a fresh-snapshot recheck prevents a delayed sweep
/// from overwriting a newer claim's `busy` state.
pub async fn reconcile_orphaned_busy_rows(pool: &PgPool) -> Result<Vec<Participant>> {
    let mut tx = pool.begin().await?;
    let candidate_ids = sqlx::query_scalar::<_, Uuid>(LOCK_ORPHANED_BUSY_CANDIDATES_SQL).fetch_all(&mut *tx).await?;
    let participants =
        sqlx::query_as::<_, Participant>(RECONCILE_ORPHANED_BUSY_SQL).bind(&candidate_ids).fetch_all(&mut *tx).await?;
    for participant in &participants {
        sqlx::query(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL)
            .bind(participant.agent_id.as_uuid())
            .bind(participant.organization_id.as_uuid())
            .bind("idle")
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(participants)
}

/// Periodic reconcile backstop: run [`reconcile_orphaned_busy_rows`], broadcast
/// each release, and count it. Returns the number of participants reconciled.
pub async fn reconcile_orphaned_busy(client: &Client, pool: &PgPool) -> Result<u64> {
    let participants = reconcile_orphaned_busy_rows(pool).await?;
    for participant in &participants {
        if let Err(err) = publish_participant_update(client, participant, "participant.available").await {
            tracing::warn!(
                error = %err,
                participant_id = %participant.id,
                "Failed to broadcast reconciled participant availability"
            );
        }
    }
    if !participants.is_empty() {
        metrics::counter!("agentforge_orchestration_participant_reconciled_total").increment(participants.len() as u64);
    }
    Ok(participants.len() as u64)
}

/// Mirror the participant's liveness onto `agents.status`. Returns `true` when
/// the agent row actually changed (the conditional `UPDATE` touched a row), so
/// the caller can count real transitions separately from no-op steady-state
/// beats (ADR 0008).
async fn update_agent_status_from_participant(pool: &PgPool, participant: &Participant) -> Result<bool> {
    let status = if participant.status == "busy" { "working" } else { "idle" };
    let mut tx = pool.begin().await?;
    agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, participant.agent_id.as_uuid()).await?;
    let result = sqlx::query(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL)
        .bind(participant.agent_id.as_uuid())
        .bind(participant.organization_id.as_uuid())
        .bind(status)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

async fn update_agent_status_offline(pool: &PgPool, participant: &Participant) -> Result<()> {
    let mut tx = pool.begin().await?;
    agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, participant.agent_id.as_uuid()).await?;
    sqlx::query(UPDATE_AGENT_STATUS_OFFLINE_SQL)
        .bind(participant.agent_id.as_uuid())
        .bind(participant.organization_id.as_uuid())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ExpiredLeaseOutcome {
    pub task: OrchestrationTask,
    pub participant: Option<Participant>,
}

pub async fn expire_working_leases(pool: &PgPool, stale_after: Duration) -> Result<Vec<ExpiredLeaseOutcome>> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar::<_, Uuid>(LOCK_EXPIRING_PARTICIPANTS_SQL).fetch_all(&mut *tx).await?;
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

// CN-4: span the auto-dispatch claim so the assignment it enqueues carries a
// `trace_context` (this is a background job, so there is no request span to
// inherit otherwise). No-op overhead when the OTLP layer is not installed.
#[tracing::instrument(skip(pool), fields(agent_id = %agent_id))]
async fn claim_next_task_for_participant(
    pool: &PgPool,
    agent_id: Uuid,
) -> Result<Option<(OrchestrationTask, Participant)>> {
    let mut tx = pool.begin().await?;

    agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id).await?;

    let Some(participant) =
        sqlx::query_as::<_, Participant>(LOCK_PARTICIPANT_SQL).bind(agent_id).fetch_optional(&mut *tx).await?
    else {
        tx.commit().await?;
        return Ok(None);
    };

    if participant.status != "available"
        || !participant.capabilities.iter().any(|capability| !capability.trim().is_empty())
    {
        tx.commit().await?;
        return Ok(None);
    }

    if agentforge_db::agent_work_admission_is_idle_in_tx(&mut tx, participant.organization_id.as_uuid(), agent_id)
        .await?
        != Some(true)
    {
        tx.commit().await?;
        return Ok(None);
    }

    // The lifecycle lock above freezes the per-container secret until this
    // transaction commits, so this fingerprint identifies exactly the
    // container generation to which the assignment is admitted.
    let (runtime_kind, hmac_secret): (RuntimeKind, Option<String>) =
        sqlx::query_as("SELECT runtime_kind, hmac_secret FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&mut *tx)
            .await
            .context("load assignment target generation")?;
    let container_generation_fingerprint = match runtime_kind {
        RuntimeKind::Container => {
            let secret = hmac_secret.as_deref().filter(|secret| !secret.trim().is_empty()).ok_or_else(|| {
                anyhow!("refusing to dispatch container agent {agent_id} without an HMAC generation secret")
            })?;
            Some(container_generation_fingerprint(secret.as_bytes()))
        }
        RuntimeKind::Cli | RuntimeKind::Api => None,
    };

    let Some(task) = sqlx::query_as::<_, OrchestrationTask>(NEXT_DISPATCHABLE_SQL)
        .bind(participant.organization_id.as_uuid())
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };

    let delivery_id = Uuid::now_v7();
    let Some(claimed_task) = sqlx::query_as::<_, OrchestrationTask>(CLAIM_TASK_SQL)
        .bind(task.id)
        .bind(participant.organization_id.as_uuid())
        .bind(agent_id)
        .bind(delivery_id)
        .bind(DEFAULT_ASSIGNMENT_LEASE_SECS.to_string())
        .fetch_optional(&mut *tx)
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };

    let delivery_id = claimed_task
        .last_assignment_id
        .ok_or_else(|| anyhow!("claimed task {} missing assignment id", claimed_task.id))?;
    let run_insert = sqlx::query(INSERT_TASK_RUN_SQL)
        .bind(Uuid::now_v7())
        .bind(participant.organization_id.as_uuid())
        .bind(claimed_task.id)
        .bind(agent_id)
        .bind(delivery_id.to_string())
        .bind(claimed_task.started_at)
        .bind(&participant.capabilities)
        .execute(&mut *tx)
        .await?;
    if run_insert.rows_affected() != 1 {
        return Err(anyhow!("refusing to dispatch task {} without immutable runtime evidence", claimed_task.id));
    }

    let busy_participant = sqlx::query_as::<_, Participant>(SET_PARTICIPANT_STATUS_SQL)
        .bind(participant.id)
        .bind("busy")
        .fetch_one(&mut *tx)
        .await?;

    let (task_text, message) = TaskInstruction::from_params(
        &claimed_task.title,
        claimed_task.description.as_deref(),
        claimed_task.params.as_ref(),
    )
    .into_parts();
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
        runtime_kind: Some(runtime_kind),
        container_generation_fingerprint,
        image_paths: Vec::new(),
        // CN-4: stamp the auto-dispatch span's trace so the sidecar continues it
        // across the NATS hop. `None` when the OTLP layer is not installed.
        trace_context: agentforge_telemetry::current_traceparent(),
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

    // Built through the shared `ServerMessage` enum (MS-3 PR-E) so the wire
    // contract has a single compiler-checked source of truth.
    let frame = ServerMessage::OrchestrationParticipantUpdate {
        payload: OrchestrationParticipantUpdatePayload {
            action: action.to_owned(),
            event_id: Uuid::now_v7(),
            participant: OrchestrationParticipantBrief {
                id: participant.id,
                agent_id: participant.agent_id.as_uuid(),
                name: participant.name.clone(),
                status: participant_status_for_ws(&participant.status).to_owned(),
            },
        },
    };
    publish_broadcast(client, participant.organization_id, &frame).await
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
    use chrono::Utc;

    struct DispatchFixture {
        org_id: Uuid,
        user_id: Uuid,
        workspace_a: Uuid,
        workspace_b: Uuid,
        project_a: Uuid,
        project_a_fallback: Uuid,
        project_b: Uuid,
        group_a: Uuid,
        group_a_fallback: Uuid,
        group_b: Uuid,
    }

    async fn seed_dispatch_fixture(pool: &PgPool) -> DispatchFixture {
        let fixture = DispatchFixture {
            org_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            workspace_a: Uuid::new_v4(),
            workspace_b: Uuid::new_v4(),
            project_a: Uuid::new_v4(),
            project_a_fallback: Uuid::new_v4(),
            project_b: Uuid::new_v4(),
            group_a: Uuid::new_v4(),
            group_a_fallback: Uuid::new_v4(),
            group_b: Uuid::new_v4(),
        };
        let team_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Dispatch Org', $2)")
            .bind(fixture.org_id)
            .bind(format!("dispatch-{}", fixture.org_id))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(fixture.user_id)
            .bind(format!("u-{}@example.com", fixture.user_id))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Dispatch', $3)")
            .bind(team_id)
            .bind(fixture.org_id)
            .bind(format!("dispatch-{team_id}"))
            .execute(pool)
            .await
            .expect("seed team");
        for (workspace_id, name) in [(fixture.workspace_a, "Workspace A"), (fixture.workspace_b, "Workspace B")] {
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)")
                .bind(workspace_id)
                .bind(fixture.org_id)
                .bind(name)
                .execute(pool)
                .await
                .expect("seed workspace");
        }
        for (project_id, workspace_id, name) in [
            (fixture.project_a, fixture.workspace_a, "Project A"),
            (fixture.project_a_fallback, fixture.workspace_a, "Project A fallback"),
            (fixture.project_b, fixture.workspace_b, "Project B"),
        ] {
            sqlx::query(
                "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(project_id)
            .bind(fixture.org_id)
            .bind(workspace_id)
            .bind(team_id)
            .bind(name)
            .bind(format!("project-{project_id}"))
            .execute(pool)
            .await
            .expect("seed project");
        }
        for (group_id, project_id, name) in [
            (fixture.group_a, fixture.project_a, "Group A"),
            (fixture.group_a_fallback, fixture.project_a_fallback, "Group A fallback"),
            (fixture.group_b, fixture.project_b, "Group B"),
        ] {
            sqlx::query(
                "INSERT INTO groups (id, organization_id, project_id, name, created_by) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(group_id)
            .bind(fixture.org_id)
            .bind(project_id)
            .bind(name)
            .bind(fixture.user_id)
            .execute(pool)
            .await
            .expect("seed group");
        }
        fixture
    }

    async fn seed_agent_participant(pool: &PgPool, fixture: &DispatchFixture) -> Uuid {
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents \
                (id, organization_id, workspace_id, project_id, user_id, name, status, cli_tool, runtime_kind, container_id, container_image_identity, hmac_secret) \
             VALUES ($1, $2, $3, $4, $5, 'dispatch-agent', 'idle', 'claude', 'container', $6, $7, $8)",
        )
        .bind(agent_id)
        .bind(fixture.org_id)
        .bind(fixture.workspace_a)
        .bind(fixture.project_a)
        .bind(fixture.user_id)
        .bind(format!("dispatch-container-{agent_id}"))
        .bind(serde_json::json!({
            "source": "agentforge-agent:claude",
            "imageId": format!("sha256:{}", "d".repeat(64)),
            "versionSource": "not-reported",
            "trust": "host-local"
        }))
        .bind(Uuid::new_v4().to_string())
        .execute(pool)
        .await
        .expect("seed agent");
        sqlx::query(
            "INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at) \
             VALUES ($1, $2, 'dispatch-agent', ARRAY['codex'], 'available', NOW())",
        )
        .bind(fixture.org_id)
        .bind(agent_id)
        .execute(pool)
        .await
        .expect("seed participant");
        agent_id
    }

    async fn seed_dispatch_task(pool: &PgPool, fixture: &DispatchFixture, group_id: Option<Uuid>, title: &str) -> Uuid {
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, group_id, title, status, priority, created_by) \
             VALUES ($1, $2, $3, $4, 'queued', 'normal', $5)",
        )
        .bind(task_id)
        .bind(fixture.org_id)
        .bind(group_id)
        .bind(title)
        .bind(fixture.user_id)
        .execute(pool)
        .await
        .expect("seed task");
        task_id
    }

    // -------------------------------------------------------------------------
    // heartbeat_is_degraded tests (issue #808)
    // -------------------------------------------------------------------------

    #[test]
    fn degraded_payload_returns_some_true_with_reason() {
        let payload = serde_json::json!({
            "agent_id": "00000000-0000-0000-0000-000000000001",
            "health": {
                "degraded": true,
                "reason": "wal_pending=1000 wal_dropped=0",
                "wal_pending": 1000,
                "wal_dropped": 0
            }
        });
        let result = heartbeat_is_degraded(&payload);
        assert!(result.is_some());
        let (degraded, reason) = result.unwrap();
        assert!(degraded);
        assert!(reason.contains("wal_pending=1000"));
    }

    #[test]
    fn missing_health_field_returns_none_for_backward_compat() {
        // Older sidecars omit the health field entirely.
        let payload = serde_json::json!({
            "agent_id": "00000000-0000-0000-0000-000000000001",
            "capabilities": ["claude"]
        });
        assert!(heartbeat_is_degraded(&payload).is_none());
    }

    #[test]
    fn non_degraded_health_field_returns_none() {
        let payload = serde_json::json!({
            "health": {
                "degraded": false,
                "reason": null,
                "wal_pending": 5,
                "wal_dropped": 0
            }
        });
        assert!(heartbeat_is_degraded(&payload).is_none());
    }

    #[test]
    fn degraded_with_missing_reason_returns_empty_reason() {
        // Defensively handle a future shape that omits `reason` despite degraded=true.
        let payload = serde_json::json!({
            "health": {
                "degraded": true,
                "wal_pending": 1000,
                "wal_dropped": 0
            }
        });
        let result = heartbeat_is_degraded(&payload);
        assert!(result.is_some());
        let (degraded, reason) = result.unwrap();
        assert!(degraded);
        assert_eq!(reason, "");
    }

    #[test]
    fn register_metrics_does_not_panic() {
        // Smoke test: calling register_metrics must not panic even when called
        // multiple times (the metrics recorder may be a no-op in tests).
        register_metrics();
        register_metrics();
    }

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
    fn touch_sql_recomputes_busy_only_on_resurrection() {
        // Hot path is a plain single-row UPDATE joined to agents for the name.
        assert!(TOUCH_PARTICIPANT_SQL.starts_with("UPDATE participants"));
        assert!(TOUCH_PARTICIPANT_SQL.contains("FROM agents agent"));
        assert!(TOUCH_PARTICIPANT_SQL.contains("participants.agent_id = $1"));
        assert!(TOUCH_PARTICIPANT_SQL.contains("last_heartbeat_at = NOW()"));
        // The busy/available recompute is gated behind a resurrection from
        // offline; steady-state beats keep the prior status (ADR 0008).
        assert!(TOUCH_PARTICIPANT_SQL.contains("WHEN participants.status = 'offline' THEN"));
        assert!(TOUCH_PARTICIPANT_SQL.contains("ELSE participants.status"));
        assert!(TOUCH_PARTICIPANT_SQL.contains("task.status = 'working'"));
    }

    #[test]
    fn insert_sql_derives_initial_status_from_working_task() {
        // First-seen must derive busy/available from the agent's working task so
        // a first beat can never leave a task-owning agent wrongly available.
        assert!(INSERT_PARTICIPANT_SQL.contains("INSERT INTO participants"));
        assert!(INSERT_PARTICIPANT_SQL.contains("FROM agents agent"));
        assert!(INSERT_PARTICIPANT_SQL.contains("task.status = 'working'"));
        assert!(INSERT_PARTICIPANT_SQL.contains("THEN 'busy'"));
        assert!(INSERT_PARTICIPANT_SQL.contains("ELSE 'available'"));
        // The race with a concurrent first beat collapses into a heartbeat touch.
        assert!(INSERT_PARTICIPANT_SQL.contains("ON CONFLICT (organization_id, agent_id) DO UPDATE"));
        assert!(INSERT_PARTICIPANT_SQL.contains("last_heartbeat_at = NOW()"));
    }

    #[test]
    fn stale_sql_only_marks_non_offline_rows() {
        assert!(MARK_STALE_OFFLINE_SQL.contains("status <> 'offline'"));
        assert!(MARK_STALE_OFFLINE_SQL.contains("last_heartbeat_at < NOW()"));
    }

    #[test]
    fn reconcile_sql_only_releases_busy_rows_without_a_working_task() {
        assert!(LOCK_ORPHANED_BUSY_CANDIDATES_SQL.contains("FOR UPDATE OF participant, agent"));
        assert!(RECONCILE_ORPHANED_BUSY_SQL.contains("SET status = 'available'"));
        assert!(RECONCILE_ORPHANED_BUSY_SQL.contains("id = ANY($1)"));
        assert!(RECONCILE_ORPHANED_BUSY_SQL.contains("AND status = 'busy'"));
        assert!(RECONCILE_ORPHANED_BUSY_SQL.contains("NOT EXISTS"));
        assert!(RECONCILE_ORPHANED_BUSY_SQL.contains("task.status = 'working'"));
    }

    #[test]
    fn redis_offline_sweep_sql_targets_non_offline_and_marks_by_agent_ids() {
        // The Redis-mode sweep probes non-offline participants and marks the
        // ones whose presence key expired (ADR 0008 Phase 2).
        assert!(NON_OFFLINE_PARTICIPANT_AGENTS_SQL.contains("status <> 'offline'"));
        assert!(NON_OFFLINE_PARTICIPANT_AGENTS_SQL.contains("SELECT agent_id"));
        assert!(MARK_OFFLINE_BY_AGENT_IDS_SQL.contains("SET status = 'offline'"));
        assert!(MARK_OFFLINE_BY_AGENT_IDS_SQL.contains("status <> 'offline'"));
        assert!(MARK_OFFLINE_BY_AGENT_IDS_SQL.contains("agent_id = ANY($1)"));
    }

    #[test]
    fn agent_status_sql_tracks_participant_liveness_without_touching_other_tenants() {
        assert!(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("organization_id = $2"));
        assert!(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("$3::agent_status"));
        assert!(UPDATE_AGENT_STATUS_OFFLINE_SQL.contains("'offline'::agent_status"));
        assert!(UPDATE_AGENT_STATUS_OFFLINE_SQL.contains("organization_id = $2"));
    }

    #[test]
    fn agent_status_heartbeat_sql_skips_unchanged_rows() {
        // The conditional WHERE makes an unchanged beat a zero-row write (no new
        // row version / WAL / dead tuple) — the steady-state case (ADR 0008).
        assert!(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("status IS DISTINCT FROM CASE"));
        // updated_at is now unconditional because the statement only runs when
        // the status actually changes; the old CASE-gated form is gone.
        assert!(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("updated_at = NOW()"));
        assert!(!UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("ELSE updated_at"));
        assert!(UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("interactive_lease_expires_at > NOW()"));
        assert!(!UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL.contains("INTERVAL '60 seconds'"));
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
    fn dispatch_sql_locks_only_the_routed_task_row() {
        assert!(NEXT_DISPATCHABLE_SQL.contains("SELECT task.*"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("FOR UPDATE OF task SKIP LOCKED"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("agent.workspace_id = task_project.workspace_id"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("task.requires_approval = FALSE"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("parent.status = 'completed'"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("prerequisite.status IS DISTINCT FROM 'completed'"));
        assert!(NEXT_DISPATCHABLE_SQL.contains("requiredInputs"));
        assert!(CLAIM_TASK_SQL.contains("agent.workspace_id = task_project.workspace_id"));
        assert!(CLAIM_TASK_SQL.contains("task.requires_approval = FALSE"));
    }

    #[test]
    fn participant_status_maps_to_ws_contract() {
        assert_eq!(participant_status_for_ws("available"), "online");
        assert_eq!(participant_status_for_ws("busy"), "busy");
        assert_eq!(participant_status_for_ws("offline"), "offline");
        assert_eq!(participant_status_for_ws("unknown"), "online");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_prefers_exact_then_falls_back_within_workspace_and_rechecks_live_route(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let exact = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "Exact").await;
        let fallback = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a_fallback), "Fallback").await;
        sqlx::query(
            "UPDATE orchestration_tasks SET priority = 'urgent', created_at = NOW() - INTERVAL '1 hour' WHERE id = $1",
        )
        .bind(fallback)
        .execute(&pool)
        .await
        .expect("prioritize fallback");

        let (claimed, _) =
            claim_next_task_for_participant(&pool, agent_id).await.expect("claim exact").expect("exact claim");
        assert_eq!(claimed.id, exact, "exact project must win before task priority");

        sqlx::query(
            "UPDATE orchestration_tasks SET status = 'completed', completed_at = NOW(), lease_expires_at = NULL WHERE id = $1",
        )
        .bind(exact)
        .execute(&pool)
        .await
        .expect("finish exact task");
        sqlx::query("UPDATE participants SET status = 'available' WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("release participant");
        sqlx::query("UPDATE projects SET deleted_at = NOW() WHERE id = $1")
            .bind(fixture.project_a)
            .execute(&pool)
            .await
            .expect("soft-delete agent primary project");
        let (claimed, _) = claim_next_task_for_participant(&pool, agent_id)
            .await
            .expect("claim fallback")
            .expect("workspace fallback");
        assert_eq!(claimed.id, fallback, "same-workspace alternate project must be eligible");

        sqlx::query(
            "UPDATE orchestration_tasks SET status = 'completed', completed_at = NOW(), lease_expires_at = NULL WHERE id = $1",
        )
        .bind(fallback)
        .execute(&pool)
        .await
        .expect("finish fallback task");
        sqlx::query("UPDATE participants SET status = 'available' WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("release participant again");
        sqlx::query("UPDATE agents SET workspace_id = $1, project_id = $2 WHERE id = $3")
            .bind(fixture.workspace_b)
            .bind(fixture.project_b)
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("move agent to workspace B");
        let unrouteable = seed_dispatch_task(&pool, &fixture, None, "No route").await;
        let workspace_a = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "Workspace A").await;
        let workspace_b = seed_dispatch_task(&pool, &fixture, Some(fixture.group_b), "Workspace B").await;
        sqlx::query("UPDATE orchestration_tasks SET priority = 'urgent', created_at = NOW() - INTERVAL '2 hours' WHERE id = ANY($1)")
            .bind([unrouteable, workspace_a])
            .execute(&pool)
            .await
            .expect("prioritize invalid heads");
        let (claimed, _) = claim_next_task_for_participant(&pool, agent_id)
            .await
            .expect("claim after workspace move")
            .expect("workspace B claim");
        assert_eq!(claimed.id, workspace_b, "unrouteable and cross-workspace tasks must not starve a valid route");

        sqlx::query("UPDATE participants SET status = 'available', capabilities = '{}' WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("make participant chat-only");
        seed_dispatch_task(&pool, &fixture, Some(fixture.group_b), "Chat-only must skip").await;
        assert!(claim_next_task_for_participant(&pool, agent_id).await.expect("chat-only claim").is_none());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_skips_inconsistent_approval_input_parent_and_explicit_dependency_rows(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let parent = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "unfinished parent").await;
        let prerequisite = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "unfinished prerequisite").await;
        sqlx::query("UPDATE orchestration_tasks SET status = 'working' WHERE id = ANY($1)")
            .bind([parent, prerequisite])
            .execute(&pool)
            .await
            .unwrap();

        let approval = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "approval bypass").await;
        let missing_input = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "input bypass").await;
        let parent_child = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "parent bypass").await;
        let dependent = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "dependency bypass").await;
        sqlx::query(
            r#"UPDATE orchestration_tasks
                  SET priority = 'urgent',
                      created_at = NOW() - INTERVAL '1 hour',
                      requires_approval = CASE WHEN id = $1 THEN TRUE ELSE requires_approval END,
                      parent_task_id = CASE WHEN id = $3 THEN $5 ELSE parent_task_id END,
                      params = CASE
                          WHEN id = $2 THEN '{"requiredInputs":["MODEL_KEY"],"env":{}}'::jsonb
                          WHEN id = $4 THEN jsonb_build_object('dependency_ids', jsonb_build_array($6::text))
                          ELSE params
                      END
                WHERE id = ANY($7)"#,
        )
        .bind(approval)
        .bind(missing_input)
        .bind(parent_child)
        .bind(dependent)
        .bind(parent)
        .bind(prerequisite)
        .bind([approval, missing_input, parent_child, dependent])
        .execute(&pool)
        .await
        .unwrap();
        let valid = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "valid").await;

        let (claimed, _) = claim_next_task_for_participant(&pool, agent_id).await.unwrap().expect("valid claim");
        assert_eq!(claimed.id, valid);
        let invalid_states: Vec<String> =
            sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = ANY($1) ORDER BY id")
                .bind([approval, missing_input, parent_child, dependent])
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(invalid_states, vec!["queued"; 4]);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_skips_container_without_image_identity(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let task_id = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "Unverified").await;
        sqlx::query("UPDATE agents SET container_image_identity = NULL WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("clear image identity");

        assert!(
            claim_next_task_for_participant(&pool, agent_id)
                .await
                .expect("identity-less container must be skipped")
                .is_none()
        );

        let (task_status, participant_status, run_count, outbox_count): (String, String, i64, i64) = sqlx::query_as(
            r#"SELECT task.status,
                      (SELECT status FROM participants WHERE agent_id = $2),
                      (SELECT count(*) FROM task_runs WHERE orchestration_task_id = task.id),
                      (SELECT count(*) FROM orchestration_outbox WHERE aggregate_id = task.id)
                 FROM orchestration_tasks task
                WHERE task.id = $1"#,
        )
        .bind(task_id)
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .expect("read rolled-back claim state");
        assert_eq!(
            (task_status.as_str(), participant_status.as_str(), run_count, outbox_count),
            ("queued", "available", 0, 0)
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_rejects_container_without_generation_secret(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let task_id = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "Missing generation").await;
        sqlx::query("UPDATE agents SET hmac_secret = NULL WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("clear generation secret");

        let err = claim_next_task_for_participant(&pool, agent_id)
            .await
            .expect_err("container without generation secret must fail closed");
        assert!(err.to_string().contains("HMAC generation secret"), "unexpected error: {err:#}");

        let (task_status, participant_status, run_count, outbox_count): (String, String, i64, i64) = sqlx::query_as(
            r#"SELECT task.status,
                      (SELECT status FROM participants WHERE agent_id = $2),
                      (SELECT count(*) FROM task_runs WHERE orchestration_task_id = task.id),
                      (SELECT count(*) FROM orchestration_outbox WHERE aggregate_id = task.id)
                 FROM orchestration_tasks task
                WHERE task.id = $1"#,
        )
        .bind(task_id)
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .expect("read generation-rejected claim state");
        assert_eq!(
            (task_status.as_str(), participant_status.as_str(), run_count, outbox_count),
            ("queued", "available", 0, 0)
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_rejects_live_interactive_owner_but_not_a_stale_status_mirror(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let task_id = seed_dispatch_task(&pool, &fixture, Some(fixture.group_a), "Interactive owner").await;
        sqlx::query(
            "UPDATE agents SET status = 'working', interactive_lease_expires_at = NOW() + INTERVAL '60 seconds' WHERE id = $1",
        )
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("establish interactive owner");

        assert!(
            claim_next_task_for_participant(&pool, agent_id).await.expect("live interactive owner check").is_none(),
            "orchestration must not overlap a live terminal/MCP lease"
        );

        sqlx::query(
            "UPDATE agents SET status = 'working', interactive_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
        )
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("expire interactive owner");
        let (claimed, _) = claim_next_task_for_participant(&pool, agent_id)
            .await
            .expect("expired lease claim")
            .expect("stale status mirror must not remain authoritative");
        assert_eq!(claimed.id, task_id);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn heartbeat_preserves_but_does_not_make_interactive_work_permanent(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let participant: Participant = sqlx::query_as("SELECT * FROM participants WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_one(&pool)
            .await
            .expect("read participant");
        sqlx::query(
            "UPDATE agents SET status = 'working', interactive_lease_expires_at = NOW() + INTERVAL '10 seconds' WHERE id = $1",
        )
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("establish expiring interactive lease");
        let before: chrono::DateTime<Utc> =
            sqlx::query_scalar("SELECT interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .expect("read original lease");

        assert!(!update_agent_status_from_participant(&pool, &participant).await.expect("heartbeat update"));
        let (status, renewed): (String, Option<chrono::DateTime<Utc>>) =
            sqlx::query_as("SELECT status::text, interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .expect("read renewed owner");
        assert_eq!(status, "working", "available participant heartbeat must not overwrite interactive work");
        assert_eq!(renewed, Some(before), "ordinary heartbeat must not extend a bounded interactive owner");

        sqlx::query("UPDATE agents SET interactive_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("expire interactive lease");
        assert!(update_agent_status_from_participant(&pool, &participant).await.expect("expired heartbeat update"));
        let (status, lease): (String, Option<chrono::DateTime<Utc>>) =
            sqlx::query_as("SELECT status::text, interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .expect("read released owner");
        assert_eq!(status, "idle");
        assert!(lease.is_none());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn hook_owner_heartbeat_requires_exact_live_session_and_generation(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;
        let secret = Uuid::new_v4().to_string();
        let generation = container_generation_fingerprint(secret.as_bytes());
        sqlx::query(
            "UPDATE agents \
                SET status = 'working', hmac_secret = $2, \
                    interactive_owner_session_id = 'session-b', \
                    interactive_lease_expires_at = NOW() + INTERVAL '10 seconds' \
              WHERE id = $1",
        )
        .bind(agent_id)
        .bind(&secret)
        .execute(&pool)
        .await
        .expect("establish current hook owner");
        let original: chrono::DateTime<Utc> =
            sqlx::query_scalar("SELECT interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert!(
            !renew_interactive_owner_from_heartbeat(&pool, agent_id, "session-a", &generation).await.unwrap(),
            "paused monitor A cannot renew newer prompt B"
        );
        assert!(
            !renew_interactive_owner_from_heartbeat(
                &pool,
                agent_id,
                "session-b",
                &container_generation_fingerprint(b"replacement-secret"),
            )
            .await
            .unwrap(),
            "a prior container generation cannot renew the current owner"
        );
        let unchanged: chrono::DateTime<Utc> =
            sqlx::query_scalar("SELECT interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, original);

        assert!(renew_interactive_owner_from_heartbeat(&pool, agent_id, "session-b", &generation).await.unwrap());
        let renewed_beyond_original_ttl: bool = sqlx::query_scalar(
            "SELECT interactive_lease_expires_at > NOW() + INTERVAL '110 seconds' FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(renewed_beyond_original_ttl, "matching heartbeats keep legitimate long work exclusive");

        sqlx::query("UPDATE agents SET interactive_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            !renew_interactive_owner_from_heartbeat(&pool, agent_id, "session-b", &generation).await.unwrap(),
            "an expired crash backstop cannot be resurrected"
        );
    }

    // An image task is bound to a specific vision-capable container agent and
    // needs server-side image materialization (object storage + symlink-safe
    // workspace write), which this self-claim lane cannot do. So it must NEVER
    // be auto-claimed here — otherwise the CLI runs without its images and the
    // vision/workspace gates are bypassed. See task_image_materializer.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn next_dispatchable_excludes_image_tasks(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let agent_id = seed_agent_participant(&pool, &fixture).await;

        // Image task: unassigned + queued + sorted FIRST (earlier created_at).
        let image_task = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, group_id, title, status, created_by, params, created_at, updated_at)
               VALUES ($1, $2, $3, 'Image', 'queued', $4, $5::jsonb, NOW() - INTERVAL '1 hour', NOW())"#,
        )
        .bind(image_task)
        .bind(fixture.org_id)
        .bind(fixture.group_a)
        .bind(fixture.user_id)
        .bind(serde_json::json!({ "imageAttachmentIds": ["11111111-1111-1111-1111-111111111111"] }))
        .execute(&pool)
        .await
        .expect("seed image task");

        // Plain task: unassigned + queued, later created_at.
        let plain_task = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, group_id, title, status, created_by, created_at, updated_at)
               VALUES ($1, $2, $3, 'Plain', 'queued', $4, NOW(), NOW())"#,
        )
        .bind(plain_task)
        .bind(fixture.org_id)
        .bind(fixture.group_a)
        .bind(fixture.user_id)
        .execute(&pool)
        .await
        .expect("seed plain task");

        let claimed = sqlx::query_as::<_, OrchestrationTask>(NEXT_DISPATCHABLE_SQL)
            .bind(fixture.org_id)
            .bind(agent_id)
            .fetch_optional(&pool)
            .await
            .expect("query dispatchable");

        // Despite sorting first, the image task is skipped; the plain task wins.
        assert_eq!(claimed.map(|t| t.id), Some(plain_task), "image task must be excluded from the self-claim lane");

        // And with ONLY an image task queued, the lane returns nothing.
        sqlx::query("DELETE FROM orchestration_tasks WHERE id = $1")
            .bind(plain_task)
            .execute(&pool)
            .await
            .expect("del");
        let none = sqlx::query_as::<_, OrchestrationTask>(NEXT_DISPATCHABLE_SQL)
            .bind(fixture.org_id)
            .bind(agent_id)
            .fetch_optional(&pool)
            .await
            .expect("query dispatchable again");
        assert!(none.is_none(), "an image-only queue must not self-dispatch");
    }
}
