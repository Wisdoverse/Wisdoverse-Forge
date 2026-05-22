//! Agent domain rules.
//!
//! This module owns the Agent bounded-context policies that are independent of
//! HTTP handlers, SQL repositories, Docker clients, and message buses.

use std::collections::HashMap;

use agentforge_core::{AgentStatus, AppResult, CliToolKind, ErrorKind};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::credential::ContainerCliCredentialPolicy;

pub(crate) fn agent_list_response<T: Serialize>(agents: T) -> Value {
    json!({ "ok": true, "agents": agents })
}

pub(crate) fn agent_response<T: Serialize>(agent: T) -> Value {
    json!({ "ok": true, "agent": agent })
}

pub(crate) fn agent_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn agent_delete_response() -> Value {
    json!({ "ok": true })
}

pub(crate) fn agent_status_response(status: &str) -> Value {
    json!({ "ok": true, "status": status })
}

pub(crate) fn agent_prompt_sent_response(agent_id: Uuid) -> Value {
    json!({ "ok": true, "status": "sent", "agent_id": agent_id })
}

pub(crate) fn agent_messages_response<T: Serialize>(messages: T, has_more: bool) -> Value {
    json!({ "ok": true, "messages": messages, "hasMore": has_more })
}

pub(crate) fn agent_messages_deleted_response(deleted: u64) -> Value {
    json!({ "ok": true, "deleted": deleted })
}

pub(crate) fn agent_container_status_response(container_id: &str, status: &str) -> Value {
    json!({ "ok": true, "container_id": container_id, "status": status })
}

pub(crate) fn agent_prompt_command_payload(prompt: &str) -> Value {
    json!({ "type": "prompt", "prompt": prompt })
}

pub(crate) fn agent_interrupt_command_payload() -> Value {
    json!({ "type": "interrupt" })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentContainerStartOutcome {
    container_id: String,
    status: AgentContainerStartStatus,
}

impl AgentContainerStartOutcome {
    pub(crate) fn started(container_id: impl Into<String>) -> Self {
        Self { container_id: container_id.into(), status: AgentContainerStartStatus::Started }
    }

    pub(crate) fn already_running(container_id: impl Into<String>) -> Self {
        Self { container_id: container_id.into(), status: AgentContainerStartStatus::AlreadyRunning }
    }

    pub(crate) fn container_id(&self) -> &str {
        &self.container_id
    }

    pub(crate) fn status(&self) -> &'static str {
        self.status.as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentContainerStartStatus {
    Started,
    AlreadyRunning,
}

impl AgentContainerStartStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::AlreadyRunning => "already_running",
        }
    }
}

pub(crate) fn agent_git_status_response() -> Value {
    json!({
        "ok": true,
        "data": {
            "branch": null,
            "ahead": 0,
            "behind": 0,
            "modified": [],
            "untracked": []
        }
    })
}

pub(crate) fn agent_permission_response(projection: AgentPermissionProjection) -> Value {
    json!({ "ok": true, "data": projection })
}

pub(crate) fn pool_status_response(docker_available: bool) -> Value {
    json!({
        "ok": true,
        "data": {
            "docker_available": docker_available,
            "message": "pool status — warm pool integration pending"
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AgentPermissionProjection {
    pub(crate) has_permission: bool,
    pub(crate) is_owner: bool,
    pub(crate) permission: Option<String>,
}

/// Validated pagination request for agent lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentListPage {
    limit: i64,
    offset: i64,
}

impl AgentListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Agent chat history pagination.
///
/// MessageRepository returns chronological rows after fetching newest-first.
/// When fetching one extra row, that extra row sits at the front and must be
/// dropped before returning the page to clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentMessagePage {
    limit: i64,
}

impl AgentMessagePage {
    pub(crate) fn new(limit: i64) -> Self {
        Self { limit: limit.clamp(1, 200) }
    }

    pub(crate) fn fetch_limit(self) -> i64 {
        self.limit + 1
    }

    pub(crate) fn split_has_more<T>(self, mut rows: Vec<T>) -> (Vec<T>, bool) {
        let has_more = rows.len() as i64 > self.limit;
        if has_more {
            rows.remove(0);
        }
        (rows, has_more)
    }
}

/// Agent display name value object.
pub(crate) struct AgentName;

impl AgentName {
    pub(crate) fn validate(name: Option<&str>) -> AppResult<()> {
        if let Some(name) = name
            && name.len() > 255
        {
            return Err(ErrorKind::Validation("name must be 255 characters or less".into()).into());
        }
        Ok(())
    }
}

/// Canonical Container CLI selection.
pub(crate) struct AgentCliToolSelection;

impl AgentCliToolSelection {
    pub(crate) fn normalize(raw: Option<&str>) -> AppResult<Option<&'static str>> {
        raw.map(|tool| {
            CliToolKind::parse_legacy(tool)
                .map(CliToolKind::as_str)
                .map_err(|err| ErrorKind::Validation(err.to_string()).into())
        })
        .transpose()
    }
}

/// Canonical docker image selection for container-backed agents.
pub(crate) struct AgentContainerImagePolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentContainerImageRejection {
    UnsupportedCliTool(String),
    MissingContainerShell,
}

impl AgentContainerImageRejection {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::UnsupportedCliTool(message) => message.clone(),
            Self::MissingContainerShell => {
                "agent has no cli_tool — provider+prompt agents have no container shell".to_string()
            }
        }
    }
}

