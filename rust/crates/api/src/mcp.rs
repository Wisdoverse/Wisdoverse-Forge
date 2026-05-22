use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use axum::Extension;
use axum::Router;
use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerCreateBody as ContainerConfig, HostConfig};
use bollard::query_parameters::{
    AttachContainerOptions, CreateContainerOptions, InspectContainerOptions, LogsOptions, RemoveContainerOptions,
    StartContainerOptions,
};
use futures::StreamExt;
use serde_json::Value;
use sqlx::PgPool;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use uuid::Uuid;

use agentforge_core::{AgentStatus, AppError, AppResult};
use agentforge_platform::DockerClient;

use crate::domain::mcp::{
    CompletionObservation, DockerCreateRequest, DockerMcpRuntimeOptions, DockerRuntimeSession, DockerSessionState,
    app_error_message, auth_error_body, cli_ready_timeout_error, create_result_text, docker_create_plan,
    docker_runtime_error, has_any_indicator, hash_bytes, infer_cli_tool, initialize_response,
    initialized_notification_response, io_runtime_error, is_not_found_error, jsonrpc_error, missing_container_id_error,
    ok_result_text, parse_optional_uuid, parse_required_uuid, request_id, request_method, runtime_markers,
    stale_working_status, status_result_text, tool_arguments, tool_name, tool_result, tools_list_response,
};
use crate::repositories::agent::{McpAgentInsertRecord, McpAgentRepository};
use crate::services::mcp_agent::{
    CreateSessionRequest, CreateSessionResult, McpAgentRecord, McpAgentRuntime, McpAgentRuntimeConfig,
    McpAgentRuntimeCreate, McpAgentRuntimeCreateResult, McpAgentService, McpAgentStore, ProjectRuntimeContext,
    SessionStatus,
};

const READY_LOG_TAIL: usize = 30;
const COMPLETION_LOG_TAIL: usize = 1000;

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

    let options = DockerMcpRuntimeOptions::default();
    let store = SqlxMcpAgentStore::new(pool);
    let runtime = DockerMcpAgentRuntime::new(
        store.clone(),
        LiveDockerMcpRuntimeBackend::new(docker, options.prompt_chunk_delay),
        options,
    );
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

#[async_trait]
trait DockerMcpRuntimeBackend: Send + Sync {
    async fn create_container(&self, request: DockerCreateRequest) -> AppResult<String>;
    async fn start_container(&self, container_id: &str) -> AppResult<()>;
    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()>;
    async fn inspect_state(&self, container_id: &str) -> AppResult<DockerSessionState>;
    async fn fetch_text_logs(&self, container_id: &str, tail: usize) -> AppResult<String>;
    async fn fetch_raw_logs(&self, container_id: &str, tail: usize) -> AppResult<Vec<u8>>;
    async fn write_stdin_sequence(&self, container_id: &str, chunks: Vec<Vec<u8>>) -> AppResult<()>;
}

#[derive(Clone)]
struct LiveDockerMcpRuntimeBackend {
    docker: Arc<DockerClient>,
    prompt_chunk_delay: Duration,
}

impl LiveDockerMcpRuntimeBackend {
    fn new(docker: Arc<DockerClient>, prompt_chunk_delay: Duration) -> Self {
        Self { docker, prompt_chunk_delay }
    }

    async fn collect_logs(&self, container_id: &str, tail: usize) -> AppResult<Vec<u8>> {
        let mut stream = self.docker.inner().logs(
            container_id,
            Some(LogsOptions {
                follow: false,
                stdout: true,
                stderr: true,
                since: 0,
                until: 0,
                timestamps: false,
                tail: tail.to_string(),
            }),
        );

        let mut buffer = Vec::new();
        while let Some(chunk) = stream.next().await {
            let output = chunk.map_err(docker_into_app_error)?;
            buffer.extend_from_slice(output.as_ref());
        }
        Ok(buffer)
    }
}

