//! Queue operations: enqueue, dequeue, complete, fail, release stale locks.
//!
//! Uses the `FOR UPDATE SKIP LOCKED` pattern for safe concurrent job processing
//! across multiple worker instances without contention.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// A job entry fetched from the `job_queue` table.
///
/// Re-exported from `agentforge_db::entities::JobQueueEntry` would also work,
/// but we define a local type to keep the jobs crate self-contained for its API.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct JobEntry {
    pub id: Uuid,
    pub queue: String,
    pub payload: Value,
    pub status: String,
    pub priority: i32,
    pub run_at: DateTime<Utc>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub locked_by: Option<String>,
    pub locked_at: Option<DateTime<Utc>>,
    pub unique_key: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Enqueue a new job into the specified queue.
///
/// If `unique_key` is provided and a job with that key already exists, the insert
/// is silently skipped (ON CONFLICT DO NOTHING). Returns `None` in that case.
///
/// NOTE: The `unique_key` ON CONFLICT clause requires a partial unique index:
/// ```sql
/// CREATE UNIQUE INDEX idx_job_queue_unique_key
///     ON job_queue(unique_key) WHERE unique_key IS NOT NULL;
/// ```
/// This index should be added in a future migration.
pub async fn enqueue(
    pool: &PgPool,
    queue: &str,
    payload: Value,
    priority: i32,
    run_at: Option<DateTime<Utc>>,
    unique_key: Option<&str>,
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let run_at = run_at.unwrap_or_else(Utc::now);
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (unique_key) WHERE unique_key IS NOT NULL DO NOTHING
           RETURNING id"#,
    )
    .bind(queue)
    .bind(&payload)
    .bind(priority)
    .bind(run_at)
    .bind(unique_key)
    .bind(max_attempts)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// Dequeue and lock the next available job from the specified queue.
///
/// Uses `FOR UPDATE SKIP LOCKED` to avoid contention between concurrent workers.
/// Jobs are ordered by priority (descending) then creation time (ascending).
/// Only jobs with `status = 'pending'` and `run_at <= now()` are considered.
pub async fn dequeue(pool: &PgPool, queue: &str, worker_id: &str) -> Result<Option<JobEntry>, sqlx::Error> {
    let job = sqlx::query_as::<_, JobEntry>(
        r#"UPDATE job_queue
           SET status = 'running', locked_by = $2, locked_at = now(), attempts = attempts + 1
           WHERE id = (
               SELECT id FROM job_queue
               WHERE queue = $1 AND status = 'pending' AND run_at <= now()
               ORDER BY priority DESC, created_at ASC
               FOR UPDATE SKIP LOCKED
               LIMIT 1
           )
           RETURNING *"#,
    )
    .bind(queue)
    .bind(worker_id)
    .fetch_optional(pool)
    .await?;
    Ok(job)
}

/// Mark a job as completed by deleting it from the queue.
pub async fn complete(pool: &PgPool, job_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM job_queue WHERE id = $1").bind(job_id).execute(pool).await?;
    Ok(())
}

/// Mark a job as failed. If max attempts are exceeded, the job moves to `dead` status.
/// Otherwise, it is rescheduled with exponential backoff (2^attempts seconds).
pub async fn fail(pool: &PgPool, job_id: Uuid, error_message: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE job_queue SET
            status = CASE
                WHEN attempts >= max_attempts THEN 'dead'
                ELSE 'pending'
            END,
            error_message = $2,
            locked_by = NULL,
            locked_at = NULL,
            run_at = CASE
                WHEN attempts >= max_attempts THEN run_at
                ELSE now() + make_interval(secs => power(2, attempts))
            END
           WHERE id = $1"#,
    )
    .bind(job_id)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}

/// Release stale locks where a worker has held a job longer than the timeout.
///
/// Returns the number of jobs that were released back to `pending` status.
/// This is a safety mechanism for workers that crash without completing or failing.
pub async fn release_stale_locks(pool: &PgPool, timeout_minutes: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE job_queue SET
            status = 'pending', locked_by = NULL, locked_at = NULL
           WHERE status = 'running'
             AND locked_at < now() - make_interval(mins => $1)"#,
    )
    .bind(timeout_minutes)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_entry_serialization_roundtrip() {
        let job = JobEntry {
            id: Uuid::now_v7(),
            queue: "email".to_string(),
            payload: serde_json::json!({"to": "dev@example.com", "subject": "test"}),
            status: "pending".to_string(),
            priority: 5,
            run_at: Utc::now(),
            attempts: 0,
            max_attempts: 3,
            locked_by: None,
            locked_at: None,
            unique_key: Some("email-123".to_string()),
            error_message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: JobEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, job.id);
        assert_eq!(deserialized.queue, "email");
        assert_eq!(deserialized.priority, 5);
        assert_eq!(deserialized.payload["to"], "dev@example.com");
    }

    #[test]
    fn job_entry_with_failure_state() {
        let job = JobEntry {
            id: Uuid::now_v7(),
            queue: "webhook".to_string(),
            payload: serde_json::json!({"url": "https://example.com/hook"}),
            status: "dead".to_string(),
            priority: 0,
            run_at: Utc::now(),
            attempts: 3,
            max_attempts: 3,
            locked_by: None,
            locked_at: None,
            unique_key: None,
            error_message: Some("Connection refused".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(job.status, "dead");
        assert_eq!(job.attempts, job.max_attempts);
        assert!(job.error_message.is_some());
    }

    #[test]
    fn job_entry_running_state() {
        let now = Utc::now();
        let job = JobEntry {
            id: Uuid::now_v7(),
            queue: "cleanup".to_string(),
            payload: serde_json::json!({}),
            status: "running".to_string(),
            priority: 10,
            run_at: now,
            attempts: 1,
            max_attempts: 5,
            locked_by: Some("worker-01".to_string()),
            locked_at: Some(now),
            unique_key: None,
            error_message: None,
            created_at: now,
            updated_at: now,
        };

        assert_eq!(job.status, "running");
        assert_eq!(job.locked_by.as_deref(), Some("worker-01"));
        assert!(job.locked_at.is_some());
    }
}
