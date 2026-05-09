//! Agent command publisher — sends prompt and interrupt commands to sidecars.

use std::sync::Arc;

use agentforge_core::AppResult;
use agentforge_infra::NatsClient;

use crate::services::agent::AgentService;
use futures::future::{BoxFuture, FutureExt};
use serde_json::{Value, json};

/// Low-level command bus abstraction for tests and production NATS publishing.
pub trait AgentCommandBus: Send + Sync {
    fn publish_json<'a>(&'a self, subject: &'a str, payload: Value) -> BoxFuture<'a, AppResult<()>>;
}

/// Publisher for agent sidecar commands.
pub struct AgentCommandService<B = Arc<NatsClient>> {
    nats: B,
}

impl<B> AgentCommandService<B> {
    pub fn new(nats: B) -> Self {
        Self { nats }
    }
}

impl<B: AgentCommandBus> AgentCommandService<B> {
    /// Send a prompt to the agent sidecar command subject.
    pub async fn send_prompt(&self, agent_id: &str, prompt: &str) -> AppResult<()> {
        self.publish(agent_id, json!({ "type": "prompt", "prompt": prompt })).await
    }

    /// Send an interrupt command to the agent sidecar command subject.
    pub async fn interrupt(&self, agent_id: &str) -> AppResult<()> {
        self.publish(agent_id, json!({ "type": "interrupt" })).await
    }

    async fn publish(&self, agent_id: &str, payload: Value) -> AppResult<()> {
        let subject = AgentService::command_subject(agent_id);
        self.nats.publish_json(&subject, payload).await
    }
}

impl AgentCommandBus for Arc<NatsClient> {
    fn publish_json<'a>(&'a self, subject: &'a str, payload: Value) -> BoxFuture<'a, AppResult<()>> {
        async move { NatsClient::publish_json(self.as_ref(), subject, payload).await }.boxed()
    }
}

impl AgentCommandBus for Arc<dyn AgentCommandBus> {
    fn publish_json<'a>(&'a self, subject: &'a str, payload: Value) -> BoxFuture<'a, AppResult<()>> {
        self.as_ref().publish_json(subject, payload)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn subject_format() {
        assert_eq!(format!("sidecar.{}.cmd", "agent-1"), "sidecar.agent-1.cmd");
    }
}
