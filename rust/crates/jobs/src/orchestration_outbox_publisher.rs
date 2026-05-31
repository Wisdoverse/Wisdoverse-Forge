use std::time::{Duration, Instant};

use agentforge_core::RuntimeKind;
use agentforge_core::orchestration_protocol::{TaskAssignment, assign_subject, assign_subject_kind};
use agentforge_db::entities::OrchestrationOutbox;
use anyhow::{Context, Result};
use async_nats::{Client, jetstream};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::watch;

const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const ERROR_BACKOFF: Duration = Duration::from_secs(1);

const NEXT_ASSIGNMENT_OUTBOX_SQL: &str = r#"SELECT *
      FROM orchestration_outbox
     WHERE published_at IS NULL
       AND event_type = 'assignment'
     ORDER BY created_at ASC
     FOR UPDATE SKIP LOCKED
     LIMIT 1"#;

const MARK_OUTBOX_PUBLISHED_SQL: &str = r#"UPDATE orchestration_outbox
       SET published_at = NOW()
     WHERE id = $1"#;

pub struct OrchestrationOutboxPublisher {
    pool: PgPool,
    client: Client,
}

impl OrchestrationOutboxPublisher {
    pub fn new(pool: PgPool, client: Client) -> Self {
        Self { pool, client }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!("Orchestration outbox publisher shutting down");
                        break;
                    }
                }
                _ = async {
                    match self.publish_next().await {
                        Ok(true) => {}
                        Ok(false) => tokio::time::sleep(IDLE_POLL_INTERVAL).await,
                        Err(err) => {
                            metrics::counter!("agentforge_orchestration_outbox_publish_errors_total").increment(1);
                            tracing::warn!(error = %err, "orchestration outbox publish failed");
                            tokio::time::sleep(ERROR_BACKOFF).await;
                        }
                    }
                } => {}
            }
        }
    }

    async fn publish_next(&self) -> Result<bool> {
        let mut tx = self.pool.begin().await.context("begin outbox tx")?;
        let Some(row) = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_ASSIGNMENT_OUTBOX_SQL)
            .fetch_optional(&mut *tx)
            .await
            .context("select next orchestration outbox row")?
        else {
            tx.commit().await.context("commit empty outbox tx")?;
            return Ok(false);
        };

        let assignment: TaskAssignment =
            serde_json::from_value(row.payload.clone()).context("decode orchestration assignment outbox payload")?;

        // #457 phase 1c: resolve the target agent's runtime_kind for the
        // namespaced subject. The hot auto-dispatch path (participant_liveness)
        // carries it on the assignment; the cold API-dispatch path and pre-1c
        // outbox rows leave it None, so fall back to one indexed PK lookup here
        // (None only on cold/old rows, never the hot path). NOT NULL post-062;
        // a vanished agent degrades to Container — the legacy copy below still
        // covers that agent's old sidecar regardless.
        let kind = match assignment.runtime_kind {
            Some(kind) => kind,
            None => {
                let raw: Option<String> = sqlx::query_scalar("SELECT runtime_kind FROM agents WHERE id = $1")
                    .bind(assignment.agent_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("lookup agent runtime_kind for assignment publish")?;
                raw.and_then(|raw| RuntimeKind::parse_legacy(&raw).ok()).unwrap_or_else(|| {
                    // The agent row vanished mid-flight (or holds an unparseable
                    // kind, impossible under the 062 CHECK). Defaulting to
                    // Container is HARMLESS while we still dual-publish the legacy
                    // copy, but after the orchestration.assigned legacy-drop deploy
                    // a wrong-kind namespaced publish would silently strand a
                    // cli/api agent's assignment. Surface it loudly + count it so
                    // the legacy-drop gate can require this metric at zero.
                    tracing::warn!(
                        agent_id = %assignment.agent_id,
                        outbox_id = %row.id,
                        "assignment runtime_kind unresolved at publish; defaulting to Container \
                         — harmless during dual-publish, an assignment-loss vector after legacy-drop"
                    );
                    metrics::counter!("agentforge_orchestration_assignment_kind_fallback_total").increment(1);
                    RuntimeKind::Container
                })
            }
        };

        let legacy_subject = assign_subject(assignment.agent_id);
        let namespaced_subject = assign_subject_kind(kind, assignment.agent_id);
        let bytes = serde_json::to_vec(&assignment).context("encode orchestration assignment payload")?;
        let publish_started = Instant::now();
        let js = jetstream::new(self.client.clone());

        // Dual-publish the SAME payload (same delivery_id) to BOTH the legacy and
        // namespaced subjects during the migration drain. A given agent's
        // per-agent durable filters exactly ONE shape (legacy on pre-1c images,
        // namespaced on new images) and the platform cannot know which image the
        // agent runs, so it must emit both. AssignmentInbox dedups by
        // delivery_id BEFORE the CLI runs, so a sidecar that ever matches both
        // executes the task only once. Mark the row published only after BOTH
        // acks succeed (a partial publish leaves the row unpublished and is
        // retried — a harmless duplicate the inbox dedups). The legacy-drop
        // deploy later switches this to namespaced-only.
        for subject in [&legacy_subject, &namespaced_subject] {
            js.publish(subject.clone(), bytes.clone().into())
                .await
                .with_context(|| format!("publish assignment outbox row {} to {subject}", row.id))?
                .await
                .with_context(|| format!("await assignment JetStream ack for outbox row {} on {subject}", row.id))?;
        }
        metrics::histogram!("agentforge_orchestration_assignment_publish_seconds")
            .record(publish_started.elapsed().as_secs_f64());

        sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .context("mark orchestration outbox row published")?;
        tx.commit().await.context("commit published outbox tx")?;

        tracing::info!(
            outbox_id = %row.id,
            aggregate_id = %row.aggregate_id,
            legacy = %legacy_subject,
            namespaced = %namespaced_subject,
            "Published orchestration assignment outbox row (dual-published)"
        );
        metrics::counter!("agentforge_orchestration_outbox_published_total").increment(1);
        Ok(true)
    }
}

