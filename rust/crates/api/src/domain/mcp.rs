//! Internal MCP bridge protocol and runtime policies.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agentforge_core::{AppError, CliToolKind, ErrorKind};

const WORKSPACE_MOUNT_TARGET: &str = "/workspace";
const DEFAULT_STALE_WORKING_SECS: i64 = 10;

pub(crate) fn auth_error_body(message: &'static str) -> Value {
    json!({ "error": message })
}

pub(crate) fn request_id(request: &Value) -> Value {
    request.get("id").cloned().unwrap_or(Value::Null)
}

pub(crate) fn request_method(request: &Value) -> Option<&str> {
    request.get("method").and_then(Value::as_str)
}

pub(crate) fn initialize_response(id: Value, request: &Value, server_version: &str) -> Value {
    let protocol_version = request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("2024-11-05");
    jsonrpc_result(
        id,
        json!({
            "protocolVersion": protocol_version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentforge-api", "version": server_version }
        }),
    )
}

pub(crate) fn tools_list_response(id: Value) -> Value {
    jsonrpc_result(id, json!({ "tools": tool_list() }))
}

pub(crate) fn initialized_notification_response(id: Value) -> Value {
    jsonrpc_result(id, json!({}))
}

pub(crate) fn jsonrpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub(crate) fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub(crate) fn tool_result(id: Value, is_error: bool, text: String) -> Value {
    jsonrpc_result(
        id,
        json!({
            "content": [{"type": "text", "text": text}],
            "isError": is_error,
        }),
    )
}

pub(crate) fn tool_name(request: &Value) -> Option<&str> {
    request.pointer("/params/name").and_then(Value::as_str)
}

pub(crate) fn tool_arguments(request: &Value) -> Value {
    request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}))
}

pub(crate) fn create_result_text(agent_id: Uuid, status: &str, name: &str) -> Result<String, String> {
    serialize_json(&json!({
        "agentId": agent_id,
        "status": status,
        "name": name,
    }))
}

pub(crate) fn ok_result_text() -> Result<String, String> {
    serialize_json(&json!({ "ok": true }))
}

pub(crate) fn status_result_text(agent_id: Uuid, status: &str) -> Result<String, String> {
    serialize_json(&json!({ "agentId": agent_id, "status": status }))
}

pub(crate) fn parse_required_uuid(arguments: &Value, key: &str) -> Result<Uuid, String> {
    let Some(value) = arguments.get(key).and_then(Value::as_str) else {
        return Err(format!("missing required argument: {key}"));
    };
    Uuid::parse_str(value).map_err(|_| format!("invalid uuid for {key}: {value}"))
}

pub(crate) fn parse_optional_uuid(value: Option<&Value>) -> Result<Option<Uuid>, String> {
    let Some(value) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Ok(None);
    }
    Uuid::parse_str(value).map(Some).map_err(|_| format!("invalid uuid: {value}"))
}

pub(crate) fn app_error_message(err: AppError) -> String {
    match err.kind {
        ErrorKind::Validation(message) => format!("validation error: {message}"),
        ErrorKind::ValidationWithCode { message, .. } => format!("validation error: {message}"),
        ErrorKind::Unprocessable(message) => format!("unprocessable entity: {message}"),
        ErrorKind::NotFound(message) => format!("not found: {message}"),
        ErrorKind::Conflict(message) => format!("conflict: {message}"),
        ErrorKind::Unauthorized => "unauthorized".to_string(),
        ErrorKind::Forbidden(_) => "forbidden".to_string(),
        ErrorKind::ForbiddenWithCode { .. } => "forbidden".to_string(),
        ErrorKind::Unavailable(message) => format!("service unavailable: {message}"),
        ErrorKind::Internal(message) => format!("internal error: {message}"),
    }
}

pub(crate) fn is_not_found_error(err: &AppError) -> bool {
    matches!(err.kind, ErrorKind::NotFound(_))
}

pub(crate) fn missing_container_id_error(agent_id: Uuid) -> AppError {
    ErrorKind::Internal(anyhow!("agent {agent_id} has no container id")).into()
}

pub(crate) fn cli_ready_timeout_error(cli_tool: &str, container_id: &str) -> AppError {
    ErrorKind::Internal(anyhow!("timed out waiting for {cli_tool} prompt in container {container_id}")).into()
}

pub(crate) fn docker_runtime_error(message: String) -> AppError {
    if message.contains("404") || message.contains("No such container") {
        return ErrorKind::NotFound(message).into();
    }
    ErrorKind::Internal(anyhow!(message)).into()
}

