//! Runtime capability profile for Container CLIs and provider-backed agents.
//!
//! The profile is intentionally typed in `core` so context injection can record
//! what the selected runtime actually supports instead of inferring behavior
//! from stringly `cli_tool` labels.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// Container CLI tool kind supported by the managed agent runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliToolKind {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl CliToolKind {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Gemini, Self::Opencode];
    pub const SUPPORTED_SLUGS: &'static str = "claude|codex|gemini|opencode";

    /// Stable DB/API slug.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
        }
    }

    /// Parse legacy DB/API strings while canonicalizing casing and whitespace.
    pub fn parse_legacy(raw: &str) -> Result<Self, RuntimeCapabilityError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "opencode" => Ok(Self::Opencode),
            _ => Err(RuntimeCapabilityError::UnknownCliTool { raw: raw.trim().to_string() }),
        }
    }

    fn max_context_tokens(self) -> u32 {
        match self {
            Self::Claude => 200_000,
            Self::Codex => 200_000,
            Self::Gemini => 1_000_000,
            Self::Opencode => 128_000,
        }
    }

    fn supports_hooks(self) -> bool {
        matches!(self, Self::Claude | Self::Codex | Self::Gemini)
    }

    fn supports_subagents(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }
}

impl fmt::Display for CliToolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CliToolKind {
    type Err = RuntimeCapabilityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_legacy(s)
    }
}

/// Execution surface that receives the context envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Container,
    Cli,
    Api,
}

impl RuntimeKind {
    pub const SUPPORTED_SLUGS: &'static str = "container|cli|api";

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Cli => "cli",
            Self::Api => "api",
        }
    }

    /// Parse DB/API strings while canonicalizing casing and whitespace.
    pub fn parse_legacy(raw: &str) -> Result<Self, RuntimeCapabilityError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "container" => Ok(Self::Container),
            "cli" => Ok(Self::Cli),
            "api" => Ok(Self::Api),
            _ => Err(RuntimeCapabilityError::UnknownRuntimeKind { raw: raw.trim().to_string() }),
        }
    }
}

impl fmt::Display for RuntimeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuntimeKind {
    type Err = RuntimeCapabilityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_legacy(s)
    }
}

/// Immutable capability snapshot recorded for a run or provider call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_tool: Option<CliToolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    pub runtime_kind: RuntimeKind,
    pub max_context_tokens: u32,
    pub supports_skills_mount: bool,
    pub supports_hooks: bool,
    pub supports_subagents: bool,
    pub supports_mcp_bridge: bool,
    pub supports_terminal: bool,
}

impl RuntimeCapability {
    /// Built-in capability matrix for every supported Container CLI and local
    /// CLI runtime. Provider-backed API runtimes are dynamic and supplied by
    /// the provider layer, so they are not part of the startup DB seed.
    pub fn all() -> Vec<Self> {
        let mut profiles = Vec::with_capacity(CliToolKind::ALL.len() * 2);
        for cli_tool in CliToolKind::ALL {
            profiles.push(Self::for_cli_tool(cli_tool, RuntimeKind::Container));
            profiles.push(Self::for_cli_tool(cli_tool, RuntimeKind::Cli));
        }
        profiles
    }

    /// Create a capability profile after validating the context window.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cli_tool: Option<CliToolKind>,
        runtime_kind: RuntimeKind,
        max_context_tokens: u32,
        supports_skills_mount: bool,
        supports_hooks: bool,
        supports_subagents: bool,
        supports_mcp_bridge: bool,
        supports_terminal: bool,
    ) -> Result<Self, RuntimeCapabilityError> {
        if max_context_tokens == 0 {
            return Err(RuntimeCapabilityError::MaxContextTokensZero { runtime_kind });
        }

        Ok(Self {
            cli_tool,
            provider_name: None,
            runtime_kind,
            max_context_tokens,
            supports_skills_mount,
            supports_hooks,
            supports_subagents,
            supports_mcp_bridge,
            supports_terminal,
        })
    }

    /// Built-in capability profile for a supported Container CLI or local CLI.
    pub fn for_cli_tool(cli_tool: CliToolKind, runtime_kind: RuntimeKind) -> Self {
        let is_container = matches!(runtime_kind, RuntimeKind::Container);
        Self {
            cli_tool: Some(cli_tool),
            provider_name: None,
            runtime_kind,
            max_context_tokens: cli_tool.max_context_tokens(),
            supports_skills_mount: is_container,
            supports_hooks: is_container && cli_tool.supports_hooks(),
            supports_subagents: is_container && cli_tool.supports_subagents(),
            supports_mcp_bridge: is_container,
            supports_terminal: is_container,
        }
    }

    /// Conservative fallback for unsupported or unavailable runtime matrix
    /// rows. It keeps the key visible while disabling all optional features.
    pub fn fallback_for_cli_tool(cli_tool: CliToolKind, runtime_kind: RuntimeKind) -> Self {
        Self {
            cli_tool: Some(cli_tool),
            provider_name: None,
            runtime_kind,
            max_context_tokens: 1,
            supports_skills_mount: false,
            supports_hooks: false,
            supports_subagents: false,
            supports_mcp_bridge: false,
            supports_terminal: false,
        }
    }

    /// Conservative default for provider-backed API runtimes.
    pub fn api_default(provider_name: impl Into<String>) -> Self {
        let provider_name = provider_name.into();
        Self {
            cli_tool: None,
            provider_name: (!provider_name.trim().is_empty()).then_some(provider_name),
            runtime_kind: RuntimeKind::Api,
            max_context_tokens: 4_096,
            supports_skills_mount: false,
            supports_hooks: false,
            supports_subagents: false,
            supports_mcp_bridge: false,
            supports_terminal: false,
        }
    }

    /// Provider-backed API runtime profile with fallback to the conservative
    /// default if the supplied limit is invalid.
    pub fn api_provider_or_default(provider_name: impl Into<String>, max_context_tokens: u32) -> Self {
        let provider_name = provider_name.into();
        Self::api_provider(provider_name.clone(), max_context_tokens)
            .unwrap_or_else(|_| Self::api_default(provider_name))
    }

    /// Provider-backed API runtime profile with an explicit context window.
    pub fn api_provider(
        provider_name: impl Into<String>,
        max_context_tokens: u32,
    ) -> Result<Self, RuntimeCapabilityError> {
        let mut profile = Self::new(None, RuntimeKind::Api, max_context_tokens, false, false, false, false, false)?;
        let provider_name = provider_name.into();
        if !provider_name.trim().is_empty() {
            profile.provider_name = Some(provider_name);
        }
        Ok(profile)
    }
}

/// Errors emitted while parsing or constructing runtime capability profiles.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeCapabilityError {
    #[error("unsupported cli_tool: {raw} (expected {})", CliToolKind::SUPPORTED_SLUGS)]
    UnknownCliTool { raw: String },
    #[error("unsupported runtime_kind: {raw} (expected {})", RuntimeKind::SUPPORTED_SLUGS)]
    UnknownRuntimeKind { raw: String },
    #[error("max_context_tokens must be greater than zero for {runtime_kind} runtime")]
    MaxContextTokensZero { runtime_kind: RuntimeKind },
}

impl RuntimeCapabilityError {
    /// Stable metric/status label for controlled runtime-capability failures.
    pub fn status_label(&self) -> &'static str {
        match self {
            Self::UnknownCliTool { .. } => "cli_tool_unknown",
            Self::UnknownRuntimeKind { .. } => "runtime_kind_unknown",
            Self::MaxContextTokensZero { .. } => "runtime_capability_invalid",
        }
    }
}
