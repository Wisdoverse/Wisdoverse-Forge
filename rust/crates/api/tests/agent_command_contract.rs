use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use agentforge_api::services::agent_commands::{AgentCommandBus, AgentCommandService};
use agentforge_core::AppResult;
use futures::FutureExt;
use serde_json::Value;

#[derive(Clone, Default)]
struct TestCommandBus {
    last: Arc<Mutex<Option<RecordedCommand>>>,
}

#[derive(Clone, Debug)]
struct RecordedCommand {
    subject: String,
    payload: Value,
}

impl TestCommandBus {
    fn last_subject(&self) -> Option<String> {
        self.last.lock().unwrap().as_ref().map(|cmd| cmd.subject.clone())
    }

    fn last_payload(&self) -> Option<Value> {
        self.last.lock().unwrap().as_ref().map(|cmd| cmd.payload.clone())
    }
}

impl AgentCommandBus for TestCommandBus {
    fn publish_json<'a>(
        &'a self,
        subject: &'a str,
        payload: Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>> {
        async move {
            *self.last.lock().unwrap() = Some(RecordedCommand { subject: subject.to_string(), payload });
            Ok(())
        }
        .boxed()
    }
}

#[tokio::test]
async fn send_prompt_publishes_nats_command() {
    let bus = TestCommandBus::default();
    let service = AgentCommandService::new(bus.clone());

    service.send_prompt("agent-1", "hello").await.unwrap();

    assert_eq!(bus.last_subject(), Some("sidecar.agent-1.cmd".into()));
    assert_eq!(
        bus.last_payload(),
        Some(serde_json::json!({
            "type": "prompt",
            "prompt": "hello"
        }))
    );
}

#[tokio::test]
async fn interrupt_publishes_nats_command() {
    let bus = TestCommandBus::default();
    let service = AgentCommandService::new(bus.clone());

    service.interrupt("agent-1").await.unwrap();

    assert_eq!(bus.last_subject(), Some("sidecar.agent-1.cmd".into()));
    assert_eq!(bus.last_payload(), Some(serde_json::json!({ "type": "interrupt" })));
}