#[async_trait]
impl DockerMcpRuntimeBackend for LiveDockerMcpRuntimeBackend {
    async fn create_container(&self, request: DockerCreateRequest) -> AppResult<String> {
        let binds = request
            .mounts
            .iter()
            .map(|mount| {
                if mount.read_only {
                    format!("{}:{}:ro", mount.source, mount.target)
                } else {
                    format!("{}:{}", mount.source, mount.target)
                }
            })
            .collect::<Vec<_>>();
        let env = request.env.into_iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>();
        let config = ContainerConfig {
            image: Some(request.image),
            working_dir: Some(request.working_dir),
            tty: Some(request.tty),
            open_stdin: Some(request.open_stdin),
            attach_stdin: Some(request.attach_stdin),
            attach_stdout: Some(request.attach_stdout),
            attach_stderr: Some(request.attach_stderr),
            env: Some(env),
            labels: Some(request.labels),
            host_config: Some(HostConfig { binds: Some(binds), ..Default::default() }),
            ..Default::default()
        };
        let response = self
            .docker
            .inner()
            // bollard 0.21 makes `platform` a plain `String`. The Docker
            // Engine API treats an empty `?platform=` query parameter as
            // unspecified, matching the pre-bump `platform: None` semantics.
            .create_container(
                Some(CreateContainerOptions { name: Some(request.name), platform: String::new() }),
                config,
            )
            .await
            .map_err(docker_into_app_error)?;
        Ok(response.id)
    }

    async fn start_container(&self, container_id: &str) -> AppResult<()> {
        self.docker
            .inner()
            .start_container(container_id, None::<StartContainerOptions>)
            .await
            .map_err(docker_into_app_error)
    }

    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()> {
        self.docker
            .inner()
            .remove_container(container_id, Some(RemoveContainerOptions { force, ..Default::default() }))
            .await
            .map_err(docker_into_app_error)
    }

    async fn inspect_state(&self, container_id: &str) -> AppResult<DockerSessionState> {
        let info = self
            .docker
            .inner()
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(docker_into_app_error)?;
        let status = info.state.and_then(|state| state.status).map(|value| value.to_string()).unwrap_or_default();
        Ok(match status.as_str() {
            "created" => DockerSessionState::Created,
            "running" => DockerSessionState::Running,
            "exited" | "stopped" => DockerSessionState::Stopped,
            "dead" => DockerSessionState::Dead,
            _ => DockerSessionState::Unknown,
        })
    }

    async fn fetch_text_logs(&self, container_id: &str, tail: usize) -> AppResult<String> {
        let raw = self.collect_logs(container_id, tail).await?;
        Ok(String::from_utf8_lossy(&raw).into_owned())
    }

    async fn fetch_raw_logs(&self, container_id: &str, tail: usize) -> AppResult<Vec<u8>> {
        self.collect_logs(container_id, tail).await
    }

