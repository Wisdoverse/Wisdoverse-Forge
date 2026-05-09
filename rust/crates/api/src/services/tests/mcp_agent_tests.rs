use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentforge_core::{AgentStatus, AppResult, ErrorKind};
use async_trait::async_trait;
use uuid::Uuid;

use crate::services::mcp_agent::{
    CreateSessionRequest, McpAgentRecord, McpAgentRuntime, McpAgentRuntimeConfig, McpAgentRuntimeCreate,
    McpAgentRuntimeCreateResult, McpAgentService, McpAgentStore, ProjectRuntimeContext, SessionStatus,
};

#[derive(Clone, Default)]
struct TestStore {
    context: Arc<Mutex<Option<ProjectRuntimeContext>>>,
    records: Arc<Mutex<Vec<McpAgentRecord>>>,
    deleted: Arc<Mutex<Vec<Uuid>>>,
}

impl TestStore {
    fn with_context(context: ProjectRuntimeContext) -> Self {
        Self { context: Arc::new(Mutex::new(Some(context))), records: Arc::default(), deleted: Arc::default() }
    }

    fn records(&self) -> Vec<McpAgentRecord> {
        self.records.lock().expect("records lock").clone()
    }

    fn deleted(&self) -> Vec<Uuid> {
        self.deleted.lock().expect("deleted lock").clone()
    }
}

#[async_trait]
impl McpAgentStore for TestStore {
    async fn resolve_project_context(
        &self,
        project_id: Option<Uuid>,
        org_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> AppResult<ProjectRuntimeContext> {
        if let Some(context) = self.context.lock().expect("context lock").clone() {
            return Ok(ProjectRuntimeContext {
                project_id: project_id.or(context.project_id),
                org_id: org_id.unwrap_or(context.org_id),
                user_id: user_id.unwrap_or(context.user_id),
                workspace_id: context.workspace_id,
            });
        }

        Err(ErrorKind::Validation("project or tenant context is required".into()).into())
    }

    async fn insert_agent(&self, record: McpAgentRecord) -> AppResult<()> {
        self.records.lock().expect("records lock").push(record);
        Ok(())
    }

    async fn get_agent(&self, agent_id: Uuid) -> AppResult<McpAgentRecord> {
        self.records
            .lock()
            .expect("records lock")
            .iter()
            .find(|record| record.agent_id == agent_id)
            .cloned()
            .ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")).into())
    }

    async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()> {
        let mut records = self.records.lock().expect("records lock");
        let Some(record) = records.iter_mut().find(|record| record.agent_id == agent_id) else {
            return Err(ErrorKind::NotFound(format!("agent {agent_id}")).into());
        };
        record.status = status;
        record.updated_at = Some(chrono::Utc::now());
        Ok(())
    }

    async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.deleted.lock().expect("deleted lock").push(agent_id);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct TestRuntime {
    create_calls: Arc<Mutex<Vec<McpAgentRuntimeCreate>>>,
    prompt_calls: Arc<Mutex<Vec<(Uuid, String)>>>,
    destroy_calls: Arc<Mutex<Vec<Uuid>>>,
    statuses: Arc<Mutex<HashMap<Uuid, SessionStatus>>>,
}

#[async_trait]
impl McpAgentRuntime for TestRuntime {
    async fn create_agent(&self, req: McpAgentRuntimeCreate) -> AppResult<McpAgentRuntimeCreateResult> {
        self.create_calls.lock().expect("create calls").push(req.clone());
        self.statuses
            .lock()
            .expect("statuses")
            .insert(req.agent_id, SessionStatus { agent_id: req.agent_id, status: "idle".to_string() });
        Ok(McpAgentRuntimeCreateResult { container_id: format!("ctr-{}", req.agent_id) })
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        self.prompt_calls.lock().expect("prompt calls").push((agent_id, prompt.to_string()));
        self.statuses
            .lock()
            .expect("statuses")
            .insert(agent_id, SessionStatus { agent_id, status: "working".to_string() });
        Ok(())
    }

    async fn destroy_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.destroy_calls.lock().expect("destroy calls").push(agent_id);
        self.statuses
            .lock()
            .expect("statuses")
            .insert(agent_id, SessionStatus { agent_id, status: "offline".to_string() });
        Ok(())
    }

    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        self.statuses
            .lock()
            .expect("statuses")
            .get(&agent_id)
            .cloned()
            .ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")).into())
    }
}

