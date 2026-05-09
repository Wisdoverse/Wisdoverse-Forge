use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::auth::AgentDirectory;
use crate::review::{self, ReviewFilter, ReviewState};
use crate::task::{self, TaskFilter, TaskState};

use super::store::Store;
use super::{AgentMetric, DashboardMetrics, ReviewLatency};

const PAGE_SIZE: usize = 1_000;
const AGENT_LIMIT: usize = 20;

pub struct MemoryMetricsStore {
    task_store: Arc<dyn task::Store>,
    review_store: Arc<dyn review::Store>,
    agent_directory: Option<Arc<dyn AgentDirectory>>,
}

impl MemoryMetricsStore {
    pub fn new(
        task_store: Arc<dyn task::Store>,
        review_store: Arc<dyn review::Store>,
        agent_directory: Option<Arc<dyn AgentDirectory>>,
    ) -> Self {
        Self { task_store, review_store, agent_directory }
    }
}

#[async_trait]
impl Store for MemoryMetricsStore {
    async fn dashboard(&self, org_id: &str) -> anyhow::Result<DashboardMetrics> {
        let tasks = load_all_tasks(self.task_store.clone(), org_id).await?;
        let reviews = load_all_reviews(self.review_store.clone(), org_id).await?;
        let today = chrono::Utc::now().date_naive();
        let active_agents = tasks
            .iter()
            .filter(|task| matches!(task.state, TaskState::Assigned | TaskState::Working))
            .filter_map(|task| task.agentforge_session_id.as_deref())
            .collect::<HashSet<_>>()
            .len();

        Ok(DashboardMetrics {
            active_tasks: tasks
                .iter()
                .filter(|task| !matches!(task.state, TaskState::Completed | TaskState::Failed))
                .count(),
            completed_today: tasks
                .iter()
                .filter(|task| matches!(task.state, TaskState::Completed))
                .filter(|task| task.updated_at.date_naive() == today)
                .count(),
            active_agents,
            pending_reviews: reviews.iter().filter(|review| matches!(review.state, ReviewState::Pending)).count(),
        })
    }

    async fn agent_leaderboard(&self, org_id: &str) -> anyhow::Result<Vec<AgentMetric>> {
        let tasks = load_all_tasks(self.task_store.clone(), org_id).await?;
        let mut grouped: HashMap<String, Vec<task::Task>> = HashMap::new();
        for task in tasks.into_iter().filter(|task| task.agentforge_session_id.is_some()) {
            let session_id = task.agentforge_session_id.clone().expect("checked is_some");
            grouped.entry(session_id).or_default().push(task);
        }

        let mut agents = Vec::with_capacity(grouped.len());
        for (session_id, tasks) in grouped {
            let completed: Vec<&task::Task> =
                tasks.iter().filter(|task| matches!(task.state, TaskState::Completed)).collect();
            let avg_duration_ms = if completed.is_empty() {
                0
            } else {
                completed.iter().map(|task| (task.updated_at - task.created_at).num_milliseconds()).sum::<i64>()
                    / completed.len() as i64
            };
            let metadata = match self.agent_directory.as_ref() {
                Some(directory) => directory.get_by_session(org_id, &session_id).await?,
                None => None,
            };
            let display_name = metadata
                .as_ref()
                .map(|participant| participant.display_name.clone())
                .unwrap_or_else(|| session_id.clone());
            let provider = metadata
                .as_ref()
                .map(|participant| participant.provider.clone())
                .unwrap_or_else(|| "unknown".to_string());
            agents.push(AgentMetric {
                participant_id: session_id.clone(),
                display_name,
                provider,
                tasks_completed: completed.len(),
                avg_duration_ms,
                success_rate: if tasks.is_empty() { 0.0 } else { completed.len() as f64 / tasks.len() as f64 },
            });
        }

        agents.sort_by(|left, right| {
            right
                .tasks_completed
                .cmp(&left.tasks_completed)
                .then_with(|| left.participant_id.cmp(&right.participant_id))
        });
        agents.truncate(AGENT_LIMIT);
        Ok(agents)
    }

    async fn review_latency(&self, org_id: &str) -> anyhow::Result<ReviewLatency> {
        let reviews = load_all_reviews(self.review_store.clone(), org_id).await?;
        let mut samples: Vec<f64> = reviews
            .iter()
            .filter(|review| {
                matches!(review.state, ReviewState::Approved | ReviewState::ChangesRequested | ReviewState::Rejected)
            })
            .map(|review| (review.updated_at - review.created_at).num_milliseconds() as f64 / 3_600_000.0)
            .collect();
        samples.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

        Ok(ReviewLatency {
            avg_hours: if samples.is_empty() { 0.0 } else { samples.iter().sum::<f64>() / samples.len() as f64 },
            p50_hours: percentile_cont(&samples, 0.50),
            p95_hours: percentile_cont(&samples, 0.95),
        })
    }
}

pub struct PgMetricsStore {
    pool: PgPool,
}