pub async fn insert_assignment_outbox_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: uuid::Uuid,
    task_id: uuid::Uuid,
    assignment: &TaskAssignment,
) -> Result<()> {
    let payload = serde_json::to_value(assignment).context("serialize assignment for outbox row")?;
    let delivery_id = assignment.delivery_id.context("assignment missing delivery_id")?;

    sqlx::query(
        r#"INSERT INTO orchestration_outbox
           (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
           VALUES ($1, $2, 'orchestration_task', $3, 'assignment', $4)"#,
    )
    .bind(delivery_id)
    .bind(organization_id)
    .bind(task_id)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .context("insert orchestration assignment outbox row")?;

    Ok(())
}

pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_outbox_published_total",
        "Orchestration assignment outbox rows published to JetStream after publish ack"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_outbox_publish_errors_total",
        "Orchestration assignment outbox publish attempts that failed before DB publish mark"
    );
    metrics::describe_histogram!(
        "agentforge_orchestration_assignment_publish_seconds",
        "Time to publish one orchestration assignment outbox row to JetStream and receive ack"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_assignment_kind_fallback_total",
        "#457 phase 1c: assignment publishes whose runtime_kind could not be resolved and \
         defaulted to Container. The orchestration.assigned legacy-drop deploy is gated on this \
         holding at zero (a non-zero value risks silently stranding a cli/api assignment post-drop)."
    );

    metrics::counter!("agentforge_orchestration_outbox_published_total").increment(0);
    metrics::counter!("agentforge_orchestration_outbox_publish_errors_total").increment(0);
    metrics::histogram!("agentforge_orchestration_assignment_publish_seconds").record(0.0);
    metrics::counter!("agentforge_orchestration_assignment_kind_fallback_total").increment(0);
}