impl AgentContainerImagePolicy {
    pub(crate) fn resolve(cli_tool: Option<&str>, model: Option<&str>) -> Result<String, AgentContainerImageRejection> {
        if let Some(tool) = cli_tool {
            let tool = CliToolKind::parse_legacy(tool)
                .map_err(|err| AgentContainerImageRejection::UnsupportedCliTool(err.to_string()))?;
            return Ok(format!("agentforge-agent:{}", tool.as_str()));
        }

        if let Some(model) = model {
            let trimmed = model.trim();
            if let Some(suffix) = trimmed.strip_prefix("agentforge-agent:")
                && CliToolKind::parse_legacy(suffix).is_ok()
            {
                return Ok(trimmed.to_string());
            }
        }

        Err(AgentContainerImageRejection::MissingContainerShell)
    }

    pub(crate) fn resolve_for_start(cli_tool: Option<&str>, model: Option<&str>) -> AppResult<String> {
        Self::resolve(cli_tool, model).map_err(|err| {
            ErrorKind::Validation(format!(
                "{} — set cli_tool to one of: claude, codex, gemini, opencode (this agent has cli_tool={cli_tool:?}, model={model:?})",
                err.message(),
            ))
            .into()
        })
    }
}

/// Environment input for a spawned agent container.
pub(crate) struct AgentContainerEnvInput<'a> {
    pub(crate) agent_id: Uuid,
    pub(crate) org_id: Uuid,
    pub(crate) cli_tool: Option<&'a str>,
    pub(crate) cli_model: Option<&'a str>,
    pub(crate) codex_default_model: Option<&'a str>,
    pub(crate) nats_base_url: Option<&'a str>,
    pub(crate) nats_connect_password: &'a str,
    pub(crate) container_server_url: Option<&'a str>,
    pub(crate) workspace_host_path: Option<&'a str>,
    pub(crate) hmac_secret: &'a str,
    pub(crate) context_injection_enabled: bool,
}

/// Agent container environment policy.
///
/// The sidecar worker bridge only starts when the spawned container receives
/// the expected identity, HMAC, and NATS env vars. Shared NATS credentials from
/// deployment config are stripped before interpolation so the container only
/// receives its own per-agent connect identity.
pub(crate) struct AgentContainerEnvPolicy;

impl AgentContainerEnvPolicy {
    /// Pick the NATS base URL (scheme + host + port, without user-info) for
    /// per-agent credential interpolation.
    pub(crate) fn pick_nats_base_url(agent_url: Option<&str>, backend_url: Option<&str>) -> Option<String> {
        let source = agent_url.or(backend_url)?;
        Self::strip_nats_url_user_info(source)
    }

    /// Strip any `user:password@` user-info segment from a `nats://...` URL.
    pub(crate) fn strip_nats_url_user_info(url: &str) -> Option<String> {
        let (scheme, rest) = url.split_once("://")?;
        let host_part = match rest.rsplit_once('@') {
            Some((_user_info, host)) => host,
            None => rest,
        };
        Some(format!("{scheme}://{host_part}"))
    }

