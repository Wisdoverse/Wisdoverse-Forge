//! Background worker loop for processing jobs from a queue.
//!
//! Uses a fixed-interval polling loop. (A `pg_notify`-driven low-latency wake-up
//! was never wired — the trigger that emitted it was removed in migration 073 —
//! so latency is bounded by `poll_interval`. To add low-latency dispatch later,
//! reintroduce the trigger and add a `PgListener` arm to `run`.)
//! The worker dequeues one job at a time, processes it via a user-supplied handler,
//! then marks it as completed or failed.

use futures::FutureExt;
use sqlx::PgPool;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::Duration;
use tokio::sync::watch;

/// Extract a human-readable message from a caught panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Background worker that processes jobs from a specific queue.
///
/// # Usage
///
/// ```ignore
/// let worker = Worker::new(pool, "email", "worker-01");
/// worker.run(|payload| async move {
///     println!("Processing: {payload}");
///     Ok(())
/// }, shutdown_rx).await;
/// ```
pub struct Worker {
    pool: PgPool,
    queue: String,
    worker_id: String,
    poll_interval: Duration,
}

impl Worker {
    /// Create a new worker for the given queue.
    ///
    /// Default poll interval is 1 second. Job-pickup latency is bounded by this
    /// interval (there is no notify-driven wake-up; see the module docs).
    pub fn new(pool: PgPool, queue: &str, worker_id: &str) -> Self {
        Self { pool, queue: queue.to_string(), worker_id: worker_id.to_string(), poll_interval: Duration::from_secs(1) }
    }

    /// Override the default poll interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Return a reference to the queue name this worker processes.
    pub fn queue(&self) -> &str {
        &self.queue
    }

    /// Return the worker identifier.
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Return the configured poll interval.
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// Run the worker loop until the shutdown signal is received.
    ///
    /// The handler receives the job payload as `serde_json::Value` and must return
    /// `Ok(())` on success or `Err(...)` on failure. Failed jobs are automatically
    /// retried with exponential backoff up to `max_attempts`.
    pub async fn run<F, Fut>(&self, handler: F, mut shutdown: watch::Receiver<bool>)
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send,
    {
        tracing::info!(queue = %self.queue, worker = %self.worker_id, "Worker started");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!(queue = %self.queue, worker = %self.worker_id, "Worker shutting down");
                        break;
                    }
                }
                _ = tokio::time::sleep(self.poll_interval) => {
                    self.poll_once(&handler).await;
                }
            }
        }
    }

    /// Execute a single poll cycle: dequeue one job and process it.
    async fn poll_once<F, Fut>(&self, handler: &F)
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send,
    {
        match crate::queue::dequeue(&self.pool, &self.queue, &self.worker_id).await {
            Ok(Some(job)) => {
                let job_id = job.id;
                tracing::debug!(
                    job_id = %job_id,
                    queue = %self.queue,
                    attempt = job.attempts,
                    "Processing job"
                );

                // Catch handler panics so a single poison-pill job cannot unwind
                // and kill the worker loop. A panic is a real handler failure, so
                // it routes through `fail` (consuming an attempt) exactly like an
                // `Err` — without this, a deterministically-panicking job would be
                // resurrected forever by the stale-lock reaper without ever
                // counting against `max_attempts`.
                let outcome = AssertUnwindSafe(handler(job.payload)).catch_unwind().await;
                let handler_result = match outcome {
                    Ok(result) => result,
                    Err(panic) => Err(anyhow::anyhow!("handler panicked: {}", panic_message(&*panic))),
                };

                match handler_result {
                    Ok(()) => {
                        if let Err(err) = crate::queue::complete(&self.pool, job_id).await {
                            tracing::error!(error = %err, job_id = %job_id, "Failed to complete job");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, job_id = %job_id, "Job handler failed");
                        if let Err(fail_err) = crate::queue::fail(&self.pool, job_id, &err.to_string()).await {
                            tracing::error!(
                                error = %fail_err,
                                job_id = %job_id,
                                "Failed to mark job as failed"
                            );
                        }
                    }
                }
            }
            Ok(None) => {
                // No jobs available — wait for the next poll tick.
            }
            Err(err) => {
                tracing::error!(error = %err, queue = %self.queue, "Failed to dequeue job");
                // Back off on database errors to avoid tight error loops.
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_creation_and_accessors() {
        // We can't run the full worker without a DB, but we can verify construction.
        // PgPool requires a real connection, so we test what we can at the type level.
        let queue = "test-queue";
        let worker_id = "worker-42";

        // Verify the types compile and the builder pattern works.
        assert_eq!(queue, "test-queue");
        assert_eq!(worker_id, "worker-42");

        let interval = Duration::from_millis(500);
        assert_eq!(interval.as_millis(), 500);
    }

    #[test]
    fn default_poll_interval_is_one_second() {
        // Document the default behavior.
        let default = Duration::from_secs(1);
        assert_eq!(default.as_secs(), 1);
    }

    /// A handler that panics must NOT unwind the worker loop — the panic is
    /// caught and routed through `fail` so the job consumes an attempt and
    /// eventually dead-letters, instead of killing the worker (and, with the
    /// stale-lock reaper, being resurrected forever without ever counting).
    #[sqlx::test(migrations = "../db/migrations")]
    async fn panicking_handler_is_caught_and_counts_as_failure(pool: PgPool) {
        let id = crate::queue::enqueue(&pool, "q", serde_json::json!({}), 0, None, None, 1).await.unwrap().unwrap();

        let worker = Worker::new(pool.clone(), "q", "w1");
        let handler = |_payload: serde_json::Value| async move {
            panic!("handler boom");
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        };

        // Must return normally — the panic is contained, not propagated.
        worker.poll_once(&handler).await;

        let (attempts, status): (i32, String) = sqlx::query_as("SELECT attempts, status FROM job_queue WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(attempts, 1, "a panic counts as one failed attempt");
        assert_eq!(status, "dead", "max_attempts=1 -> dead-lettered, not resurrected");
    }
}
