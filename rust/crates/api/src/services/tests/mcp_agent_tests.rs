use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentforge_core::{AgentStatus, AppResult, ErrorKind};
use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::services::mcp_agent::{
    CreateSessionRequest, McpAgentRecord, McpAgentRuntime, McpAgentRuntimeConfig, McpAgentRuntimeCreate,
    McpAgentRuntimeCreateResult, McpAgentService, McpAgentStore, ProjectRuntimeContext, SessionStatus,
};

#[derive(Clone, Default)]
struct TestStore {
    context: Arc<Mutex<Option<ProjectRuntimeContext>>>,
    records: Arc<Mutex<Vec<McpAgentRecord>>>,
    deleted: Arc<Mutex<Vec<(Uuid, Option<String>)>>>,
    leases: Arc<Mutex<HashMap<Uuid, chrono::DateTime<chrono::Utc>>>>,
    get_notify: Option<Arc<Notify>>,
}

impl TestStore {
    fn with_context(context: ProjectRuntimeContext) -> Self {
        Self {
            context: Arc::new(Mutex::new(Some(context))),
            records: Arc::default(),
            deleted: Arc::default(),
            leases: Arc::default(),
            get_notify: None,
        }
    }

    fn records(&self) -> Vec<McpAgentRecord> {
        self.records.lock().expect("records lock").clone()
    }