    pub(crate) fn build(input: AgentContainerEnvInput<'_>) -> Vec<String> {
        let mut env = vec![
            format!("AGENT_ID={}", input.agent_id),
            format!("ORG_ID={}", input.org_id),
            format!("HMAC_SECRET={}", input.hmac_secret),
            format!("AGENTFORGE_CONTEXT_INJECTION_ENABLED={}", input.context_injection_enabled),
        ];
        if let Some(base) = input.nats_base_url
            && let Some((scheme, host)) = base.split_once("://")
        {
            let url = format!("{scheme}://{}:{}@{host}", input.agent_id, input.nats_connect_password);
            env.push(format!("AGENTFORGE_NATS_URL={url}"));
            env.push(format!("NATS_URL={url}"));
        }
        if let Some(url) = input.container_server_url {
            env.push(format!("AGENTFORGE_SERVER_URL={url}"));
        }
        if let Some(path) = input.workspace_host_path {
            env.push(format!("AGENTFORGE_WORKSPACE_HOST_PATH={path}"));
        }
        if let Some(tool) = input.cli_tool.and_then(|tool| CliToolKind::parse_legacy(tool).ok()) {
            env.push(format!("CLI_TOOL={}", tool.as_str()));
        }
        if let Some(model) = Self::cli_model_env_value(input.cli_tool, input.cli_model, input.codex_default_model) {
            env.push(format!("AGENTFORGE_CLI_MODEL={model}"));
        }
        env
    }

    /// CREDS_DIR per Container CLI matches the canonical paths in
    /// `docker/scripts/agent-entrypoint.sh`.
    pub(crate) fn creds_dir_for_cli_tool(cli_tool: &str) -> Option<&'static str> {
        match CliToolKind::parse_legacy(cli_tool).ok()? {
            CliToolKind::Claude => Some("/home/agent/.claude"),
            CliToolKind::Gemini => Some("/home/agent/.gemini"),
            CliToolKind::Opencode => Some("/home/agent/.local/share/opencode"),
            CliToolKind::Codex => Some("/home/agent/.codex"),
        }
    }

    fn cli_model_env_value(
        cli_tool: Option<&str>,
        model: Option<&str>,
        codex_default_model: Option<&str>,
    ) -> Option<String> {
        let tool = cli_tool.and_then(|tool| CliToolKind::parse_legacy(tool).ok())?;
        let explicit = model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .filter(|model| !model.starts_with("agentforge-agent:"))
            .filter(|model| CliToolKind::parse_legacy(model).is_err());
        if let Some(model) = explicit {
            return Some(model.to_string());
        }
        if tool == CliToolKind::Codex {
            return codex_default_model.map(str::trim).filter(|model| !model.is_empty()).map(str::to_string);
        }
        None
    }
}

/// Agent lifecycle policy.
pub(crate) struct AgentLifecycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentStatusTransition {
    Noop,
    Change(AgentStatus),
}

/// Runtime state reduced to the lifecycle distinction needed for restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentContainerRuntimeState {
    Running,
    NotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRestartPlan {
    StopThenStart,
    StartOnly,
}

pub(crate) struct AgentContainerLifecyclePolicy;

impl AgentContainerLifecyclePolicy {
    pub(crate) fn ensure_container_backed(cli_tool: Option<&str>) -> AppResult<()> {
        if cli_tool.is_none() {
            return Err(ErrorKind::Validation("agent is not container-backed".into()).into());
        }
        Ok(())
    }

    pub(crate) fn restart_container_id(container_id: Option<&str>) -> AppResult<&str> {
        container_id.ok_or_else(|| ErrorKind::Validation("agent has no container".into()).into())
    }

    pub(crate) fn resume_container_id(container_id: Option<&str>) -> AppResult<&str> {
        container_id.ok_or_else(|| ErrorKind::Validation("agent has no container to resume".into()).into())
    }

    pub(crate) fn running_container_id(container_id: Option<&str>) -> AppResult<&str> {
        container_id.ok_or_else(|| ErrorKind::Validation("agent has no running container".into()).into())
    }

    pub(crate) fn stale_container_reference_error() -> ErrorKind {
        ErrorKind::Validation("agent container is no longer available; start the agent again".into())
    }

    pub(crate) fn restart_plan(state: AgentContainerRuntimeState) -> AgentRestartPlan {
        match state {
            AgentContainerRuntimeState::Running => AgentRestartPlan::StopThenStart,
            AgentContainerRuntimeState::NotRunning => AgentRestartPlan::StartOnly,
        }
    }
}

pub(crate) struct AgentContainerRuntimePolicy;

