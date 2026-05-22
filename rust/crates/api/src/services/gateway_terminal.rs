use agentforge_core::{AgentId, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::gateway::GatewayTerminalAttachTarget;
use crate::repositories::agent::AgentRepository;

pub(crate) struct GatewayTerminalService {
    agents: AgentRepository,
}

impl GatewayTerminalService {
    pub(crate) fn new(agents: AgentRepository) -> Self {
        Self { agents }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(AgentRepository::new(pool))
    }

    pub(crate) async fn attach_target(&self, scope: &TenantScope, agent_id: Uuid) -> GatewayTerminalAttachTarget {
        match self.agents.find_by_id(scope, AgentId::from(agent_id)).await {
            Ok(agent) => agent
                .container_id
                .map(GatewayTerminalAttachTarget::ready)
                .unwrap_or_else(GatewayTerminalAttachTarget::missing_container),
            Err(err) => GatewayTerminalAttachTarget::lookup_failed(&err.kind),
        }
    }
}