pub(crate) fn io_runtime_error(err: std::io::Error) -> AppError {
    ErrorKind::Internal(anyhow!(err)).into()
}

fn serialize_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn tool_list() -> Vec<Value> {
    let cli_tool_enum = CliToolKind::ALL.map(CliToolKind::as_str);
    vec![
        tool(
            "wisdoverse.agent.create",
            "Create a managed workflow agent backed by the Rust API runtime.",
            json!({
                "type": "object",
                "properties": {
                    "projectId": {"type": "string"},
                    "cliTool": {"type": "string", "enum": cli_tool_enum},
                    "name": {"type": "string"},
                    "orgId": {"type": "string"},
                    "userId": {"type": "string"}
                }
            }),
        ),
        tool(
            "wisdoverse.agent.prompt",
            "Send a prompt to an existing managed workflow agent.",
            json!({
                "type": "object",
                "properties": {
                    "agentId": {"type": "string"},
                    "prompt": {"type": "string"}
                },
                "required": ["agentId", "prompt"]
            }),
        ),
        tool(
            "wisdoverse.agent.status",
            "Read the current status of a managed workflow agent.",
            json!({
                "type": "object",
                "properties": {"agentId": {"type": "string"}},
                "required": ["agentId"]
            }),
        ),
        tool(
            "wisdoverse.agent.destroy",
            "Destroy a managed workflow agent.",
            json!({
                "type": "object",
                "properties": {"agentId": {"type": "string"}},
                "required": ["agentId"]
            }),
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerMount {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerCreateRequest {
    pub(crate) image: String,
    pub(crate) name: String,
    pub(crate) working_dir: String,
    pub(crate) env: HashMap<String, String>,
    pub(crate) labels: HashMap<String, String>,
    pub(crate) mounts: Vec<DockerMount>,
    pub(crate) tty: bool,
    pub(crate) open_stdin: bool,
    pub(crate) attach_stdin: bool,
    pub(crate) attach_stdout: bool,
    pub(crate) attach_stderr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerCreatePlan {
    pub(crate) request: DockerCreateRequest,
    pub(crate) cli_tool: String,
}

pub(crate) fn docker_create_plan(
    agent_id: Uuid,
    org_id: Uuid,
    project_id: Option<Uuid>,
    image: String,
    cwd: String,
    mut env: HashMap<String, String>,
) -> DockerCreatePlan {
    let cli_tool = env
        .get("AGENTFORGE_CLI_TOOL")
        .cloned()
        .or_else(|| infer_cli_tool_from_image(&image).map(str::to_string))
        .unwrap_or_else(|| "claude".to_string());

    env.insert("AGENTFORGE_AGENT_ID".to_string(), agent_id.to_string());
    env.insert("AGENTFORGE_ORG_ID".to_string(), org_id.to_string());
    env.insert("AGENTFORGE_WORKSPACE_HOST_PATH".to_string(), cwd.clone());
    if let Some(project_id) = project_id {
        env.insert("AGENTFORGE_PROJECT_ID".to_string(), project_id.to_string());
    }

    DockerCreatePlan {
        request: DockerCreateRequest {
            image,
            name: format!("agentforge-agent-{agent_id}"),
            working_dir: WORKSPACE_MOUNT_TARGET.to_string(),
            env,
            labels: HashMap::from([
                ("agentforge.agent_id".to_string(), agent_id.to_string()),
                ("agentforge.org_id".to_string(), org_id.to_string()),
                ("agentforge.runtime".to_string(), "rust-mcp".to_string()),
            ]),
            mounts: vec![DockerMount { source: cwd, target: WORKSPACE_MOUNT_TARGET.to_string(), read_only: false }],
            tty: true,
            open_stdin: true,
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
        },
        cli_tool,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DockerSessionState {
    Created,
    Running,
    Stopped,
    Dead,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerRuntimeSession {
    pub(crate) container_id: String,
    pub(crate) cli_tool: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletionObservation {
    pub(crate) initial_hash: String,
    pub(crate) last_hash: Option<String>,
    pub(crate) stable_polls: usize,
    pub(crate) saw_working_indicator: bool,
    pub(crate) first_seen_at: Instant,
}

impl CompletionObservation {
    pub(crate) fn new(initial_hash: String, saw_working_indicator: bool) -> Self {
        Self { initial_hash, last_hash: None, stable_polls: 0, saw_working_indicator, first_seen_at: Instant::now() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerMcpRuntimeOptions {
    pub(crate) ready_poll_interval: Duration,
    pub(crate) ready_timeout: Duration,
    pub(crate) prompt_chunk_delay: Duration,
    pub(crate) completion_initial_delay: Duration,
    pub(crate) completion_poll_interval: Duration,
    pub(crate) completion_stable_polls: usize,
}

impl Default for DockerMcpRuntimeOptions {
    fn default() -> Self {
        Self {
            ready_poll_interval: Duration::from_millis(500),
            ready_timeout: Duration::from_secs(90),
            prompt_chunk_delay: Duration::from_millis(150),
            completion_initial_delay: Duration::from_secs(2),
            completion_poll_interval: Duration::from_millis(500),
            completion_stable_polls: 3,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CliRuntimeMarkers {
    pub(crate) ready: &'static [&'static str],
    pub(crate) idle_prompt: &'static [&'static str],
    pub(crate) working_indicator: &'static [&'static str],
}

pub(crate) fn runtime_markers(cli_tool: &str) -> CliRuntimeMarkers {
    match CliToolKind::parse_legacy(cli_tool).unwrap_or(CliToolKind::Claude) {
        CliToolKind::Codex => CliRuntimeMarkers {
            ready: &["for shortcuts", "OpenAI Codex"],
            idle_prompt: &["for shortcuts"],
            working_indicator: &["Working (", "esc to interrupt"],
        },
        CliToolKind::Opencode => CliRuntimeMarkers {
            ready: &["opencode", "Database migration complete"],
            idle_prompt: &["opencode"],
            working_indicator: &[],
        },
        CliToolKind::Gemini => CliRuntimeMarkers { ready: &[">"], idle_prompt: &[">"], working_indicator: &[] },
        CliToolKind::Claude => {
            CliRuntimeMarkers { ready: &["Try \"", "❯"], idle_prompt: &["❯"], working_indicator: &[] }
        }
    }
}

pub(crate) fn has_any_indicator(text: &str, indicators: &[&str]) -> bool {
    indicators.iter().any(|indicator| text.contains(indicator))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(crate) fn infer_cli_tool(record_model: Option<&str>, session_cli_tool: Option<&str>) -> String {
    session_cli_tool
        .map(str::to_string)
        .or_else(|| record_model.and_then(infer_cli_tool_from_image).map(str::to_string))
        .unwrap_or_else(|| "claude".to_string())
}

pub(crate) fn infer_cli_tool_from_image(image: &str) -> Option<&'static str> {
    CliToolKind::ALL.map(CliToolKind::as_str).into_iter().find(|tool| {
        image.contains(&format!(":{tool}")) || image.contains(&format!("-{tool}")) || image.ends_with(tool)
    })
}

pub(crate) fn stale_working_status(updated_at: Option<chrono::DateTime<Utc>>) -> bool {
    updated_at
        .map(|value| Utc::now().signed_duration_since(value) >= ChronoDuration::seconds(DEFAULT_STALE_WORKING_SECS))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_helpers_preserve_tool_result_shape() {
        let result = tool_result(Value::from(7), false, "ok".to_string());
        assert_eq!(result["jsonrpc"], "2.0");
        assert_eq!(result["id"], 7);
        assert_eq!(result["result"]["content"][0]["text"], "ok");
        assert_eq!(result["result"]["isError"], false);
    }

    #[test]
    fn docker_create_plan_owns_workspace_mount_and_labels() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let plan = docker_create_plan(
            agent_id,
            org_id,
            Some(project_id),
            "agentforge-agent-codex:latest".to_string(),
            "/tmp/projects".to_string(),
            HashMap::new(),
        );

        assert_eq!(plan.cli_tool, "codex");
        assert_eq!(plan.request.name, format!("agentforge-agent-{agent_id}"));
        assert_eq!(plan.request.working_dir, "/workspace");
        assert_eq!(plan.request.env["AGENTFORGE_AGENT_ID"], agent_id.to_string());
        assert_eq!(plan.request.env["AGENTFORGE_ORG_ID"], org_id.to_string());
        assert_eq!(plan.request.env["AGENTFORGE_PROJECT_ID"], project_id.to_string());
        assert_eq!(plan.request.labels["agentforge.runtime"], "rust-mcp");
        assert_eq!(
            plan.request.mounts,
            vec![DockerMount {
                source: "/tmp/projects".to_string(),
                target: "/workspace".to_string(),
                read_only: false
            }]
        );
    }
}