    async fn write_stdin_sequence(&self, container_id: &str, chunks: Vec<Vec<u8>>) -> AppResult<()> {
        let mut attached = self
            .docker
            .inner()
            .attach_container(
                container_id,
                Some(AttachContainerOptions {
                    stdin: true,
                    stdout: false,
                    stderr: false,
                    stream: true,
                    logs: false,
                    detach_keys: None,
                }),
            )
            .await
            .map_err(docker_into_app_error)?;

        for (index, chunk) in chunks.into_iter().enumerate() {
            attached.input.write_all(&chunk).await.map_err(io_into_app_error)?;
            attached.input.flush().await.map_err(io_into_app_error)?;
            if index + 1 < 3 && !self.prompt_chunk_delay.is_zero() {
                sleep(self.prompt_chunk_delay).await;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct DockerMcpAgentRuntime<S, D> {
    store: S,
    docker: D,
    options: DockerMcpRuntimeOptions,
    sessions: Arc<Mutex<HashMap<Uuid, DockerRuntimeSession>>>,
    observations: Arc<Mutex<HashMap<Uuid, CompletionObservation>>>,
}

impl<S, D> DockerMcpAgentRuntime<S, D> {
    fn new(store: S, docker: D, options: DockerMcpRuntimeOptions) -> Self {
        Self {
            store,
            docker,
            options,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            observations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn session_meta(&self, agent_id: Uuid) -> Option<DockerRuntimeSession> {
        self.sessions.lock().expect("session lock").get(&agent_id).cloned()
    }

    fn remember_session(&self, agent_id: Uuid, session: DockerRuntimeSession) {
        self.sessions.lock().expect("session lock").insert(agent_id, session);
    }

    fn clear_tracking(&self, agent_id: Uuid) {
        self.sessions.lock().expect("session lock").remove(&agent_id);
        self.observations.lock().expect("observation lock").remove(&agent_id);
    }
}

impl<S, D> DockerMcpAgentRuntime<S, D>
where
    S: McpAgentStore,
    D: DockerMcpRuntimeBackend,
{
    async fn wait_for_cli_ready(&self, container_id: &str, cli_tool: &str) -> AppResult<()> {
        let markers = runtime_markers(cli_tool);
        let deadline = Instant::now() + self.options.ready_timeout;
        loop {
            let logs = self.docker.fetch_text_logs(container_id, READY_LOG_TAIL).await?;
            if has_any_indicator(&logs, markers.ready) || has_any_indicator(&logs, markers.idle_prompt) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(cli_ready_timeout_error(cli_tool, container_id));
            }
            sleep(self.options.ready_poll_interval).await;
        }
    }

    async fn prompt_completed(
        &self,
        agent_id: Uuid,
        container_id: &str,
        cli_tool: &str,
        updated_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> AppResult<bool> {
        let markers = runtime_markers(cli_tool);
        let raw_logs = self.docker.fetch_raw_logs(container_id, COMPLETION_LOG_TAIL).await?;
        let text_logs = String::from_utf8_lossy(&raw_logs);
        let current_hash = hash_bytes(&raw_logs);
        let fallback_ready = stale_working_status(updated_at);

        let mut observations = self.observations.lock().expect("observation lock");
        let observation =
            observations.entry(agent_id).or_insert_with(|| CompletionObservation::new(current_hash.clone(), false));

        if has_any_indicator(&text_logs, markers.working_indicator) {
            observation.saw_working_indicator = true;
            observation.last_hash = Some(current_hash);
            observation.stable_polls = 0;
            return Ok(false);
        }

        if observation.first_seen_at.elapsed() < self.options.completion_initial_delay {
            observation.last_hash = Some(current_hash);
            return Ok(false);
        }

        observation.stable_polls = match observation.last_hash.as_deref() {
            Some(previous) if previous == current_hash => observation.stable_polls + 1,
            _ => 1,
        };
        observation.last_hash = Some(current_hash.clone());

        if observation.stable_polls < self.options.completion_stable_polls {
            return Ok(false);
        }

        let changed_since_initial = current_hash != observation.initial_hash;
        if markers.working_indicator.is_empty() {
            let idle_detected = has_any_indicator(&text_logs, markers.idle_prompt);
            return Ok(idle_detected && (changed_since_initial || fallback_ready));
        }

        if observation.saw_working_indicator {
            return Ok(changed_since_initial);
        }

        Ok(fallback_ready)
    }
}

#[async_trait]
impl<S, D> McpAgentRuntime for DockerMcpAgentRuntime<S, D>
where
    S: McpAgentStore + Clone + Send + Sync + 'static,
    D: DockerMcpRuntimeBackend + Clone + Send + Sync + 'static,
{
    async fn create_agent(&self, req: McpAgentRuntimeCreate) -> AppResult<McpAgentRuntimeCreateResult> {
        let plan = docker_create_plan(req.agent_id, req.org_id, req.project_id, req.image, req.cwd, req.env);

        let container_id = self.docker.create_container(plan.request).await?;
        if let Err(err) = self.docker.start_container(&container_id).await {
            let _ = self.docker.remove_container(&container_id, true).await;
            return Err(err);
        }

        self.remember_session(
            req.agent_id,
            DockerRuntimeSession { container_id: container_id.clone(), cli_tool: plan.cli_tool },
        );
        self.observations.lock().expect("observation lock").remove(&req.agent_id);

        Ok(McpAgentRuntimeCreateResult { container_id })
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        let record = self.store.get_agent(agent_id).await?;
        let container_id = require_container_id(&record)?;
        let cli_tool = infer_cli_tool(
            record.model.as_deref(),
            self.session_meta(agent_id).as_ref().map(|session| session.cli_tool.as_str()),
        );

        self.wait_for_cli_ready(&container_id, &cli_tool).await?;
        self.docker
            .write_stdin_sequence(&container_id, vec![b"\x15".to_vec(), prompt.as_bytes().to_vec(), b"\r".to_vec()])
            .await?;
        self.store.update_agent_status(agent_id, AgentStatus::Working).await?;

        let initial_hash = match self.docker.fetch_raw_logs(&container_id, COMPLETION_LOG_TAIL).await {
            Ok(raw_logs) => {
                let markers = runtime_markers(&cli_tool);
                let saw_working_indicator =
                    has_any_indicator(&String::from_utf8_lossy(&raw_logs), markers.working_indicator);
                CompletionObservation::new(hash_bytes(&raw_logs), saw_working_indicator)
            }
            Err(_) => CompletionObservation::new(String::new(), false),
        };
        self.observations.lock().expect("observation lock").insert(agent_id, initial_hash);
        self.remember_session(agent_id, DockerRuntimeSession { container_id, cli_tool });
        Ok(())
    }

    async fn destroy_agent(&self, agent_id: Uuid) -> AppResult<()> {
        let record = self.store.get_agent(agent_id).await?;
        let container_id = require_container_id(&record)?;
        self.clear_tracking(agent_id);
        self.docker.remove_container(&container_id, true).await
    }

    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        let record = self.store.get_agent(agent_id).await?;
        let container_id = require_container_id(&record)?;
        let state = match self.docker.inspect_state(&container_id).await {
            Ok(state) => state,
            Err(err) if is_not_found_error(&err) => DockerSessionState::Stopped,
            Err(err) => return Err(err),
        };

        if !matches!(state, DockerSessionState::Running | DockerSessionState::Created) {
            let _ = self.store.update_agent_status(agent_id, AgentStatus::Offline).await;
            self.clear_tracking(agent_id);
            return Ok(SessionStatus { agent_id, status: AgentStatus::Offline.to_string() });
        }

        if record.status == AgentStatus::Offline {
            self.store.update_agent_status(agent_id, AgentStatus::Idle).await?;
            return Ok(SessionStatus { agent_id, status: AgentStatus::Idle.to_string() });
        }

        if record.status == AgentStatus::Working {
            let cli_tool = infer_cli_tool(
                record.model.as_deref(),
                self.session_meta(agent_id).as_ref().map(|session| session.cli_tool.as_str()),
            );
            if self.prompt_completed(agent_id, &container_id, &cli_tool, record.updated_at).await? {
                self.store.update_agent_status(agent_id, AgentStatus::Idle).await?;
                self.observations.lock().expect("observation lock").remove(&agent_id);
                return Ok(SessionStatus { agent_id, status: AgentStatus::Idle.to_string() });
            }
            return Ok(SessionStatus { agent_id, status: AgentStatus::Working.to_string() });
        }

        Ok(SessionStatus { agent_id, status: record.status.to_string() })
    }
}

fn require_container_id(record: &McpAgentRecord) -> AppResult<String> {
    record.container_id.clone().ok_or_else(|| missing_container_id_error(record.agent_id))
}

fn docker_error_message(err: &BollardError) -> String {
    let message = err.to_string();
    if message.contains("404") || message.contains("No such container") {
        return message;
    }
    message
}

fn docker_into_app_error(err: BollardError) -> AppError {
    docker_runtime_error(docker_error_message(&err))
}

fn io_into_app_error(err: std::io::Error) -> AppError {
    io_runtime_error(err)
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

#[cfg(test)]
mod docker_runtime_tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use agentforge_core::ErrorKind;
    use async_trait::async_trait;
    use chrono::{Duration as ChronoDuration, Utc};

    use crate::domain::mcp::DockerMount;

    type StdinWriteRecord = (String, Vec<Vec<u8>>);

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedCreateRequest {
        image: String,
        name: String,
        working_dir: String,
        env: HashMap<String, String>,
        labels: HashMap<String, String>,
        mounts: Vec<DockerMount>,
        tty: bool,
        open_stdin: bool,
        attach_stdin: bool,
    }

    #[derive(Clone, Default)]
    struct TestDockerBackend {
        creates: Arc<Mutex<Vec<RecordedCreateRequest>>>,
        starts: Arc<Mutex<Vec<String>>>,
        removes: Arc<Mutex<Vec<(String, bool)>>>,
        stdin_writes: Arc<Mutex<Vec<StdinWriteRecord>>>,
        text_logs: Arc<Mutex<HashMap<String, VecDeque<String>>>>,
        raw_logs: Arc<Mutex<HashMap<String, VecDeque<Vec<u8>>>>>,
        states: Arc<Mutex<HashMap<String, VecDeque<DockerSessionState>>>>,
    }

    impl TestDockerBackend {
        fn push_text_logs(&self, container_id: &str, logs: impl IntoIterator<Item = &'static str>) {
            self.text_logs
                .lock()
                .expect("text logs lock")
                .insert(container_id.to_string(), logs.into_iter().map(str::to_string).collect());
        }

        fn push_raw_logs(&self, container_id: &str, logs: impl IntoIterator<Item = &'static [u8]>) {
            self.raw_logs
                .lock()
                .expect("raw logs lock")
                .insert(container_id.to_string(), logs.into_iter().map(|entry| entry.to_vec()).collect());
        }

        fn push_states(&self, container_id: &str, states: impl IntoIterator<Item = DockerSessionState>) {
            self.states.lock().expect("states lock").insert(container_id.to_string(), states.into_iter().collect());
        }

        fn take_creates(&self) -> Vec<RecordedCreateRequest> {
            self.creates.lock().expect("creates lock").clone()
        }

        fn take_starts(&self) -> Vec<String> {
            self.starts.lock().expect("starts lock").clone()
        }

        fn take_removes(&self) -> Vec<(String, bool)> {
            self.removes.lock().expect("removes lock").clone()
        }

        fn take_stdin_writes(&self) -> Vec<StdinWriteRecord> {
            self.stdin_writes.lock().expect("stdin writes lock").clone()
        }
    }

    #[async_trait]
    impl DockerMcpRuntimeBackend for TestDockerBackend {
        async fn create_container(&self, request: DockerCreateRequest) -> AppResult<String> {
            self.creates.lock().expect("creates lock").push(RecordedCreateRequest {
                image: request.image,
                name: request.name,
                working_dir: request.working_dir,
                env: request.env,
                labels: request.labels,
                mounts: request.mounts,
                tty: request.tty,
                open_stdin: request.open_stdin,
                attach_stdin: request.attach_stdin,
            });
            Ok("ctr-test".to_string())
        }

        async fn start_container(&self, container_id: &str) -> AppResult<()> {
            self.starts.lock().expect("starts lock").push(container_id.to_string());
            Ok(())
        }

        async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()> {
            self.removes.lock().expect("removes lock").push((container_id.to_string(), force));
            Ok(())
        }

        async fn inspect_state(&self, container_id: &str) -> AppResult<DockerSessionState> {
            let mut states = self.states.lock().expect("states lock");
            let queue = states.get_mut(container_id).expect("state queue");
            let value = if queue.len() > 1 {
                queue.pop_front().expect("state entry")
            } else {
                queue.front().expect("state entry").clone()
            };
            Ok(value)
        }

        async fn fetch_text_logs(&self, container_id: &str, _tail: usize) -> AppResult<String> {
            let mut logs = self.text_logs.lock().expect("text logs lock");
            let queue = logs.get_mut(container_id).expect("text queue");
            let value = if queue.len() > 1 {
                queue.pop_front().expect("text log entry")
            } else {
                queue.front().expect("text log entry").clone()
            };
            Ok(value)
        }

        async fn fetch_raw_logs(&self, container_id: &str, _tail: usize) -> AppResult<Vec<u8>> {
            let mut logs = self.raw_logs.lock().expect("raw logs lock");
            let queue = logs.get_mut(container_id).expect("raw queue");
            let value = if queue.len() > 1 {
                queue.pop_front().expect("raw log entry")
            } else {
                queue.front().expect("raw log entry").clone()
            };
            Ok(value)
        }

        async fn write_stdin_sequence(&self, container_id: &str, chunks: Vec<Vec<u8>>) -> AppResult<()> {
            self.stdin_writes.lock().expect("stdin writes lock").push((container_id.to_string(), chunks));
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct TestStore {
        agents: Arc<Mutex<HashMap<Uuid, McpAgentRecord>>>,
    }

    impl TestStore {
        fn insert(&self, record: McpAgentRecord) {
            self.agents.lock().expect("agents lock").insert(record.agent_id, record);
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
            let org_id = org_id.expect("org_id");
            Ok(ProjectRuntimeContext { project_id, org_id, user_id: user_id.expect("user_id"), workspace_id: org_id })
        }

        async fn insert_agent(&self, record: McpAgentRecord) -> AppResult<()> {
            self.insert(record);
            Ok(())
        }

        async fn get_agent(&self, agent_id: Uuid) -> AppResult<McpAgentRecord> {
            self.agents
                .lock()
                .expect("agents lock")
                .get(&agent_id)
                .cloned()
                .ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")).into())
        }

        async fn update_agent_status(&self, agent_id: Uuid, status: AgentStatus) -> AppResult<()> {
            let mut agents = self.agents.lock().expect("agents lock");
            let agent = agents.get_mut(&agent_id).ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")))?;
            agent.status = status;
            agent.updated_at = Some(Utc::now());
            Ok(())
        }

        async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
            self.agents.lock().expect("agents lock").remove(&agent_id);
            Ok(())
        }
    }

    fn runtime(store: TestStore, docker: TestDockerBackend) -> DockerMcpAgentRuntime<TestStore, TestDockerBackend> {
        DockerMcpAgentRuntime::new(
            store,
            docker,
            DockerMcpRuntimeOptions {
                ready_poll_interval: Duration::from_millis(5),
                ready_timeout: Duration::from_millis(40),
                prompt_chunk_delay: Duration::from_millis(0),
                completion_initial_delay: Duration::from_millis(0),
                completion_poll_interval: Duration::from_millis(5),
                completion_stable_polls: 2,
            },
        )
    }

    #[tokio::test]
    async fn docker_runtime_creates_container_with_workspace_mount_and_runtime_env() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let store = TestStore::default();
        let docker = TestDockerBackend::default();
        let runtime = runtime(store, docker.clone());

        let result = runtime
            .create_agent(McpAgentRuntimeCreate {
                agent_id,
                org_id,
                user_id,
                project_id: Some(project_id),
                name: "Workflow worker".to_string(),
                image: "agentforge-agent-codex:latest".to_string(),
                cwd: format!("/data/agentforge/workspaces/orgs/{org_id}/workspaces/{workspace_id}/projects"),
                env: HashMap::from([
                    ("AGENTFORGE_CLI_TOOL".to_string(), "codex".to_string()),
                    ("OPENAI_API_KEY".to_string(), "test-key".to_string()),
                ]),
            })
            .await
            .expect("create agent");

        assert_eq!(result.container_id, "ctr-test");
        assert_eq!(docker.take_starts(), vec!["ctr-test".to_string()]);

        let creates = docker.take_creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].image, "agentforge-agent-codex:latest");
        assert_eq!(creates[0].working_dir, "/workspace");
        assert_eq!(creates[0].name, format!("agentforge-agent-{agent_id}"));
        assert_eq!(creates[0].env.get("AGENTFORGE_AGENT_ID").map(String::as_str), Some(agent_id.to_string().as_str()));
        assert_eq!(creates[0].env.get("AGENTFORGE_ORG_ID").map(String::as_str), Some(org_id.to_string().as_str()));
        assert_eq!(
            creates[0].env.get("AGENTFORGE_PROJECT_ID").map(String::as_str),
            Some(project_id.to_string().as_str())
        );
        assert_eq!(creates[0].env.get("AGENTFORGE_CLI_TOOL").map(String::as_str), Some("codex"));
        assert_eq!(
            creates[0].mounts,
            vec![DockerMount {
                source: format!("/data/agentforge/workspaces/orgs/{org_id}/workspaces/{workspace_id}/projects"),
                target: "/workspace".to_string(),
                read_only: false,
            }]
        );
        assert!(creates[0].tty);
        assert!(creates[0].open_stdin);
        assert!(creates[0].attach_stdin);
    }

    #[tokio::test]
    async fn docker_runtime_prompt_status_and_destroy_use_local_store_and_docker_backend() {
        let agent_id = Uuid::now_v7();
        let now = Utc::now() - ChronoDuration::seconds(30);
        let store = TestStore::default();
        store.insert(McpAgentRecord {
            agent_id,
            organization_id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            project_id: Some(Uuid::now_v7()),
            name: "Prompt worker".to_string(),
            status: AgentStatus::Idle,
            container_id: Some("ctr-test".to_string()),
            model: Some("agentforge-agent-codex:latest".to_string()),
            provider: Some("openai".to_string()),
            updated_at: Some(now),
        });

        let docker = TestDockerBackend::default();
        docker.push_text_logs("ctr-test", ["OpenAI Codex\nfor shortcuts", "OpenAI Codex\nfor shortcuts"]);
        docker.push_raw_logs(
            "ctr-test",
            [
                b"OpenAI Codex\nWorking (3s)".as_slice(),
                b"OpenAI Codex\nAnswer ready\nfor shortcuts".as_slice(),
                b"OpenAI Codex\nAnswer ready\nfor shortcuts".as_slice(),
            ],
        );
        docker.push_states(
            "ctr-test",
            [DockerSessionState::Running, DockerSessionState::Running, DockerSessionState::Running],
        );
        let runtime = runtime(store.clone(), docker.clone());

        runtime.send_prompt(agent_id, "ship it").await.expect("send prompt");
        let working = runtime.session_status(agent_id).await.expect("working status");
        let idle = runtime.session_status(agent_id).await.expect("idle status");
        runtime.destroy_agent(agent_id).await.expect("destroy");

        assert_eq!(working.status, "working");
        assert_eq!(idle.status, "idle");
        assert_eq!(
            docker.take_stdin_writes(),
            vec![("ctr-test".to_string(), vec![b"\x15".to_vec(), b"ship it".to_vec(), b"\r".to_vec()],)]
        );
        assert_eq!(docker.take_removes(), vec![("ctr-test".to_string(), true)]);
    }
}
