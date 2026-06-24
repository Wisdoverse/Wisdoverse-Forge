//! Agent message service.
//!
//! Keeps chat-history ownership checks, pagination policy, and persistence
//! coordination out of HTTP handlers.

use chrono::{DateTime, Utc};

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::AgentMessage;
use sqlx::PgPool;

use crate::domain::agent::AgentMessagePage;
use crate::repositories::agent::{AgentRepository, MessageRepository};

pub(crate) struct AgentMessageService {
    agents: AgentRepository,
    messages: MessageRepository,
}

impl AgentMessageService {
    pub(crate) fn new(agents: AgentRepository, messages: MessageRepository) -> Self {
        Self { agents, messages }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(AgentRepository::new(pool.clone()), MessageRepository::new(pool))
    }

    pub(crate) async fn list(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> AppResult<(Vec<AgentMessage>, bool)> {
        self.agents.find_by_id(scope, agent_id).await?;
        let page = AgentMessagePage::new(limit);
        let rows = self.messages.list(scope, agent_id, page.fetch_limit(), before).await?;
        Ok(page.split_has_more(rows))
    }

    pub(crate) async fn delete_all(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<u64> {
        self.agents.find_by_id(scope, agent_id).await?;
        self.messages.delete_all_by_agent(scope, agent_id).await
    }
}
