use async_trait::async_trait;

use super::{AgentMetric, DashboardMetrics, ReviewLatency};

#[async_trait]
pub trait Store: Send + Sync {
    async fn dashboard(&self, org_id: &str) -> anyhow::Result<DashboardMetrics>;
    async fn agent_leaderboard(&self, org_id: &str) -> anyhow::Result<Vec<AgentMetric>>;
    async fn review_latency(&self, org_id: &str) -> anyhow::Result<ReviewLatency>;
}