    fn deleted(&self) -> Vec<(Uuid, Option<String>)> {
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
        let record =
            self.records.lock().expect("records lock").iter().find(|record| record.agent_id == agent_id).cloned();
        if let Some(notify) = &self.get_notify {
            notify.notify_one();
        }
        record.ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")).into())
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

    async fn begin_agent_work(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
    ) -> AppResult<chrono::DateTime<chrono::Utc>> {
        let current = self
            .records
            .lock()
            .expect("records lock")
            .iter()
            .find(|record| record.agent_id == agent_id)
            .and_then(|record| record.container_id.clone());
        if current.as_deref() != Some(expected_container_id) {
            return Err(ErrorKind::Conflict("agent container changed".into()).into());
        }
        let lease = chrono::Utc::now() + chrono::Duration::seconds(60);
        self.leases.lock().expect("leases lock").insert(agent_id, lease);
        Ok(lease)
    }

    async fn renew_agent_work_lease(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<Option<chrono::DateTime<chrono::Utc>>> {
        let current = self
            .records
            .lock()
            .expect("records lock")
            .iter()
            .find(|record| record.agent_id == agent_id)
            .and_then(|record| record.container_id.clone());
        let mut leases = self.leases.lock().expect("leases lock");
        if current.as_deref() != Some(expected_container_id) || leases.get(&agent_id) != Some(&expected_lease) {
            return Ok(None);
        }
        let renewed = chrono::Utc::now() + chrono::Duration::seconds(60);
        leases.insert(agent_id, renewed);
        Ok(Some(renewed))
    }

    async fn finish_agent_work(
        &self,
        agent_id: Uuid,
        expected_container_id: &str,
        expected_lease: chrono::DateTime<chrono::Utc>,
        status: AgentStatus,
    ) -> AppResult<bool> {
        let mut records = self.records.lock().expect("records lock");
        let Some(record) = records.iter_mut().find(|record| record.agent_id == agent_id) else {
            return Ok(false);
        };
        if record.container_id.as_deref() != Some(expected_container_id) {
            return Ok(false);
        }
        let mut leases = self.leases.lock().expect("leases lock");
        if leases.get(&agent_id) != Some(&expected_lease) {
            return Ok(false);
        }
        leases.remove(&agent_id);
        record.status = status;
        Ok(true)
    }

    async fn delete_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()> {
        let mut records = self.records.lock().expect("records lock");
        let Some(index) = records
            .iter()
            .position(|record| record.agent_id == agent_id && record.container_id.as_deref() == expected_container_id)
        else {
            return Err(ErrorKind::Conflict("agent container changed".into()).into());
        };
        records.remove(index);
        self.deleted.lock().expect("deleted lock").push((agent_id, expected_container_id.map(str::to_owned)));
        Ok(())
    }
}

#[derive(Clone, Default)]
struct TestRuntime {
    create_calls: Arc<Mutex<Vec<McpAgentRuntimeCreate>>>,
    prompt_calls: Arc<Mutex<Vec<(Uuid, String)>>>,
    destroy_calls: Arc<Mutex<Vec<(Uuid, Option<String>)>>>,
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
        Ok(McpAgentRuntimeCreateResult {
            container_id: format!("ctr-{}", req.agent_id),
            image_identity: serde_json::json!({
                "source": req.image,
                "imageId": "sha256:test",
                "versionSource": "not-reported",
                "trust": "host-local"
            }),
        })
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        self.prompt_calls.lock().expect("prompt calls").push((agent_id, prompt.to_string()));
        self.statuses
            .lock()
            .expect("statuses")
            .insert(agent_id, SessionStatus { agent_id, status: "working".to_string() });
        Ok(())
    }

    async fn destroy_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()> {
        self.destroy_calls.lock().expect("destroy calls").push((agent_id, expected_container_id.map(str::to_owned)));
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
    let service = McpAgentService::new_for_test(
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
    let service = McpAgentService::new_for_test(
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
    let service = McpAgentService::new_for_test(
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

    let ws = created.workspace_id;
    service.send_prompt(org_id, ws, created.agent_id, "ship it").await.expect("send prompt");
    let status = service.session_status(org_id, ws, created.agent_id).await.expect("status");
    assert_eq!(status.status, "working");

    service.destroy_session(org_id, ws, created.agent_id).await.expect("destroy");

    assert_eq!(
        runtime.prompt_calls.lock().expect("prompt calls").as_slice(),
        &[(created.agent_id, "ship it".to_string())]
    );
    assert_eq!(
        runtime.destroy_calls.lock().expect("destroy calls").as_slice(),
        &[(created.agent_id, Some(format!("ctr-{}", created.agent_id)))]
    );
    assert_eq!(store.deleted(), vec![(created.agent_id, Some(format!("ctr-{}", created.agent_id)))]);
}

#[tokio::test]
async fn prompt_status_destroy_reject_foreign_org_or_workspace() {
    // Tenant-isolation gate (#885): operations scoped to a different org OR workspace
    // than the agent's must be rejected, and must not touch the runtime or store.
    let org_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let store = TestStore::with_context(ProjectRuntimeContext {
        project_id: None,
        org_id,
        user_id,
        workspace_id: Uuid::now_v7(),
    });
    let runtime = TestRuntime::default();
    let service = McpAgentService::new_for_test(
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
            project_id: None,
            cli_tool: "claude".to_string(),
            name: None,
            org_id: Some(org_id),
            user_id: Some(user_id),
        })
        .await
        .expect("create session");

    let ws = created.workspace_id;
    let foreign_org = Uuid::now_v7();
    let foreign_ws = Uuid::now_v7();
    // Different org (even with the correct workspace id) is rejected.
    assert!(service.send_prompt(foreign_org, ws, created.agent_id, "x").await.is_err(), "cross-org prompt must fail");
    assert!(service.session_status(foreign_org, ws, created.agent_id).await.is_err(), "cross-org status must fail");
    assert!(service.destroy_session(foreign_org, ws, created.agent_id).await.is_err(), "cross-org destroy must fail");
    // Different workspace within the same org is also rejected (workspace is the access boundary).
    assert!(service.send_prompt(org_id, foreign_ws, created.agent_id, "x").await.is_err(), "cross-ws prompt must fail");
    assert!(service.session_status(org_id, foreign_ws, created.agent_id).await.is_err(), "cross-ws status must fail");
    assert!(service.destroy_session(org_id, foreign_ws, created.agent_id).await.is_err(), "cross-ws destroy must fail");

    assert!(runtime.prompt_calls.lock().expect("prompt calls").is_empty(), "runtime prompt must not run");
    assert!(runtime.destroy_calls.lock().expect("destroy calls").is_empty(), "runtime destroy must not run");
    assert!(store.deleted().is_empty(), "store delete must not run");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn destroy_rereads_container_after_waiting_for_lifecycle_lock(pool: PgPool) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'MCP race', $2)")
        .bind(org_id)
        .bind(format!("mcp-race-{org_id}"))
        .execute(&pool)
        .await
        .expect("seed organization");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(&pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("mcp-race-{user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("seed user");
    let image_identity = serde_json::json!({
        "source": "agentforge-agent:claude",
        "imageId": format!("sha256:{}", "a".repeat(64)),
        "versionSource": "not-reported",
        "trust": "host-local"
    });
    sqlx::query(
        "INSERT INTO agents
            (id, organization_id, workspace_id, user_id, name, status, cli_tool, runtime_kind,
             container_id, container_image_identity)
         VALUES ($1, $2, $2, $3, 'MCP race', 'idle', 'claude', 'container', 'container-x', $4)",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(&image_identity)
    .execute(&pool)
    .await
    .expect("seed agent");

    let first_read = Arc::new(Notify::new());
    let store = TestStore {
        context: Arc::default(),
        records: Arc::new(Mutex::new(vec![McpAgentRecord {
            agent_id,
            organization_id: org_id,
            workspace_id: org_id,
            user_id,
            project_id: None,
            name: "MCP race".into(),
            status: AgentStatus::Idle,
            container_id: Some("container-x".into()),
            container_image_identity: Some(image_identity.clone()),
            cli_tool: Some("claude".into()),
            model: None,
            provider: None,
            updated_at: None,
        }])),
        deleted: Arc::default(),
        leases: Arc::default(),
        get_notify: Some(first_read.clone()),
    };
    let runtime = TestRuntime::default();
    let service = McpAgentService::new(
        store.clone(),
        runtime.clone(),
        McpAgentRuntimeConfig {
            workspace_root: "/tmp".into(),
            default_image: "agentforge-agent:latest".into(),
            tool_images: HashMap::new(),
            system_api_keys: HashMap::new(),
        },
        pool.clone(),
    );

    let mut replacement = pool.begin().await.expect("begin replacement");
    agentforge_db::lock_agent_lifecycle_in_tx(&mut replacement, agent_id).await.expect("lock replacement");
    let destroy = tokio::spawn(async move { service.destroy_session(org_id, org_id, agent_id).await });
    first_read.notified().await;
    store.records.lock().expect("records lock")[0].container_id = Some("container-y".into());
    sqlx::query("UPDATE agents SET container_id = 'container-y' WHERE id = $1")
        .bind(agent_id)
        .execute(&mut *replacement)
        .await
        .expect("replace container");
    replacement.commit().await.expect("commit replacement");

    destroy.await.expect("join destroy").expect("destroy replacement");
    assert_eq!(
        runtime.destroy_calls.lock().expect("destroy calls").as_slice(),
        &[(agent_id, Some("container-y".into()))]
    );
    assert_eq!(store.deleted(), vec![(agent_id, Some("container-y".into()))]);
}
