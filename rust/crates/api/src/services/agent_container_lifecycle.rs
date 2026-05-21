//! Agent container lifecycle service.
//!
//! Owns Docker-backed lifecycle orchestration for existing agent containers.

use std::sync::Arc;

use agentforge_core::{AgentId, AgentStatus, AppResult, ErrorKind, TenantScope};
use agentforge_platform::{ContainerState, DockerClient};
use sqlx::PgPool;

use crate::domain::agent::{AgentContainerLifecyclePolicy, AgentContainerRuntimeState, AgentRestartPlan};
use crate::repositories::agent::AgentRepository;
use crate::services::agent::AgentService;

pub(crate) struct AgentContainerLifecycleService {
    agents: AgentService,
    docker: Option<Arc<DockerClient>>,
}

impl AgentContainerLifecycleService {
    pub(crate) fn new(agents: AgentRepository, docker: Option<Arc<DockerClient>>) -> Self {
        Self { agents: AgentService::new(agents), docker }
    }

    pub(crate) fn from_runtime(pool: PgPool, docker: Option<Arc<DockerClient>>) -> Self {
        Self::new(AgentRepository::new(pool), docker)
    }

    pub(crate) async fn restart(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        let docker = self.docker.as_ref().ok_or_else(docker_unavailable)?;
        let agent = self.agents.get(scope, agent_id).await?;

        if agent.cli_tool.is_none() {
            return Err(ErrorKind::Validation("agent is not container-backed".into()).into());
        }

        let container_id =
            agent.container_id.as_ref().ok_or_else(|| ErrorKind::Validation("agent has no container".into()))?;

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
                return Err(ErrorKind::Validation(
                    "agent container is no longer available; start the agent again".into(),
                )
                .into());
            }
            Err(err) => return Err(docker_lifecycle_unavailable("inspect", err).into()),
        };

        match AgentContainerLifecyclePolicy::restart_plan(restart_state_from_container_state(container_info.status)) {
            AgentRestartPlan::StopThenStart => {
                docker
                    .stop_container(container_id, 10)
                    .await
                    .map_err(|err| docker_lifecycle_unavailable("stop", err))?;
                docker.start_container(container_id).await.map_err(|err| docker_lifecycle_unavailable("start", err))?;
            }
            AgentRestartPlan::StartOnly => {
                tracing::info!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    status = ?container_info.status,
                    "agent restart found a non-running container; starting it directly"
                );
                docker.start_container(container_id).await.map_err(|err| docker_lifecycle_unavailable("start", err))?;
            }
        }

        Ok(())
    }

    pub(crate) async fn resume(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        let agent = self.agents.get(scope, agent_id).await?;
        let container_id = agent
            .container_id
            .as_ref()
            .ok_or_else(|| ErrorKind::Validation("agent has no container to resume".into()))?;

        if let Some(docker) = &self.docker {
            docker
                .start_container(container_id)
                .await
                .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("resume failed: {err}")))?;
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

fn docker_unavailable() -> ErrorKind {
    ErrorKind::Unavailable("Docker runtime is not available".into())
}

fn docker_lifecycle_unavailable(action: &str, err: impl std::fmt::Display) -> ErrorKind {
    ErrorKind::Unavailable(format!("failed to {action} agent container: {err}"))
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