impl AgentContainerRuntimePolicy {
    pub(crate) fn control_docker_unavailable() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Docker not available"))
    }

    pub(crate) fn lifecycle_docker_unavailable() -> ErrorKind {
        ErrorKind::Unavailable("Docker runtime is not available".into())
    }

    pub(crate) fn create_container_failed(
        image: &str,
        cli_tool: Option<&str>,
        missing_image: bool,
        err: impl std::fmt::Display,
    ) -> ErrorKind {
        if missing_image {
            let tool = cli_tool.unwrap_or("claude");
            return ErrorKind::Validation(format!(
                "agent image '{image}' is not installed on this host; run `make update-agents AGENT_TOOLS={tool}` or `make build-agent CLI_TOOL={tool}` before starting this agent"
            ));
        }
        ErrorKind::Internal(anyhow::anyhow!("Failed to create container: {err}"))
    }

    pub(crate) fn start_container_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Failed to start container: {err}"))
    }

    pub(crate) fn stop_container_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Failed to stop container: {err}"))
    }

    pub(crate) fn remove_container_after_stop_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("Failed to remove container after stop: {err}"))
    }

    pub(crate) fn prepare_workspace_failed(path: impl std::fmt::Display, err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("failed to prepare agent workspace {path}: {err}"))
    }

    pub(crate) fn prepare_working_directory_failed(
        path: impl std::fmt::Display,
        err: impl std::fmt::Display,
    ) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("failed to prepare agent working directory {path}: {err}"))
    }

    pub(crate) fn lifecycle_action_unavailable(action: &str, err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Unavailable(format!("failed to {action} agent container: {err}"))
    }

    pub(crate) fn resume_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("resume failed: {err}"))
    }
}

impl AgentLifecycle {
    pub(crate) fn transition(from: AgentStatus, to: AgentStatus) -> AppResult<AgentStatusTransition> {
        if from == to {
            return Ok(AgentStatusTransition::Noop);
        }

        if Self::is_valid_transition(from, to) {
            return Ok(AgentStatusTransition::Change(to));
        }

        Err(ErrorKind::Validation(format!("invalid status transition: {from:?} -> {to:?}")).into())
    }

    pub(crate) fn is_valid_transition(from: AgentStatus, to: AgentStatus) -> bool {
        matches!(
            (from, to),
            (AgentStatus::Idle, AgentStatus::Working)
                | (AgentStatus::Idle, AgentStatus::Offline)
                | (AgentStatus::Working, AgentStatus::Idle)
                | (AgentStatus::Working, AgentStatus::Offline)
                | (AgentStatus::Offline, AgentStatus::Idle)
                | (AgentStatus::Offline, AgentStatus::Working)
        )
    }
}

/// Collaborator permission inside the Agent bounded context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentCollaboratorPermission {
    View,
    Edit,
    Admin,
}

