use std::time::{Duration, Instant};

use agentforge_core::orchestration_protocol::{TaskAssignment, assign_subject};
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
        let subject = assign_subject(assignment.agent_id);
        let bytes = serde_json::to_vec(&assignment).context("encode orchestration assignment payload")?;
        let publish_started = Instant::now();

        jetstream::new(self.client.clone())
            .publish(subject.clone(), bytes.into())
            .await
            .with_context(|| format!("publish assignment outbox row {} to {subject}", row.id))?
            .await
            .with_context(|| format!("await assignment JetStream ack for outbox row {}", row.id))?;
        metrics::histogram!("agentforge_orchestration_assignment_publish_seconds")
            .record(publish_started.elapsed().as_secs_f64());

        sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .context("mark orchestration outbox row published")?;
        tx.commit().await.context("commit published outbox tx")?;

        tracing::info!(outbox_id = %row.id, aggregate_id = %row.aggregate_id, %subject, "Published orchestration assignment outbox row");
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

    metrics::counter!("agentforge_orchestration_outbox_published_total").increment(0);
    metrics::counter!("agentforge_orchestration_outbox_publish_errors_total").increment(0);
    metrics::histogram!("agentforge_orchestration_assignment_publish_seconds").record(0.0);
}