impl PgMetricsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgMetricsStore {
    async fn dashboard(&self, org_id: &str) -> anyhow::Result<DashboardMetrics> {
        let active_tasks: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE org_id = $1 AND state NOT IN ('completed', 'failed')")
                .bind(org_id)
                .fetch_one(&self.pool)
                .await
                .context("count active tasks")?;

        let completed_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks WHERE state = 'completed' AND org_id = $1 AND updated_at >= CURRENT_DATE",
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .context("count completed today")?;

        let active_agents: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT agentforge_session_id) FROM tasks              WHERE state IN ('assigned', 'working') AND org_id = $1 AND agentforge_session_id IS NOT NULL",
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .context("count active agents")?;

        let pending_reviews: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM code_reviews WHERE state = 'pending' AND org_id = $1")
                .bind(org_id)
                .fetch_one(&self.pool)
                .await
                .context("count pending reviews")?;

        Ok(DashboardMetrics {
            active_tasks: active_tasks as usize,
            completed_today: completed_today as usize,
            active_agents: active_agents as usize,
            pending_reviews: pending_reviews as usize,
        })
    }

    async fn agent_leaderboard(&self, org_id: &str) -> anyhow::Result<Vec<AgentMetric>> {
        let rows = sqlx::query(
            "SELECT t.agentforge_session_id,                     COALESCE(p.display_name, t.agentforge_session_id) AS display_name,                     COALESCE(p.agent_provider, 'unknown') AS provider,                     COUNT(*) FILTER (WHERE t.state = 'completed') AS tasks_completed,                     COALESCE(AVG(EXTRACT(EPOCH FROM (t.updated_at - t.created_at)) * 1000)                       FILTER (WHERE t.state = 'completed'), 0)::bigint AS avg_duration_ms,                     CASE WHEN COUNT(*) = 0 THEN 0                          ELSE COUNT(*) FILTER (WHERE t.state = 'completed')::float / COUNT(*)::float                     END AS success_rate              FROM tasks t              LEFT JOIN participants p ON p.agent_session_id = t.agentforge_session_id              WHERE t.org_id = $1 AND t.agentforge_session_id IS NOT NULL              GROUP BY t.agentforge_session_id, p.display_name, p.agent_provider              ORDER BY tasks_completed DESC              LIMIT 20"
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await
        .context("agent leaderboard query")?;

        let mut agents = Vec::with_capacity(rows.len());
        for row in rows {
            let participant_id = row
                .try_get::<Option<String>, _>("agentforge_session_id")?
                .ok_or_else(|| anyhow::anyhow!("agentforge_session_id missing"))?;
            let tasks_completed: i64 = row.try_get("tasks_completed")?;
            let avg_duration_ms: i64 = row.try_get("avg_duration_ms")?;
            let success_rate: f64 = row.try_get("success_rate")?;
            agents.push(AgentMetric {
                participant_id,
                display_name: row.try_get("display_name")?,
                provider: row.try_get("provider")?,
                tasks_completed: tasks_completed as usize,
                avg_duration_ms,
                success_rate,
            });
        }
        Ok(agents)
    }

    async fn review_latency(&self, org_id: &str) -> anyhow::Result<ReviewLatency> {
        let row = sqlx::query(
            "SELECT COALESCE(AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600), 0) AS avg_hours,                     COALESCE(PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600), 0) AS p50_hours,                     COALESCE(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600), 0) AS p95_hours              FROM code_reviews              WHERE org_id = $1 AND state IN ('approved', 'changes_requested', 'rejected')"
        )
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .context("review latency query")?;

        Ok(ReviewLatency {
            avg_hours: row.try_get("avg_hours")?,
            p50_hours: row.try_get("p50_hours")?,
            p95_hours: row.try_get("p95_hours")?,
        })
    }
}

async fn load_all_tasks(store: Arc<dyn task::Store>, org_id: &str) -> anyhow::Result<Vec<task::Task>> {
    let mut tasks = Vec::new();
    let mut offset = 0;
    loop {
        let batch = store
            .list(TaskFilter { org_id: org_id.to_string(), state: None, assigned_to: None, limit: PAGE_SIZE, offset })
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let count = batch.len();
        tasks.extend(batch);
        if count < PAGE_SIZE {
            break;
        }
        offset += count;
    }
    Ok(tasks)
}

async fn load_all_reviews(store: Arc<dyn review::Store>, org_id: &str) -> anyhow::Result<Vec<review::CodeReview>> {
    let mut reviews = Vec::new();
    let mut offset = 0;
    loop {
        let batch = store
            .list(ReviewFilter { org_id: org_id.to_string(), task_id: None, state: None, limit: PAGE_SIZE, offset })
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let count = batch.len();
        reviews.extend(batch);
        if count < PAGE_SIZE {
            break;
        }
        offset += count;
    }
    Ok(reviews)
}

fn percentile_cont(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }

    let position = percentile.clamp(0.0, 1.0) * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        let fraction = position - lower as f64;
        values[lower] + (values[upper] - values[lower]) * fraction
    }
}