impl AgentCollaboratorPermission {
    pub(crate) fn parse(permission: &str) -> AppResult<Self> {
        match permission {
            "view" => Ok(Self::View),
            "edit" => Ok(Self::Edit),
            "admin" => Ok(Self::Admin),
            _ => Err(ErrorKind::Validation("permission must be 'view', 'edit', or 'admin'".into()).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
            Self::Admin => "admin",
        }
    }

    fn allows(self, action: AgentPermissionAction) -> bool {
        match action {
            AgentPermissionAction::View => true,
            AgentPermissionAction::Edit => matches!(self, Self::Edit | Self::Admin),
            AgentPermissionAction::Admin => matches!(self, Self::Admin),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentPermissionAction {
    View,
    Edit,
    Admin,
}

impl AgentPermissionAction {
    fn parse(action: &str) -> Option<Self> {
        match action {
            "view" => Some(Self::View),
            "edit" => Some(Self::Edit),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// Agent ownership and collaborator access policy.
pub(crate) struct AgentAccessPolicy;

impl AgentAccessPolicy {
    pub(crate) fn has_permission(is_owner: bool, collaborator_permission: Option<&str>, action: &str) -> bool {
        if is_owner {
            return true;
        }

        let Some(action) = AgentPermissionAction::parse(action) else {
            return false;
        };

        collaborator_permission
            .and_then(|permission| AgentCollaboratorPermission::parse(permission).ok())
            .is_some_and(|permission| permission.allows(action))
    }
}

pub(crate) fn agent_permission_projection(
    is_owner: bool,
    collaborator_permission: Option<&str>,
    action: &str,
) -> AgentPermissionProjection {
    AgentPermissionProjection {
        has_permission: AgentAccessPolicy::has_permission(is_owner, collaborator_permission, action),
        is_owner,
        permission: collaborator_permission.map(str::to_string),
    }
}

/// Command subject used by the sidecar command bus.
pub(crate) struct AgentCommandSubject;

impl AgentCommandSubject {
    pub(crate) fn for_agent_id(agent_id: &str) -> String {
        format!("sidecar.{agent_id}.cmd")
    }
}

/// Plain-text prompt currently supported by the Rust Agent path.
#[derive(Debug)]
pub(crate) struct PlainTextAgentPrompt<'a> {
    content: &'a str,
}

impl<'a> PlainTextAgentPrompt<'a> {
    pub(crate) fn new(content: &'a str, images: Option<&[String]>) -> AppResult<Self> {
        if content.trim().is_empty() {
            return Err(ErrorKind::Validation("prompt content is required".into()).into());
        }

        if images.is_some_and(|images| !images.is_empty()) {
            return Err(ErrorKind::Validation("prompt images are not supported yet".into()).into());
        }

        Ok(Self { content })
    }

    pub(crate) fn content(&self) -> &'a str {
        self.content
    }
}

/// Prompt sent through the internal MCP Agent tool bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct McpAgentPrompt<'a> {
    content: &'a str,
}

impl<'a> McpAgentPrompt<'a> {
    pub(crate) fn parse(content: &'a str) -> AppResult<Self> {
        if content.trim().is_empty() {
            return Err(ErrorKind::Validation("prompt is required".into()).into());
        }
        Ok(Self { content })
    }

    pub(crate) fn content(self) -> &'a str {
        self.content
    }
}

/// Container runtime defaults for MCP-backed agents.
pub(crate) struct McpAgentRuntimePolicy;

impl McpAgentRuntimePolicy {
    pub(crate) fn image_for_tool(cli_tool: &str, default_image: &str, tool_images: &HashMap<String, String>) -> String {
        tool_images.get(cli_tool).cloned().unwrap_or_else(|| default_image.to_string())
    }

    pub(crate) fn system_env_for_tool(
        cli_tool: &str,
        system_api_keys: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut env = HashMap::from([
            ("AGENTFORGE_CLI_TOOL".to_string(), cli_tool.to_string()),
            ("AGENTFORGE_GIT_LFS_SKIP".to_string(), "true".to_string()),
        ]);
        if cli_tool == "gemini" {
            env.insert("GEMINI_CLI_NO_RELAUNCH".to_string(), "true".to_string());
        }
        if let Some(name) = ContainerCliCredentialPolicy::api_key_env_for_tool(cli_tool)
            && let Some(value) = system_api_keys.get(name)
        {
            env.insert(name.to_string(), value.clone());
        }
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transition_plans_noops_and_changes() {
        assert_eq!(
            AgentLifecycle::transition(AgentStatus::Idle, AgentStatus::Idle).unwrap(),
            AgentStatusTransition::Noop
        );
        assert_eq!(
            AgentLifecycle::transition(AgentStatus::Idle, AgentStatus::Working).unwrap(),
            AgentStatusTransition::Change(AgentStatus::Working)
        );
    }

    #[test]
    fn agent_container_lifecycle_policy_maps_runtime_states_to_restart_plans() {
        assert_eq!(
            AgentContainerLifecyclePolicy::restart_plan(AgentContainerRuntimeState::Running),
            AgentRestartPlan::StopThenStart
        );
        assert_eq!(
            AgentContainerLifecyclePolicy::restart_plan(AgentContainerRuntimeState::NotRunning),
            AgentRestartPlan::StartOnly
        );
    }

    #[test]
    fn agent_container_lifecycle_policy_validates_container_backing_and_ids() {
        assert!(AgentContainerLifecyclePolicy::ensure_container_backed(Some("claude")).is_ok());
        assert!(AgentContainerLifecyclePolicy::ensure_container_backed(None).is_err());
        assert_eq!(AgentContainerLifecyclePolicy::restart_container_id(Some("ctr-1")).unwrap(), "ctr-1");
        assert!(AgentContainerLifecyclePolicy::restart_container_id(None).is_err());
        assert_eq!(AgentContainerLifecyclePolicy::resume_container_id(Some("ctr-2")).unwrap(), "ctr-2");
        assert!(AgentContainerLifecyclePolicy::resume_container_id(None).is_err());
        assert_eq!(AgentContainerLifecyclePolicy::running_container_id(Some("ctr-3")).unwrap(), "ctr-3");
        assert!(AgentContainerLifecyclePolicy::running_container_id(None).is_err());
    }

    #[test]
    fn agent_container_runtime_policy_owns_docker_error_contracts() {
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::control_docker_unavailable()).contains("Docker not available")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::lifecycle_docker_unavailable())
                .contains("Docker runtime is not available")
        );
        assert!(
            format!(
                "{:?}",
                AgentContainerRuntimePolicy::create_container_failed(
                    "agentforge-agent:codex",
                    Some("codex"),
                    true,
                    "missing",
                )
            )
            .contains("make update-agents AGENT_TOOLS=codex")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::create_container_failed("image", None, false, "bad"))
                .contains("Failed to create container")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::start_container_failed("bad"))
                .contains("Failed to start container")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::stop_container_failed("bad"))
                .contains("Failed to stop container")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::remove_container_after_stop_failed("bad"))
                .contains("Failed to remove container after stop")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::prepare_workspace_failed("/tmp/ws", "bad"))
                .contains("failed to prepare agent workspace")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::prepare_working_directory_failed("/tmp/cwd", "bad"))
                .contains("failed to prepare agent working directory")
        );
        assert!(
            format!("{:?}", AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect", "bad"))
                .contains("failed to inspect agent container")
        );
        assert!(format!("{:?}", AgentContainerRuntimePolicy::resume_failed("bad")).contains("resume failed"));
    }

    #[test]
    fn agent_container_image_policy_owns_start_error_contract() {
        let err = AgentContainerImagePolicy::resolve_for_start(None, None).expect_err("missing shell should fail");
        assert!(format!("{:?}", err.kind).contains("agent has no cli_tool"));

        let err = AgentContainerImagePolicy::resolve_for_start(Some("vim"), None).expect_err("unknown cli should fail");
        assert!(format!("{:?}", err.kind).contains("set cli_tool to one of"));
    }

    #[test]
    fn agent_message_page_fetches_one_extra_and_drops_oldest_extra() {
        let page = AgentMessagePage::new(2);
        assert_eq!(page.fetch_limit(), 3);

        let (rows, has_more) = page.split_has_more(vec!["oldest-extra", "first", "second"]);

        assert!(has_more);
        assert_eq!(rows, vec!["first", "second"]);
    }

    #[test]
    fn access_policy_allows_owner_for_known_and_unknown_actions() {
        assert!(AgentAccessPolicy::has_permission(true, None, "view"));
        assert!(AgentAccessPolicy::has_permission(true, None, "unknown"));
    }

    #[test]
    fn access_policy_maps_collaborator_permissions() {
        assert!(AgentAccessPolicy::has_permission(false, Some("view"), "view"));
        assert!(!AgentAccessPolicy::has_permission(false, Some("view"), "edit"));
        assert!(AgentAccessPolicy::has_permission(false, Some("edit"), "edit"));
        assert!(!AgentAccessPolicy::has_permission(false, Some("edit"), "admin"));
        assert!(AgentAccessPolicy::has_permission(false, Some("admin"), "admin"));
        assert!(!AgentAccessPolicy::has_permission(false, Some("owner"), "view"));
        assert!(!AgentAccessPolicy::has_permission(false, Some("admin"), "unknown"));
    }

    #[test]
    fn cli_tool_selection_canonicalizes_supported_tools() {
        assert_eq!(AgentCliToolSelection::normalize(Some(" Codex ")).unwrap(), Some("codex"));
        assert_eq!(AgentCliToolSelection::normalize(None).unwrap(), None);
        assert!(AgentCliToolSelection::normalize(Some("unknown")).is_err());
    }

    #[test]
    fn container_image_policy_uses_cli_tool() {
        assert_eq!(AgentContainerImagePolicy::resolve(Some("claude"), None).unwrap(), "agentforge-agent:claude");
        assert_eq!(AgentContainerImagePolicy::resolve(Some("CODEX"), None).unwrap(), "agentforge-agent:codex");
    }

    #[test]
    fn container_image_policy_rejects_unknown_cli_tool() {
        assert!(matches!(
            AgentContainerImagePolicy::resolve(Some("vim"), None),
            Err(AgentContainerImageRejection::UnsupportedCliTool(_))
        ));
    }

    #[test]
    fn container_image_policy_accepts_legacy_agent_image_model() {
        assert_eq!(
            AgentContainerImagePolicy::resolve(None, Some("agentforge-agent:claude")).unwrap(),
            "agentforge-agent:claude"
        );
    }

    #[test]
    fn container_image_policy_rejects_raw_model_or_missing_metadata() {
        assert!(matches!(
            AgentContainerImagePolicy::resolve(None, Some("claude-sonnet-4-6")),
            Err(AgentContainerImageRejection::MissingContainerShell)
        ));
        assert!(matches!(
            AgentContainerImagePolicy::resolve(None, None),
            Err(AgentContainerImageRejection::MissingContainerShell)
        ));
    }

    #[test]
    fn agent_container_env_strips_user_info_from_nats_url() {
        assert_eq!(
            AgentContainerEnvPolicy::strip_nats_url_user_info("nats://backend:pw@nats:4222").unwrap(),
            "nats://nats:4222"
        );
        assert_eq!(AgentContainerEnvPolicy::strip_nats_url_user_info("nats://nats:4222").unwrap(), "nats://nats:4222");
        assert_eq!(AgentContainerEnvPolicy::strip_nats_url_user_info("not-a-url"), None);
    }

    #[test]
    fn agent_container_env_picks_agent_nats_url_before_backend_url() {
        let agent = Some("nats://agent:pw1@nats:4222");
        let backend = Some("nats://backend:pw2@nats:4222");

        assert_eq!(AgentContainerEnvPolicy::pick_nats_base_url(agent, backend).unwrap(), "nats://nats:4222");
        assert_eq!(
            AgentContainerEnvPolicy::pick_nats_base_url(None, Some("nats://backend:pw@nats:4222")).unwrap(),
            "nats://nats:4222"
        );
        assert_eq!(AgentContainerEnvPolicy::pick_nats_base_url(None, None), None);
    }

    #[test]
    fn agent_container_env_includes_identity_and_hmac_only_without_nats() {
        let agent = Uuid::new_v4();
        let org = Uuid::new_v4();
        let nats_credential = ["pw", "-ignored"].concat();
        let hmac_value = ["secret", "-xyz"].concat();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent,
            org_id: org,
            cli_tool: Some("claude"),
            cli_model: None,
            codex_default_model: None,
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: &hmac_value,
            context_injection_enabled: false,
        });

        assert!(env.contains(&format!("AGENT_ID={agent}")));
        assert!(env.contains(&format!("ORG_ID={org}")));
        assert!(env.contains(&format!("HMAC_SECRET={hmac_value}")));
        assert!(env.contains(&"AGENTFORGE_CONTEXT_INJECTION_ENABLED=false".to_string()));
        assert!(env.contains(&"CLI_TOOL=claude".to_string()));
        assert!(!env.iter().any(|v| v.starts_with("AGENTFORGE_NATS_URL=")));
        assert!(!env.iter().any(|v| v.starts_with("NATS_URL=")));
        assert!(!env.iter().any(|v| v.starts_with("AGENTFORGE_SERVER_URL=")));
    }

    #[test]
    fn agent_container_env_injects_per_agent_nats_url() {
        let agent = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let nats_credential = ["pw", "-abc", "-123"].concat();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent,
            org_id: Uuid::new_v4(),
            cli_tool: Some("codex"),
            cli_model: Some("gpt-5.4-mini"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: Some("nats://nats:4222"),
            nats_connect_password: &nats_credential,
            container_server_url: Some("http://agentforge:4003"),
            workspace_host_path: Some("/data/agentforge/workspaces/orgs/o/workspaces/w/projects"),
            hmac_secret: "hmac",
            context_injection_enabled: true,
        });
        let expected = format!("nats://11111111-2222-3333-4444-555555555555:{nats_credential}@nats:4222");

        assert!(env.contains(&format!("AGENTFORGE_NATS_URL={expected}")), "missing AGENTFORGE_NATS_URL in {env:?}");
        assert!(env.contains(&format!("NATS_URL={expected}")), "missing NATS_URL in {env:?}");
        assert!(env.contains(&"AGENTFORGE_SERVER_URL=http://agentforge:4003".to_string()));
        assert!(env.contains(
            &"AGENTFORGE_WORKSPACE_HOST_PATH=/data/agentforge/workspaces/orgs/o/workspaces/w/projects".to_string()
        ));
        assert!(env.contains(&"AGENTFORGE_CONTEXT_INJECTION_ENABLED=true".to_string()));
        assert!(env.contains(&"CLI_TOOL=codex".to_string()));
        assert!(env.contains(&"AGENTFORGE_CLI_MODEL=gpt-5.4-mini".to_string()));
    }

    #[test]
    fn agent_container_env_nats_url_never_leaks_shared_credentials() {
        let shared_url = ["nats://backend:", "super-secret", "@nats:4222"].concat();
        let base = AgentContainerEnvPolicy::pick_nats_base_url(None, Some(&shared_url));
        let nats_credential = ["per-agent", "-pw"].concat();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some("claude"),
            cli_model: None,
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: base.as_deref(),
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac",
            context_injection_enabled: false,
        });

        for entry in &env {
            assert!(!entry.contains("backend:super-secret"), "leaked shared backend creds in env entry {entry}");
            assert!(!entry.contains("super-secret@"), "leaked shared backend creds in env entry {entry}");
        }
    }

    #[test]
    fn agent_container_env_skips_unknown_cli_tool() {
        let nats_credential = "pw".to_string();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: None,
            cli_model: Some("gpt-5.4-mini"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac",
            context_injection_enabled: false,
        });

        assert!(!env.iter().any(|v| v.starts_with("CLI_TOOL=")));
        assert!(!env.iter().any(|v| v.starts_with("AGENTFORGE_CLI_MODEL=")));
    }

    #[test]
    fn agent_container_env_uses_codex_default_for_legacy_image_model_values() {
        let nats_credential = "pw".to_string();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some("codex"),
            cli_model: Some("agentforge-agent:codex"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac",
            context_injection_enabled: false,
        });

        assert!(env.contains(&"AGENTFORGE_CLI_MODEL=gpt-5.5".to_string()));
    }

    #[test]
    fn agent_container_env_canonicalizes_legacy_cli_tool_values() {
        let nats_credential = "pw".to_string();
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some(" CODEX "),
            cli_model: None,
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac",
            context_injection_enabled: false,
        });

        assert!(env.contains(&"CLI_TOOL=codex".to_string()));
        assert!(env.contains(&"AGENTFORGE_CLI_MODEL=gpt-5.5".to_string()));
    }

    #[test]
    fn agent_container_env_creds_dir_matches_entrypoint_paths() {
        assert_eq!(AgentContainerEnvPolicy::creds_dir_for_cli_tool("claude"), Some("/home/agent/.claude"));
        assert_eq!(AgentContainerEnvPolicy::creds_dir_for_cli_tool("gemini"), Some("/home/agent/.gemini"));
        assert_eq!(
            AgentContainerEnvPolicy::creds_dir_for_cli_tool("opencode"),
            Some("/home/agent/.local/share/opencode")
        );
        assert_eq!(AgentContainerEnvPolicy::creds_dir_for_cli_tool("codex"), Some("/home/agent/.codex"));
        assert_eq!(AgentContainerEnvPolicy::creds_dir_for_cli_tool("unknown"), None);
    }

    #[test]
    fn plain_text_prompt_rejects_unsupported_shapes() {
        assert!(PlainTextAgentPrompt::new("hello", None).is_ok());
        assert!(PlainTextAgentPrompt::new("   ", None).is_err());
        assert!(PlainTextAgentPrompt::new("hello", Some(&["base64".to_string()])).is_err());
    }

    #[test]
    fn sidecar_command_payloads_keep_protocol_shape() {
        assert_eq!(agent_prompt_command_payload("ship")["type"], "prompt");
        assert_eq!(agent_prompt_command_payload("ship")["prompt"], "ship");
        assert_eq!(agent_interrupt_command_payload()["type"], "interrupt");
    }

    #[test]
    fn mcp_agent_prompt_requires_content_without_rewriting_value() {
        assert_eq!(McpAgentPrompt::parse(" ship it ").unwrap().content(), " ship it ");
        assert!(McpAgentPrompt::parse("   ").is_err());
    }

    #[test]
    fn mcp_agent_runtime_policy_selects_tool_image_or_default() {
        let tool_images = HashMap::from([("codex".to_string(), "agentforge/codex:latest".to_string())]);

        assert_eq!(
            McpAgentRuntimePolicy::image_for_tool("codex", "agentforge/default:latest", &tool_images),
            "agentforge/codex:latest"
        );
        assert_eq!(
            McpAgentRuntimePolicy::image_for_tool("claude", "agentforge/default:latest", &tool_images),
            "agentforge/default:latest"
        );
    }

    #[test]
    fn mcp_agent_runtime_policy_builds_cli_env() {
        let env = McpAgentRuntimePolicy::system_env_for_tool("gemini", &HashMap::new());

        assert_eq!(env.get("AGENTFORGE_CLI_TOOL").map(String::as_str), Some("gemini"));
        assert_eq!(env.get("AGENTFORGE_GIT_LFS_SKIP").map(String::as_str), Some("true"));
        assert_eq!(env.get("GEMINI_CLI_NO_RELAUNCH").map(String::as_str), Some("true"));
    }

    #[test]
    fn mcp_agent_runtime_policy_injects_matching_system_api_key_only() {
        let env = McpAgentRuntimePolicy::system_env_for_tool(
            "codex",
            &HashMap::from([("OPENAI_API_KEY".to_string(), "sk-test".to_string())]),
        );

        assert_eq!(env.get("OPENAI_API_KEY").map(String::as_str), Some("sk-test"));
        assert!(!env.contains_key("ANTHROPIC_API_KEY"));
    }
}
