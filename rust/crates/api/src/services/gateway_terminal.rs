use agentforge_core::{AgentId, AppResult, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::gateway::GatewayTerminalAttachTarget;
use crate::repositories::agent::AgentRepository;

pub(crate) struct GatewayTerminalService {
    pool: PgPool,
    agents: AgentRepository,
}

impl GatewayTerminalService {
    pub(crate) fn new(pool: PgPool, agents: AgentRepository) -> Self {
        Self { pool, agents }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(pool.clone(), AgentRepository::new(pool))
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

    /// Fence each browser-terminal input against container replacement. The
    /// short DB lease covers the write-to-Docker handoff; normal hook events
    /// keep `agents.status=working` once the CLI begins actual work.
    pub(crate) async fn admit_input(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
        expected_container_id: &str,
    ) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id).await?;
        let admitted = AgentRepository::renew_interactive_lease_in_tx(
            &mut tx,
            scope,
            AgentId::from(agent_id),
            expected_container_id,
        )
        .await?;
        tx.commit().await?;
        Ok(admitted)
    }
}
