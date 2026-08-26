//! Analytics service — event tracking and aggregation.

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::AnalyticsEvent;
use sqlx::PgPool;

pub(crate) use crate::domain::observability::PricingTable;
pub(crate) use crate::domain::observability::analytics_data_response;
use crate::domain::observability::{
    AgentReliabilityItem, AgentReliabilityReport, AgentReliabilityWindow, AgentUsageItem, AgentUsageReport,
    AnalyticsEventName, AnalyticsListPage, AnalyticsSummary,
};
use crate::repositories::analytics::AnalyticsRepository;

/// Business logic layer for analytics operations.
pub struct AnalyticsService {
    repo: AnalyticsRepository,
}

impl AnalyticsService {
    pub fn new(repo: AnalyticsRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(AnalyticsRepository::new(pool))
    }

    /// Track a new analytics event.
    pub async fn track(
        &self,
        scope: &TenantScope,
        event_name: &str,
        properties: &serde_json::Value,
    ) -> AppResult<AnalyticsEvent> {
        let event_name = AnalyticsEventName::parse(event_name)?;
        self.repo.track(scope, event_name.value(), properties).await
    }

    /// List analytics events with optional filters.
    pub async fn list(
        &self,
        scope: &TenantScope,
        event_name: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<Vec<AnalyticsEvent>> {
        let page = AnalyticsListPage::new(limit, offset);
        self.repo.list(scope, event_name, page.limit(), page.offset()).await
    }

    /// Get aggregate summary stats.
    pub(crate) async fn summary(&self, scope: &TenantScope) -> AppResult<AnalyticsSummary> {
        self.repo.summary(scope).await
    }

    /// Per-agent work reliability over a rolling window of finished runs.
    pub(crate) async fn agent_reliability(
        &self,
        scope: &TenantScope,
        hours: Option<i64>,
    ) -> AppResult<AgentReliabilityReport> {
        let window = AgentReliabilityWindow::normalize(hours);
        let rows = self.repo.agent_reliability_rows(scope, window.hours()).await?;
        let agents = rows
            .into_iter()
            .map(|(agent_id, name, total, succeeded)| AgentReliabilityItem {
                agent_id: AgentId::from(agent_id),
                name,
                total,
                succeeded,
                failed: total - succeeded,
                success_rate: if total > 0 { succeeded as f64 / total as f64 } else { 0.0 },
            })
            .collect();
        Ok(AgentReliabilityReport { window_hours: window.hours(), agents })
    }

    /// Per-agent LLM usage over a rolling window with each agent's share of
    /// the window's tokens and the estimated cost when `LLM_PRICING` rates
    /// are configured.
    pub(crate) async fn agent_usage(
        &self,
        scope: &TenantScope,
        hours: Option<i64>,
        pricing: Option<&PricingTable>,
    ) -> AppResult<AgentUsageReport> {
        let window = AgentReliabilityWindow::normalize(hours);
        let rows = self.repo.agent_usage_rows(scope, window.hours()).await?;
        let mut agents: Vec<AgentUsageItem> = Vec::new();
        for row in rows {
            let estimated_cost =
                pricing.and_then(|table| table.cost_usd(row.model.as_deref(), row.tokens_in, row.tokens_out));
            let agent_id = row.agent_id;
            let tokens_in = row.tokens_in;
            let tokens_out = row.tokens_out;
            let requests = row.requests;
            match agents.iter_mut().find(|entry| entry.agent_id == AgentId::from(agent_id)) {
                Some(entry) => {
                    entry.requests += requests;
                    entry.tokens_in += tokens_in;
                    entry.tokens_out += tokens_out;
                    entry.estimated_cost = Some(entry.estimated_cost.unwrap_or(0.0) + estimated_cost.unwrap_or(0.0));
                }
                None => agents.push(AgentUsageItem {
                    agent_id: AgentId::from(agent_id),
                    name: row.name,
                    requests,
                    tokens_in,
                    tokens_out,
                    total_tokens: tokens_in + tokens_out,
                    share: 0.0,
                    estimated_cost,
                }),
            }
        }
        let total_tokens: i64 = agents.iter().map(|entry| entry.total_tokens).sum();
        for entry in &mut agents {
            entry.total_tokens = entry.tokens_in + entry.tokens_out;
            entry.share = if total_tokens > 0 { entry.total_tokens as f64 / total_tokens as f64 } else { 0.0 };
        }
        Ok(AgentUsageReport { window_hours: window.hours(), pricing_configured: pricing.is_some(), agents })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::observability::{
        AgentReliabilityItem, AgentReliabilityReport, AgentReliabilityWindow, AgentUsageReport, AnalyticsEventName,
        AnalyticsListPage, PricingTable,
    };
    use agentforge_core::AgentId;
    use uuid::Uuid;

    #[test]
    fn empty_event_name_rejected() {
        assert!(AnalyticsEventName::parse("").is_err());
    }

    #[test]
    fn limit_capped_at_1000() {
        assert_eq!(AnalyticsListPage::new(Some(5000_i64), None).limit(), 1000);
    }

    #[test]
    fn default_limit_is_50() {
        assert_eq!(AnalyticsListPage::new(None, None).limit(), 50);
    }

    #[test]
    fn offset_cannot_be_negative() {
        assert_eq!(AnalyticsListPage::new(None, Some(-10_i64)).offset(), 0);
    }

    #[test]
    fn reliability_window_defaults_to_30_days() {
        assert_eq!(AgentReliabilityWindow::normalize(None).hours(), 720);
    }

    #[test]
    fn reliability_window_is_clamped_to_bounds() {
        assert_eq!(AgentReliabilityWindow::normalize(Some(1)).hours(), 1);
        assert_eq!(AgentReliabilityWindow::normalize(Some(0)).hours(), 1);
        assert_eq!(AgentReliabilityWindow::normalize(Some(100_000)).hours(), 8_760);
    }

    #[test]
    fn usage_report_serializes_camel_case() {
        let report = AgentUsageReport {
            window_hours: 720,
            pricing_configured: true,
            agents: vec![crate::domain::observability::AgentUsageItem {
                agent_id: AgentId::from(Uuid::nil()),
                name: Some("Worker".to_string()),
                requests: 2,
                tokens_in: 250,
                tokens_out: 50,
                total_tokens: 300,
                share: 1.0,
                estimated_cost: Some(0.01),
            }],
        };
        let value = serde_json::to_value(&report).expect("serialize usage");
        assert_eq!(value["windowHours"].as_i64(), Some(720));
        assert_eq!(value["pricingConfigured"].as_bool(), Some(true));
        assert_eq!(value["agents"][0]["tokensIn"].as_i64(), Some(250));
        assert_eq!(value["agents"][0]["tokensOut"].as_i64(), Some(50));
        assert_eq!(value["agents"][0]["totalTokens"].as_i64(), Some(300));
        assert_eq!(value["agents"][0]["share"].as_f64(), Some(1.0));
        assert_eq!(value["agents"][0]["requests"].as_i64(), Some(2));
        assert_eq!(value["agents"][0]["estimatedCost"].as_f64(), Some(0.01));
    }

    #[test]
    fn pricing_table_parses_and_matches_models_case_insensitively() {
        let table = PricingTable::parse(r#"{"GPT-4o":{"input":2.5,"output":10.0}}"#).expect("parse");
        let cost = table.cost_usd(Some("gpt-4o"), 100_000, 50_000).expect("rate");
        let expected = 2.5 * 0.1 + 10.0 * 0.05;
        assert!((cost - expected).abs() < 1e-9, "cost was {cost}");
        assert!(table.cost_usd(Some("unknown-model"), 100, 100).is_none());
        assert!(table.cost_usd(None, 100, 100).is_none());
    }

    #[test]
    fn pricing_table_rejects_malformed_json() {
        assert!(PricingTable::parse(r#"not json"#).is_err());
        assert!(PricingTable::parse(r#"[]"#).is_err());
        assert!(PricingTable::parse(r#"{"m":{"input":-1,"output":10}}"#).is_err());
    }

    #[test]
    fn reliability_report_serializes_camel_case() {
        let report = AgentReliabilityReport {
            window_hours: 720,
            agents: vec![AgentReliabilityItem {
                agent_id: AgentId::from(Uuid::nil()),
                name: Some("Reliable".to_string()),
                total: 5,
                succeeded: 4,
                failed: 1,
                success_rate: 0.8,
            }],
        };
        let value = serde_json::to_value(&report).expect("serialize report");
        assert_eq!(value["windowHours"].as_i64(), Some(720));
        assert_eq!(value["agents"][0]["agentId"].as_str(), Some("00000000-0000-0000-0000-000000000000"));
        assert_eq!(value["agents"][0]["successRate"].as_f64(), Some(0.8));
        assert_eq!(value["agents"][0]["success_rate"], serde_json::Value::Null);
    }
}
