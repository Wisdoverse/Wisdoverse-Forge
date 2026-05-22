use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use axum::Extension;
use axum::Router;
use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use agentforge_core::{AgentStatus, AppResult};
use agentforge_platform::DockerClient;

use crate::domain::mcp::{
    app_error_message, auth_error_body, create_result_text, initialize_response, initialized_notification_response,
    jsonrpc_error, ok_result_text, parse_optional_uuid, parse_required_uuid, request_id, request_method,
    status_result_text, tool_arguments, tool_name, tool_result, tools_list_response,
};
use crate::repositories::agent::{McpAgentInsertRecord, McpAgentRepository};
use crate::services::mcp_agent::{
    CreateSessionRequest, CreateSessionResult, McpAgentRecord, McpAgentRuntime, McpAgentRuntimeConfig, McpAgentService,
    McpAgentStore, ProjectRuntimeContext, SessionStatus,
};
use crate::services::mcp_docker_runtime::docker_mcp_agent_runtime;

#[async_trait]
pub trait McpAgentTools: Send + Sync {
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult>;
    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()>;
    async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()>;
    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus>;
}

#[derive(Clone)]
struct McpState {
    internal_token: Arc<String>,
    tools: Arc<dyn McpAgentTools>,
}

pub fn mcp_router<S>(internal_token: impl Into<String>, tools: Arc<dyn McpAgentTools>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = McpState { internal_token: Arc::new(internal_token.into()), tools };
    Router::new().route("/mcp", post(handle_request)).layer(Extension(state))
}

pub async fn build_live_mcp_components(
    pool: PgPool,
    docker: Option<Arc<DockerClient>>,
) -> anyhow::Result<Option<(String, Arc<dyn McpAgentTools>)>> {
    let enabled = read_bool("MCP_ENABLED");
    if !enabled {
        return Ok(None);
    }

    let token = read_required("MCP_TOKEN").context("MCP_ENABLED=true requires MCP_TOKEN")?;
    let docker = docker.ok_or_else(|| anyhow!("MCP_ENABLED=true requires Docker to be available"))?;
    let workspace_root =
        env::var("AGENTFORGE_WORKSPACE_ROOT").unwrap_or_else(|_| "/data/agentforge/workspaces".to_string());
    let default_image = env::var("CONTAINER_AGENT_IMAGE").unwrap_or_else(|_| "agentforge-agent:latest".to_string());

    let tool_images = HashMap::from_iter(
        [
            ("claude", env::var("CONTAINER_IMAGE_CLAUDE").ok()),
            ("opencode", env::var("CONTAINER_IMAGE_OPENCODE").ok()),
            ("codex", env::var("CONTAINER_IMAGE_CODEX").ok()),
            ("gemini", env::var("CONTAINER_IMAGE_GEMINI").ok()),
        ]
        .into_iter()
        .filter_map(|(tool, image)| {
            image.filter(|value| !value.trim().is_empty()).map(|value| (tool.to_string(), value))
        }),
    );

    let system_api_keys = HashMap::from_iter(
        [
            ("ANTHROPIC_API_KEY", env::var("CONTAINER_ANTHROPIC_API_KEY").ok()),
            ("OPENAI_API_KEY", env::var("CONTAINER_OPENAI_API_KEY").ok()),
            ("GEMINI_API_KEY", env::var("CONTAINER_GOOGLE_API_KEY").ok()),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            value.filter(|entry| !entry.trim().is_empty()).map(|entry| (name.to_string(), entry))
        }),
    );

    let store = SqlxMcpAgentStore::new(pool);
    let runtime = docker_mcp_agent_runtime(store.clone(), docker);
    let service = McpAgentService::new(
        store,
        runtime,
        McpAgentRuntimeConfig { workspace_root, default_image, tool_images, system_api_keys },
    );

    Ok(Some((token, Arc::new(service))))
}

async fn handle_request(
    Extension(state): Extension<McpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = authorize(&headers, state.internal_token.as_str()) {
        return *response;
    }

    Json(handle_jsonrpc(state.tools.as_ref(), body).await).into_response()
}

fn authorize(headers: &HeaderMap, internal_token: &str) -> Result<(), Box<Response>> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("missing authorization header"))).into_response(),
        ));
    };
    let Ok(value) = value.to_str() else {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("invalid authorization token"))).into_response(),
        ));
    };
    if value != format!("Bearer {internal_token}") {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("invalid authorization token"))).into_response(),
        ));
    }
    Ok(())
}

async fn handle_jsonrpc(tools: &dyn McpAgentTools, request: Value) -> Value {
    let id = request_id(&request);
    let Some(method) = request_method(&request) else {
        return jsonrpc_error(id, -32600, "invalid request");
    };

    match method {
        "initialize" => initialize_response(id, &request, crate::VERSION),
        "tools/list" => tools_list_response(id),
        "tools/call" => call_tool(tools, id, &request).await,
        "notifications/initialized" => initialized_notification_response(id),
        _ => jsonrpc_error(id, -32601, "method not found"),
    }
}

