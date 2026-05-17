//! Agent domain rules.
//!
//! This module owns the Agent bounded-context policies that are independent of
//! HTTP handlers, SQL repositories, Docker clients, and message buses.

use std::collections::HashMap;

use agentforge_core::{AgentStatus, AppResult, CliToolKind, ErrorKind};

use crate::domain::credential::ContainerCliCredentialPolicy;

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
}

/// Agent lifecycle policy.
pub(crate) struct AgentLifecycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentStatusTransition {
    Noop,
    Change(AgentStatus),
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
    fn plain_text_prompt_rejects_unsupported_shapes() {
        assert!(PlainTextAgentPrompt::new("hello", None).is_ok());
        assert!(PlainTextAgentPrompt::new("   ", None).is_err());
        assert!(PlainTextAgentPrompt::new("hello", Some(&["base64".to_string()])).is_err());
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
