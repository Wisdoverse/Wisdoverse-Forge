//! Self-fix PR-bridge worker. Dequeues `self_fix_pr` jobs (enqueued in-tx by
//! `complete_task`) and drives `SelfFixService::open_pr`. Mirrors the
//! dequeue/poll/shutdown shape of `ProjectCloneWorker`.

use std::sync::Arc;

use agentforge_core::{AppResult, OrgId, SELF_FIX_PR_QUEUE, SelfFixPrJob, TenantScope, UserId};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::services::self_fix::SelfFixService;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

pub struct SelfFixPrWorker {
    pool: PgPool,
    service: Arc<SelfFixService>,
    worker_id: String,
}

impl SelfFixPrWorker {
    pub fn new(pool: PgPool, service: Arc<SelfFixService>) -> Self {
        Self { pool, service, worker_id: format!("self-fix-pr-{}", Uuid::now_v7()) }
    }

    /// Dequeue loop until shutdown. pg_notify is wake-only; poll on the interval.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(worker_id = %self.worker_id, "self_fix_pr worker starting");
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!(worker_id = %self.worker_id, "self_fix_pr worker shutting down");
                        return;
                    }
                }
                result = self.dequeue_and_process() => match result {
                    Ok(true) => {}
                    Ok(false) => tokio::time::sleep(POLL_INTERVAL).await,
                    Err(err) => {
                        tracing::warn!(error = %err, "self_fix_pr worker tick failed");
                        tokio::time::sleep(POLL_INTERVAL).await;
                    }
                },
            }
        }
    }

    /// Process at most one job. Returns `Ok(true)` if a job was claimed.
    pub async fn dequeue_and_process(&self) -> AppResult<bool> {
        let job = agentforge_jobs::queue::dequeue(&self.pool, SELF_FIX_PR_QUEUE, &self.worker_id)
            .await
            .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
        let Some(job) = job else {
            return Ok(false);
        };

        let payload: SelfFixPrJob = match serde_json::from_value(job.payload.clone()) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "self_fix_pr payload undecodable; dropping");
                agentforge_jobs::queue::complete(&self.pool, job.id)
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
                return Ok(true);
            }
        };

        // Background workers act on behalf of the job's org; the user axis is
        // unused by `open_pr` (org-scoped), so use a nil placeholder
        // (precedent: project_clone_worker.rs:1372).
        let scope = TenantScope::new(OrgId::from(payload.org_id), UserId::from(Uuid::nil()));

        match self.service.open_pr(&scope, payload.task_id).await {
            Ok(outcome) => {
                metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "opened").increment(1);
                tracing::info!(task_id = %payload.task_id, pr = outcome.pr_number, "self-fix PR opened");
                agentforge_jobs::queue::complete(&self.pool, job.id)
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
            }
            Err(err) => {
                metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "failed").increment(1);
                tracing::warn!(task_id = %payload.task_id, error = %err, "self-fix PR open failed");
                agentforge_jobs::queue::fail(&self.pool, job.id, &err.to_string())
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
            }
        }
        Ok(true)
    }
}

/// Describe metric series so they are present from the first scrape.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_self_fix_pr_total",
        "Self-fix PR-bridge outcomes, labeled opened|failed"
    );
    metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "opened").increment(0);
    metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "failed").increment(0);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::PgPool;
    use uuid::Uuid;

    use super::SelfFixPrWorker;

    fn build_service(pool: &PgPool) -> crate::services::self_fix::SelfFixService {
        let config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
        let container_control =
            crate::services::agent_container_control::AgentContainerControlService::from_runtime(
                pool.clone(),
                &config,
                crate::domain::context::ContextFeatureFlags::default(),
                None, // encryption_key
                None, // docker
                None, // auth_callout
            );
        crate::services::self_fix::SelfFixService::new(
            crate::repositories::orchestration::OrchestrationTaskRepository::new(pool.clone()),
            crate::repositories::agent::AgentRepository::new(pool.clone()),
            container_control,
            None,    // github not configured -> open_pr returns github_not_configured BEFORE touching container_control
            crate::services::agent_workspace::workspace_root_from_env(),
            crate::services::self_fix::import::ImportLimits::default(),
        )
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn worker_fails_job_when_github_unconfigured(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();

        // Seed org
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(format!("Org {org_id}"))
            .bind(format!("org-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed org");

        // Seed workspace
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed workspace");

        // Seed user
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");

        // Seed a self_fix task (no agent assignment needed; open_pr errors on github=None first)
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by, self_fix) \
             VALUES ($1, $2, 't', 'working', $3, true)",
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed task");

        // Enqueue a self_fix_pr job
        agentforge_jobs::queue::enqueue(
            &pool,
            agentforge_core::SELF_FIX_PR_QUEUE,
            serde_json::to_value(agentforge_core::SelfFixPrJob { task_id, org_id }).unwrap(),
            0,
            None,
            Some(&task_id.to_string()),
            5,
        )
        .await
        .unwrap();

        let worker = SelfFixPrWorker::new(pool.clone(), Arc::new(build_service(&pool)));
        let processed = worker.dequeue_and_process().await.unwrap();
        assert!(processed, "a queued job should be processed");

        // github=None => open_pr errors => queue::fail bumps attempts; the job is
        // not silently completed/deleted.
        let (status, attempts): (String, i32) = sqlx::query_as(
            "SELECT status, attempts FROM job_queue WHERE queue = $1 AND unique_key = $2",
        )
        .bind(agentforge_core::SELF_FIX_PR_QUEUE)
        .bind(task_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        let _ = status;
        assert!(attempts >= 1, "attempt count must increment on failure");
    }
}