async fn call_tool(tools: &dyn McpAgentTools, id: Value, request: &Value) -> Value {
    let Some(name) = tool_name(request) else {
        return tool_result(id, true, "missing required argument: name".to_string());
    };
    let arguments = tool_arguments(request);

    let result = match name {
        "wisdoverse.agent.create" | "agentforge.agent.create" => handle_create(tools, &arguments).await,
        "wisdoverse.agent.prompt" | "agentforge.agent.prompt" => handle_prompt(tools, &arguments).await,
        "wisdoverse.agent.status" | "agentforge.agent.status" => handle_status(tools, &arguments).await,
        "wisdoverse.agent.destroy" | "agentforge.agent.destroy" => handle_destroy(tools, &arguments).await,
        _ => Err(format!("unknown tool: {name}")),
    };

    match result {
        Ok(text) => tool_result(id, false, text),
        Err(message) => tool_result(id, true, message),
    }
}

async fn handle_create(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let cli_tool = arguments.get("cliTool").and_then(Value::as_str).unwrap_or("claude").to_string();
    let request = CreateSessionRequest {
        project_id: parse_optional_uuid(arguments.get("projectId"))?,
        cli_tool,
        name: arguments.get("name").and_then(Value::as_str).map(str::to_owned),
        org_id: parse_optional_uuid(arguments.get("orgId"))?,
        user_id: parse_optional_uuid(arguments.get("userId"))?,
    };

    let result = tools.create_session(request).await.map_err(app_error_message)?;
    create_result_text(result.agent_id, &result.status, &result.name)
}

async fn handle_prompt(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    let Some(prompt) = arguments.get("prompt").and_then(Value::as_str) else {
        return Err("missing required argument: prompt".to_string());
    };
    tools.send_prompt(agent_id, prompt).await.map_err(app_error_message)?;
    ok_result_text()
}

async fn handle_status(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    let status = tools.session_status(agent_id).await.map_err(app_error_message)?;
    status_result_text(status.agent_id, &status.status)
}

async fn handle_destroy(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    tools.destroy_session(agent_id).await.map_err(app_error_message)?;
    ok_result_text()
}

#[derive(Clone)]
pub struct SqlxMcpAgentStore {
    repo: McpAgentRepository,
}

impl SqlxMcpAgentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { repo: McpAgentRepository::new(pool) }
    }
}

#[async_trait]
impl McpAgentStore for SqlxMcpAgentStore {
    async fn resolve_project_context(
        &self,
        project_id: Option<Uuid>,
        org_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> AppResult<ProjectRuntimeContext> {
        let row = self.repo.resolve_project_context(project_id, org_id, user_id).await?;
        Ok(ProjectRuntimeContext {
            project_id: row.project_id,
            org_id: row.organization_id,
            user_id: row.user_id,
            workspace_id: row.workspace_id,
        })
    }

    async fn insert_agent(&self, record: McpAgentRecord) -> AppResult<()> {
        self.repo
            .insert_agent(McpAgentInsertRecord {
                agent_id: record.agent_id,
                organization_id: record.organization_id,
                workspace_id: record.workspace_id,
                project_id: record.project_id,
                user_id: record.user_id,
                name: record.name,
                status: record.status,
                container_id: record.container_id,
                model: record.model,
                provider: record.provider,
            })
            .await
    }

    async fn get_agent(&self, agent_id: Uuid) -> AppResult<McpAgentRecord> {
        let agent = self.repo.get_agent(agent_id).await?;

        Ok(McpAgentRecord {
            agent_id: agent.id.as_uuid(),
            organization_id: agent.organization_id.as_uuid(),
            workspace_id: agent.workspace_id.as_uuid(),
            user_id: agent.user_id.as_uuid(),
            project_id: agent.project_id.map(|id| id.as_uuid()),
            name: agent.name.unwrap_or_else(|| format!("Agent {}", &agent.id.to_string()[..8])),
            status: agent.status,
            container_id: agent.container_id,
            model: agent.model,
            provider: agent.provider,
            updated_at: Some(agent.updated_at),
        })
    }

    async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()> {
        self.repo.update_agent_status(agent_id, status).await
    }

    async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.repo.delete_agent(agent_id).await
    }
}

fn read_bool(name: &str) -> bool {
    matches!(env::var(name).ok().as_deref(), Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on"))
}

fn read_required(name: &str) -> anyhow::Result<String> {
    let value = env::var(name).with_context(|| format!("missing environment variable {name}"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("environment variable {name} is empty"));
    }
    Ok(value)
}

#[async_trait]
impl<S, R> McpAgentTools for McpAgentService<S, R>
where
    S: McpAgentStore,
    R: McpAgentRuntime,
{
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult> {
        McpAgentService::create_session(self, request).await
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        McpAgentService::send_prompt(self, agent_id, prompt).await
    }

    async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()> {
        McpAgentService::destroy_session(self, agent_id).await
    }

    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        McpAgentService::session_status(self, agent_id).await
    }
}
