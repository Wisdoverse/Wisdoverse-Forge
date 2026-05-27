//! Agent container lifecycle service.
//!
//! Owns Docker-backed lifecycle orchestration for existing agent containers.

use std::sync::Arc;

use agentforge_core::{AgentId, AgentStatus, AppResult, TenantScope};
use agentforge_platform::{ContainerState, DockerClient};
use sqlx::PgPool;

use crate::domain::agent::{
    AgentContainerLifecyclePolicy, AgentContainerRuntimePolicy, AgentContainerRuntimeState, AgentOwnerPolicy,
    AgentRestartPlan, ContainerAgent,
};
use crate::repositories::agent::AgentRepository;
use crate::services::agent::AgentService;

pub struct AgentContainerLifecycleService {
    agents: AgentService,
    docker: Option<Arc<DockerClient>>,
}

impl AgentContainerLifecycleService {
    pub fn new(agents: AgentRepository, docker: Option<Arc<DockerClient>>) -> Self {
        Self { agents: AgentService::new(agents), docker }
    }

    pub(crate) fn from_runtime(pool: PgPool, docker: Option<Arc<DockerClient>>) -> Self {
        Self::new(AgentRepository::new(pool), docker)
    }

    pub async fn restart(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        // Owner check fires FIRST so non-owner intra-org callers get a uniform
        // 403 that does NOT disclose the runtime kind. The typestate check (which
        // would 422 with runtime-kind info) comes after.
        let aggregate = self.agents.find_aggregate(scope, agent_id.as_uuid()).await?;
        AgentOwnerPolicy::require_owner(scope.user_id().as_uuid(), aggregate.user_id())?;
        let container = ContainerAgent::try_from(aggregate)
            .map_err(|r| r.into_app_error("Restart"))?;
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::lifecycle_docker_unavailable)?;
        let inner = container.inner();
        let container_id = AgentContainerLifecyclePolicy::restart_container_id(inner.container_id.as_deref())?;

        let container_info = match docker.inspect_container(container_id).await {
            Ok(info) => info,
            Err(err) if err.is_not_found() => {
                tracing::warn!(
                    error = %err,
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "agent restart found a stale container reference"
                );
                self.agents.clear_container(scope, agent_id).await?;
                return Err(AgentContainerLifecyclePolicy::stale_container_reference_error().into());
            }
            Err(err) => return Err(AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect", err).into()),
        };

        match AgentContainerLifecyclePolicy::restart_plan(restart_state_from_container_state(container_info.status)) {
            AgentRestartPlan::StopThenStart => {
                docker
                    .stop_container(container_id, 10)
                    .await
                    .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("stop", err))?;
                docker
                    .start_container(container_id)
                    .await
                    .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("start", err))?;
            }
            AgentRestartPlan::StartOnly => {
                tracing::info!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    status = ?container_info.status,
                    "agent restart found a non-running container; starting it directly"
                );
                docker
                    .start_container(container_id)
                    .await
                    .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("start", err))?;
            }
        }

        Ok(())
    }

    pub async fn resume(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        // Owner check before typestate check — same ordering as restart.
        let aggregate = self.agents.find_aggregate(scope, agent_id.as_uuid()).await?;
        AgentOwnerPolicy::require_owner(scope.user_id().as_uuid(), aggregate.user_id())?;
        let container = ContainerAgent::try_from(aggregate)
            .map_err(|r| r.into_app_error("Resume"))?;
        let inner = container.inner();
        let container_id = AgentContainerLifecyclePolicy::resume_container_id(inner.container_id.as_deref())?;

        if let Some(docker) = &self.docker {
            docker.start_container(container_id).await.map_err(AgentContainerRuntimePolicy::resume_failed)?;
        }

        self.agents.update_status(scope, agent_id, AgentStatus::Idle).await?;
        Ok(())
    }
}

fn restart_state_from_container_state(state: ContainerState) -> AgentContainerRuntimeState {
    match state {
        ContainerState::Running => AgentContainerRuntimeState::Running,
        ContainerState::Created
        | ContainerState::Paused
        | ContainerState::Stopped
        | ContainerState::Dead
        | ContainerState::Unknown => AgentContainerRuntimeState::NotRunning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_state_maps_only_running_to_stop_then_start() {
        assert_eq!(
            AgentContainerLifecyclePolicy::restart_plan(restart_state_from_container_state(ContainerState::Running)),
            AgentRestartPlan::StopThenStart
        );

        for state in [
            ContainerState::Created,
            ContainerState::Paused,
            ContainerState::Stopped,
            ContainerState::Dead,
            ContainerState::Unknown,
        ] {
            assert_eq!(
                AgentContainerLifecyclePolicy::restart_plan(restart_state_from_container_state(state)),
                AgentRestartPlan::StartOnly
            );
        }
    }
}