#[tokio::test]
async fn create_session_resolves_project_context_and_persists_runtime_record() {
    let project_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let workspace_id = Uuid::now_v7();
    let store =
        TestStore::with_context(ProjectRuntimeContext { project_id: Some(project_id), org_id, user_id, workspace_id });
    let runtime = TestRuntime::default();
    let service = McpAgentService::new(
        store.clone(),
        runtime.clone(),
        McpAgentRuntimeConfig {
            workspace_root: "/data/agentforge/workspaces".to_string(),
            default_image: "agentforge-agent:latest".to_string(),
            tool_images: HashMap::from([("codex".to_string(), "agentforge-agent-codex:latest".to_string())]),
            system_api_keys: HashMap::from([("OPENAI_API_KEY".to_string(), "openai-test-key".to_string())]),
        },
    );

    let created = service
        .create_session(CreateSessionRequest {
            project_id: Some(project_id),
            cli_tool: "codex".to_string(),
            name: Some("Workflow worker".to_string()),
            org_id: None,
            user_id: None,
        })
        .await
        .expect("create session");

    let calls = runtime.create_calls.lock().expect("create calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].org_id, org_id);
    assert_eq!(calls[0].user_id, user_id);
    assert_eq!(calls[0].project_id, Some(project_id));
    assert_eq!(calls[0].name, "Workflow worker");
    assert_eq!(calls[0].image, "agentforge-agent-codex:latest");
    assert_eq!(calls[0].cwd, format!("/data/agentforge/workspaces/orgs/{org_id}/workspaces/{workspace_id}/projects"));
    assert_eq!(calls[0].env.get("OPENAI_API_KEY").map(String::as_str), Some("openai-test-key"));

    let records = store.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].agent_id, created.agent_id);
    assert_eq!(records[0].organization_id, org_id);
    assert_eq!(records[0].user_id, user_id);
    assert_eq!(records[0].project_id, Some(project_id));
    assert_eq!(records[0].status, AgentStatus::Idle);
    assert_eq!(records[0].container_id.as_deref(), Some(format!("ctr-{}", created.agent_id).as_str()));
    assert_eq!(records[0].model.as_deref(), Some("agentforge-agent-codex:latest"));
    assert_eq!(records[0].provider.as_deref(), Some("openai"));
    assert_eq!(records[0].updated_at, None);
    assert_eq!(created.status, "idle");
    assert_eq!(created.name, "Workflow worker");
}

#[tokio::test]
async fn create_session_rejects_missing_project_or_tenant_context() {
    let service = McpAgentService::new(
        TestStore::default(),
        TestRuntime::default(),
        McpAgentRuntimeConfig {
            workspace_root: "/data/agentforge/workspaces".to_string(),
            default_image: "agentforge-agent:latest".to_string(),
            tool_images: HashMap::new(),
            system_api_keys: HashMap::new(),
        },
    );

    let err = service
        .create_session(CreateSessionRequest {
            project_id: None,
            cli_tool: "claude".to_string(),
            name: None,
            org_id: None,
            user_id: None,
        })
        .await
        .expect_err("missing context should fail");

    assert!(
        matches!(err.kind, ErrorKind::Validation(ref message) if message == "project or tenant context is required")
    );
}

#[tokio::test]
async fn prompt_status_and_destroy_delegate_to_runtime_and_store() {
    let project_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let store = TestStore::with_context(ProjectRuntimeContext {
        project_id: Some(project_id),
        org_id,
        user_id,
        workspace_id: Uuid::now_v7(),
    });
    let runtime = TestRuntime::default();
    let service = McpAgentService::new(
        store.clone(),
        runtime.clone(),
        McpAgentRuntimeConfig {
            workspace_root: "/data/agentforge/workspaces".to_string(),
            default_image: "agentforge-agent:latest".to_string(),
            tool_images: HashMap::new(),
            system_api_keys: HashMap::new(),
        },
    );

    let created = service
        .create_session(CreateSessionRequest {
            project_id: Some(project_id),
            cli_tool: "claude".to_string(),
            name: Some("Prompt worker".to_string()),
            org_id: Some(org_id),
            user_id: Some(user_id),
        })
        .await
        .expect("create session");

    service.send_prompt(created.agent_id, "ship it").await.expect("send prompt");
    let status = service.session_status(created.agent_id).await.expect("status");
    assert_eq!(status.status, "working");

    service.destroy_session(created.agent_id).await.expect("destroy");

    assert_eq!(
        runtime.prompt_calls.lock().expect("prompt calls").as_slice(),
        &[(created.agent_id, "ship it".to_string())]
    );
    assert_eq!(runtime.destroy_calls.lock().expect("destroy calls").as_slice(), &[created.agent_id]);
    assert_eq!(store.deleted(), vec![created.agent_id]);
}
