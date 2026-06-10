//! Agent domain rules.
//!
//! This module owns the Agent bounded-context policies that are independent of
//! HTTP handlers, SQL repositories, Docker clients, and message buses.

use std::collections::{BTreeMap, HashMap};

use agentforge_core::{AgentId, AgentStatus, AppError, AppResult, CliToolKind, ErrorKind, RuntimeKind, TenantScope};
use chrono::{DateTime, Utc};
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

pub(crate) fn agent_enrollment_response<T: Serialize, U: Serialize>(agent: T, enrollment: U) -> Value {
    json!({ "ok": true, "agent": agent, "enrollment": enrollment })
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

/// JSON body for a successful join-code claim. Flat (no `data` wrapper) so
/// the PowerShell bootstrap reads `.env` / `.cliTool` directly.
pub(crate) fn agent_join_claim_response(claimed: ClaimedHostAgentJoin) -> Value {
    json!({
        "ok": true,
        "agentId": claimed.agent_id,
        "agentName": claimed.agent_name,
        "cliTool": claimed.cli_tool,
        "env": claimed.env,
        "sidecarCommand": claimed.sidecar_command,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostAgentEnrollment {
    pub(crate) agent_id: Uuid,
    pub(crate) runtime_id: String,
    pub(crate) cli_tool: String,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) shell_exports: String,
    pub(crate) sidecar_command: String,
    pub(crate) server_url: Option<String>,
    /// One-command join pairing code (plaintext, never persisted). Present
    /// whenever a code was minted for this enrollment.
    pub(crate) join_code: Option<String>,
    pub(crate) join_code_expires_at: Option<DateTime<Utc>>,
    /// Copy-paste join command for macOS/Linux. Requires `server_url`.
    pub(crate) join_command: Option<String>,
    /// Copy-paste join command for Windows PowerShell. Requires `server_url`.
    pub(crate) join_command_powershell: Option<String>,
}

/// What a successful join-code claim hands the bootstrap script. Carries the
/// same env a fresh enrollment would produce, plus ready-to-eval export-line
/// renderings so the script needs no JSON tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaimedHostAgentJoin {
    pub(crate) agent_id: Uuid,
    pub(crate) agent_name: Option<String>,
    pub(crate) cli_tool: Option<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) shell_export_lines: String,
    pub(crate) powershell_export_lines: String,
    pub(crate) sidecar_command: String,
}

pub(crate) struct HostAgentEnrollmentPolicy;

impl HostAgentEnrollmentPolicy {
    pub(crate) const SIDECAR_COMMAND: &'static str = "agentforge-sidecar";

    pub(crate) fn require_cli_tool(raw: &str) -> AppResult<&'static str> {
        AgentCliToolSelection::normalize(Some(raw))?
            .ok_or_else(|| ErrorKind::Validation("cliTool is required for Host CLI enrollment".into()).into())
    }

    /// Returned when a non-`tls://` NATS URL is presented during Host CLI
    /// enrollment and `allow_plaintext_host_nats` is not set.
    pub(crate) fn plaintext_nats_blocked_error() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.agent.enroll.plaintext_nats_blocked",
            message: "Host CLI enrollment requires a tls:// NATS URL. Configure \
                      NATS_AGENT_URL to use tls://, or set ALLOW_PLAINTEXT_HOST_NATS=true \
                      to permit plaintext (dev/test only)."
                .into(),
        }
        .into()
    }

    /// Returned when the `cli_tool` value supplied at enrollment cannot be
    /// mapped to a known [`CliToolKind`].
    pub(crate) fn unknown_cli_tool_error(cli_tool: &str) -> AppError {
        ErrorKind::Validation(format!("unknown cli_tool: {cli_tool}")).into()
    }

    pub(crate) fn require_nats_base_url(agent_url: Option<&str>, backend_url: Option<&str>) -> AppResult<String> {
        AgentContainerEnvPolicy::pick_nats_base_url(agent_url, backend_url)
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| {
                ErrorKind::Validation(
                    "NATS_AGENT_URL or NATS_URL must be configured before enrolling a Host CLI agent".into(),
                )
                .into()
            })
    }

    pub(crate) fn env_map(env: Vec<String>) -> BTreeMap<String, String> {
        env.into_iter()
            .filter_map(|entry| {
                let (key, value) = entry.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect()
    }

    pub(crate) fn shell_exports(env: &BTreeMap<String, String>) -> String {
        env.iter()
            .map(|(key, value)| format!("export {key}={}", shell_quote(value)))
            .chain(std::iter::once(Self::SIDECAR_COMMAND.to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Bash `export` lines only (no trailing sidecar command) — the payload
    /// the join bootstrap script evals and persists as the agent env file.
    pub(crate) fn shell_export_lines(env: &BTreeMap<String, String>) -> String {
        env.iter().map(|(key, value)| format!("export {key}={}", shell_quote(value))).collect::<Vec<_>>().join("\n")
    }

    /// PowerShell `$env:` lines for the Windows join bootstrap.
    pub(crate) fn powershell_export_lines(env: &BTreeMap<String, String>) -> String {
        env.iter()
            .map(|(key, value)| format!("$env:{key} = {}", powershell_quote(value)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// One-command join for macOS/Linux, modeled on the pairing-code bridge
    /// commands of comparable products: fetch the bootstrap from the
    /// operator's own deployment and pass the pairing code as a flag.
    pub(crate) fn join_command(server_url: &str, code: &str) -> String {
        let base = server_url.trim_end_matches('/');
        format!("curl -fsSL {base}/api/v1/agents/local-join/script | sh -s -- --code {code}")
    }

    /// One-command join for Windows PowerShell. The code travels via an env
    /// var because `irm | iex` cannot take script arguments.
    pub(crate) fn join_command_powershell(server_url: &str, code: &str) -> String {
        let base = server_url.trim_end_matches('/');
        format!("$env:AGENTFORGE_JOIN_CODE = '{code}'; irm {base}/api/v1/agents/local-join/script.ps1 | iex")
    }

    /// Opaque rejection for unknown, expired, or non-host-CLI join codes.
    /// One error shape on purpose: callers are unauthenticated, so the
    /// response must not reveal whether a code ever existed.
    pub(crate) fn invalid_join_code_error() -> AppError {
        ErrorKind::NotFound("join code is invalid or has expired".into()).into()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// A one-command join pairing code: `afj_` + 43 base64url chars (32 random
/// bytes). The plaintext lives only in the enrollment response and the
/// operator's command line; persistence stores [`JoinCode::hash_hex`].
pub(crate) struct JoinCode(String);

impl JoinCode {
    pub(crate) const PREFIX: &'static str = "afj_";
    /// Codes stay claimable this long. Short enough that a leaked command
    /// line goes stale quickly, long enough to download a sidecar binary on
    /// a slow connection and retry once.
    pub(crate) const TTL_SECS: i64 = 15 * 60;

    pub(crate) fn generate() -> Self {
        use base64::Engine as _;
        use rand::RngCore as _;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(format!("{}{}", Self::PREFIX, base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)))
    }

    /// Accepts a presented code for claiming. Shape-validates only — the
    /// authoritative check is the hash lookup. Invalid shapes get the same
    /// opaque error as unknown codes.
    pub(crate) fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim();
        let valid = trimmed.strip_prefix(Self::PREFIX).is_some_and(|tail| {
            (32..=64).contains(&tail.len()) && tail.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
        if !valid {
            return Err(HostAgentEnrollmentPolicy::invalid_join_code_error());
        }
        Ok(Self(trimmed.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Hex SHA-256 of the full code string — the only form that touches the
    /// database.
    pub(crate) fn hash_hex(&self) -> String {
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(self.0.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub(crate) fn expires_at_from(now: DateTime<Utc>) -> DateTime<Utc> {
        now + chrono::Duration::seconds(Self::TTL_SECS)
    }
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("JoinCode").field(&"<redacted>").finish()
    }
}

#[derive(Clone)]
pub struct HostCliIdentity {
    agent_id: Uuid,
    runtime_id: String,
    hmac_secret: String,
    nats_connect_password: String,
}

impl std::fmt::Debug for HostCliIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCliIdentity")
            .field("agent_id", &self.agent_id)
            .field("runtime_id", &self.runtime_id)
            .field("hmac_secret", &"<redacted>")
            .field("nats_connect_password", &"<redacted>")
            .finish()
    }
}

impl HostCliIdentity {
    pub fn generate() -> Self {
        let agent_id = Uuid::now_v7();
        Self {
            runtime_id: format!("host-{agent_id}"),
            hmac_secret: Uuid::new_v4().to_string(),
            nats_connect_password: Uuid::new_v4().to_string(),
            agent_id,
        }
    }

    pub fn agent_id(&self) -> Uuid {
        self.agent_id
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub fn hmac_secret(&self) -> &str {
        &self.hmac_secret
    }

    pub fn nats_connect_password(&self) -> &str {
        &self.nats_connect_password
    }
}

/// Aggregate root for the Agent bounded context. Loaded by
/// `AgentRepository::find_aggregate` for write-side operations (added in Task 4.3).
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct AgentAggregate {
    pub(crate) id: Uuid,
    pub(crate) runtime_kind: RuntimeKind,
    pub(crate) cli_tool: Option<String>,
    pub(crate) container_id: Option<String>,
    pub(crate) runtime_id: Option<String>,
    pub(crate) workspace_id: Uuid,
    pub(crate) organization_id: Uuid,
    pub(crate) user_id: Uuid,
    pub(crate) status: AgentStatus,
}

impl AgentAggregate {
    pub fn runtime_kind(&self) -> RuntimeKind {
        self.runtime_kind
    }

    /// Owner (creator) of this agent. Used by lifecycle ACL checks.
    pub(crate) fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Typed sum-type projection of this aggregate's runtime.
    ///
    /// Domain branching should go through this instead of matching on
    /// [`runtime_kind`](AgentAggregate::runtime_kind) plus inspecting nullable
    /// columns, so illegal field combinations cannot reach lifecycle logic.
    ///
    /// The DB CHECK (`agents_runtime_kind_invariants`) already guarantees the
    /// columns are coherent for the stored `runtime_kind`, so in practice this
    /// conversion always succeeds. It still handles the impossible case
    /// explicitly (defense in depth against schema drift or corrupt inserts):
    /// a `Container`/`Cli` row whose required column is `NULL`, or any row whose
    /// `cli_tool` slug fails to parse, yields a typed error rather than a panic.
    pub(crate) fn runtime(&self) -> AppResult<AgentRuntime> {
        let cli_tool = |label: &str| -> AppResult<CliToolKind> {
            let raw = self.cli_tool.as_deref().ok_or_else(|| {
                AppError::from(ErrorKind::Internal(anyhow::anyhow!(
                    "{label} agent {} is missing cli_tool (DB invariant violated)",
                    self.id
                )))
            })?;
            CliToolKind::parse_legacy(raw).map_err(|err| {
                AppError::from(ErrorKind::Internal(anyhow::anyhow!(
                    "agent {} has unparseable cli_tool {raw:?}: {err}",
                    self.id
                )))
            })
        };

        match self.runtime_kind {
            RuntimeKind::Container => Ok(AgentRuntime::Container {
                cli_tool: cli_tool("Container")?,
                container_id: self.container_id.clone(),
            }),
            RuntimeKind::Cli => {
                let runtime_id = self.runtime_id.clone().ok_or_else(|| {
                    AppError::from(ErrorKind::Internal(anyhow::anyhow!(
                        "Host CLI agent {} is missing runtime_id (DB invariant violated)",
                        self.id
                    )))
                })?;
                Ok(AgentRuntime::HostCli { cli_tool: cli_tool("Host CLI")?, runtime_id })
            }
            RuntimeKind::Api => Ok(AgentRuntime::Api),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(kind: RuntimeKind, cli_tool: Option<&str>, container_id: Option<&str>) -> Self {
        Self::for_test_full(kind, cli_tool, container_id, None)
    }

    #[cfg(test)]
    pub(crate) fn for_test_full(
        kind: RuntimeKind,
        cli_tool: Option<&str>,
        container_id: Option<&str>,
        runtime_id: Option<&str>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            runtime_kind: kind,
            cli_tool: cli_tool.map(str::to_string),
            container_id: container_id.map(str::to_string),
            runtime_id: runtime_id.map(str::to_string),
            workspace_id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            status: AgentStatus::Idle,
        }
    }
}

/// Typed view of an agent's runtime, derived from the normalized aggregate
/// columns. Makes illegal field combinations unrepresentable in domain code:
/// a `Container`/`HostCli` value carries a parsed [`CliToolKind`], an `Api`
/// value carries no `cli_tool` at all, and only `HostCli` carries a
/// `runtime_id`. The DB stays normalized — this is an in-memory projection
/// over the flat `AgentAggregate` columns, not a storage change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentRuntime {
    /// Docker-managed Container CLI agent. `container_id` is absent until the
    /// container has been provisioned.
    Container { cli_tool: CliToolKind, container_id: Option<String> },
    /// Host-enrolled CLI agent. The sidecar runs on the operator machine, so the
    /// platform tracks it by `runtime_id` and never owns a container for it.
    HostCli { cli_tool: CliToolKind, runtime_id: String },
    /// Provider-backed API agent. Has no Container CLI and no container.
    Api,
}

/// Typestate wrapper proving an agent is container-backed.
#[derive(Debug)]
pub(crate) struct ContainerAgent(AgentAggregate);

/// Rejection returned when a lifecycle operation requires a container-backed
/// agent but the aggregate belongs to a different runtime kind.
#[derive(Debug)]
pub(crate) enum LifecycleRejection {
    HostCli,
    Api,
}

impl ContainerAgent {
    pub(crate) fn try_from(agent: AgentAggregate) -> Result<Self, LifecycleRejection> {
        // Branch on the typed runtime sum type. Only the `Container` variant is a
        // valid container-backed agent; `HostCli`/`Api` are rejected with the
        // matching lifecycle variant.
        //
        // `runtime()` only returns `Err` when a stored row violates the DB CHECK
        // invariants (e.g. a `Container` row with `cli_tool = NULL`). That cannot
        // change which runtime *family* the row belongs to, so the rejection
        // decision falls back to `runtime_kind` to keep behavior identical to the
        // previous tag-only match.
        match agent.runtime() {
            Ok(AgentRuntime::Container { .. }) => Ok(Self(agent)),
            Ok(AgentRuntime::HostCli { .. }) => Err(LifecycleRejection::HostCli),
            Ok(AgentRuntime::Api) => Err(LifecycleRejection::Api),
            Err(_) => match agent.runtime_kind {
                RuntimeKind::Container => Ok(Self(agent)),
                RuntimeKind::Cli => Err(LifecycleRejection::HostCli),
                RuntimeKind::Api => Err(LifecycleRejection::Api),
            },
        }
    }

    pub(crate) fn inner(&self) -> &AgentAggregate {
        &self.0
    }
}

impl LifecycleRejection {
    pub(crate) fn into_app_error(self, action: &str) -> AppError {
        let (code, message) = match (self, action) {
            (Self::HostCli, "Restart") => (
                "errors.agent.lifecycle.restart_host_cli",
                "Host CLI agent: restart the sidecar from your machine using the enrollment script.".to_string(),
            ),
            (Self::Api, "Restart") => {
                ("errors.agent.lifecycle.restart_api", "API/provider agent has no container to restart.".to_string())
            }
            (Self::HostCli, "Start") => (
                "errors.agent.lifecycle.start_host_cli",
                "Host CLI agent: start the sidecar from your machine using the enrollment script.".to_string(),
            ),
            (Self::Api, "Start") => {
                ("errors.agent.lifecycle.start_api", "API/provider agent has no container to start.".to_string())
            }
            (Self::HostCli, "Stop") => (
                "errors.agent.lifecycle.stop_host_cli",
                "Host CLI agent: stop the sidecar from your machine.".to_string(),
            ),
            (Self::Api, "Stop") => {
                ("errors.agent.lifecycle.stop_api", "API/provider agent has no container to stop.".to_string())
            }
            (Self::HostCli, "Resume") => (
                "errors.agent.lifecycle.start_host_cli",
                "Host CLI agent: start the sidecar from your machine using the enrollment script.".to_string(),
            ),
            (Self::Api, "Resume") => {
                ("errors.agent.lifecycle.start_api", "API/provider agent has no container to start.".to_string())
            }
            (Self::HostCli, _) => (
                "errors.agent.lifecycle.restart_host_cli",
                format!(
                    "Host CLI agent: the platform does not manage the local container lifecycle. \
                     {action} the sidecar on the operator machine using the enrollment script."
                ),
            ),
            (Self::Api, _) => (
                "errors.agent.lifecycle.restart_api",
                format!("API/provider agent has no container to {}.", action.to_lowercase()),
            ),
        };
        ErrorKind::ValidationWithCode { code, message }.into()
    }
}

/// Typestate proving an agent is an enrolled Host CLI runtime
/// (`runtime_kind == Cli`) WITH its NATS `runtime_id` present. Host-CLI
/// credential issuance / re-issuance accepts only this type, so NATS credential
/// material can never be minted for a container or api aggregate.
///
/// This is the messaging-boundary parallel to [`ContainerAgent`] (the Docker
/// lifecycle boundary): together with the [`AgentRuntime`] sum type it makes a
/// "host-cli without runtime_id" agent unrepresentable on the credential path.
#[derive(Debug)]
pub(crate) struct EnrolledHostCli {
    agent: AgentAggregate,
    runtime_id: String,
}

/// Rejection returned when a credential-issuance operation requires a host-CLI
/// agent but the aggregate belongs to a different runtime kind, or its row is
/// incoherent (DB invariant violated).
#[derive(Debug)]
pub(crate) enum HostCliRejection {
    Container,
    Api,
    /// The stored row violates the `agents_runtime_kind_invariants` CHECK
    /// (e.g. a `Cli` row with `runtime_id = NULL`). Should not occur in
    /// production; mapped to a 500 rather than an operator-facing 4xx.
    Incoherent(AppError),
}

impl EnrolledHostCli {
    pub(crate) fn try_from(agent: AgentAggregate) -> Result<Self, HostCliRejection> {
        // Branch on the typed runtime sum type (#455). Only the `HostCli`
        // variant is a valid messaging-boundary agent; `Container`/`Api` are
        // rejected with the matching variant.
        //
        // The `HostCli` variant carries a non-null `runtime_id` by construction,
        // so we capture it here rather than re-reading the nullable aggregate
        // column. `runtime()` only returns `Err` for an incoherent stored row
        // (DB CHECK violated); that maps to `Incoherent` -> 500, since we cannot
        // safely mint credentials for a row whose invariants are broken.
        match agent.runtime() {
            Ok(AgentRuntime::HostCli { runtime_id, .. }) => Ok(Self { agent, runtime_id }),
            Ok(AgentRuntime::Container { .. }) => Err(HostCliRejection::Container),
            Ok(AgentRuntime::Api) => Err(HostCliRejection::Api),
            Err(err) => match agent.runtime_kind {
                // A `Cli` row that fails `runtime()` is missing required columns
                // (runtime_id/cli_tool) — refuse to issue creds for it.
                RuntimeKind::Cli => Err(HostCliRejection::Incoherent(err)),
                RuntimeKind::Container => Err(HostCliRejection::Container),
                RuntimeKind::Api => Err(HostCliRejection::Api),
            },
        }
    }

    pub(crate) fn inner(&self) -> &AgentAggregate {
        &self.agent
    }

    /// The validated `runtime_id` from the `HostCli` runtime variant.
    ///
    /// The typestate guarantees this is present, so callers never have to handle
    /// the nullable [`AgentAggregate::runtime_id`] column on the credential path.
    pub(crate) fn runtime_id(&self) -> &str {
        &self.runtime_id
    }
}

impl HostCliRejection {
    pub(crate) fn into_app_error(self) -> AppError {
        match self {
            Self::Container => ErrorKind::ValidationWithCode {
                code: "errors.agent.enroll.not_host_cli_container",
                message: "Container agent: NATS credentials are issued for the managed container, \
                          not through Host CLI enrollment."
                    .into(),
            }
            .into(),
            Self::Api => ErrorKind::ValidationWithCode {
                code: "errors.agent.enroll.not_host_cli_api",
                message: "API/provider agent has no Host CLI sidecar and is not issued NATS credentials.".into(),
            }
            .into(),
            // Preserve the underlying internal error (already a 500) so the
            // operator sees a generic message and the cause is logged server-side.
            Self::Incoherent(err) => err,
        }
    }
}

/// Typed aggregate factory for creating new agents.
///
/// Replaces the open-shape `CreateAgentParams` for new code paths. Three
/// constructors encode invariants that differ across runtime surfaces:
/// - [`container`](NewAgent::container): Docker-managed Container CLI agent.
/// - [`host_cli`](NewAgent::host_cli): Host-enrolled CLI agent (carries `HostCliIdentity`).
/// - [`api`](NewAgent::api): Provider-backed API agent (requires provider + model).
#[derive(Debug, Clone)]
pub struct NewAgent {
    runtime_kind: RuntimeKind,
    cli_tool: Option<&'static str>,
    name: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    cwd: Option<String>,
    workspace_id: Uuid,
    project_id: Option<Uuid>,
    system_prompt: Option<String>,
    runtime_id: Option<String>,
    hmac_secret: Option<String>,
    nats_connect_password: Option<String>,
    initial_status: AgentStatus,
}

impl NewAgent {
    pub fn container(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
        system_prompt: Option<&str>,
    ) -> AppResult<Self> {
        let _ = scope;
        AgentName::validate(name)?;
        Ok(Self {
            runtime_kind: RuntimeKind::Container,
            cli_tool: Some(cli_tool.as_str()),
            name: name.map(str::to_string),
            model: model.map(str::to_string),
            provider: None,
            cwd: cwd.map(str::to_string),
            workspace_id,
            project_id,
            system_prompt: system_prompt.map(str::to_string),
            runtime_id: None,
            hmac_secret: None,
            nats_connect_password: None,
            initial_status: AgentStatus::Idle,
        })
    }

    pub fn host_cli(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        identity: HostCliIdentity,
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> {
        let _ = scope;
        AgentName::validate(name)?;
        Ok(Self {
            runtime_kind: RuntimeKind::Cli,
            cli_tool: Some(cli_tool.as_str()),
            name: name.map(str::to_string),
            model: model.map(str::to_string),
            provider: None,
            cwd: cwd.map(str::to_string),
            workspace_id,
            project_id,
            system_prompt: None,
            runtime_id: Some(identity.runtime_id().to_string()),
            hmac_secret: Some(identity.hmac_secret().to_string()),
            nats_connect_password: Some(identity.nats_connect_password().to_string()),
            initial_status: AgentStatus::Offline,
        })
    }

    pub fn api(
        scope: &TenantScope,
        provider: &str,
        model: &str,
        name: Option<&str>,
        system_prompt: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> {
        let _ = scope;
        AgentName::validate(name)?;
        if provider.trim().is_empty() {
            return Err(ErrorKind::Validation("provider is required for API runtime agent".into()).into());
        }
        if model.trim().is_empty() {
            return Err(ErrorKind::Validation("model is required for API runtime agent".into()).into());
        }
        Ok(Self {
            runtime_kind: RuntimeKind::Api,
            cli_tool: None,
            name: name.map(str::to_string),
            model: Some(model.to_string()),
            provider: Some(provider.to_string()),
            cwd: None,
            workspace_id,
            project_id,
            system_prompt: system_prompt.map(str::to_string),
            runtime_id: None,
            hmac_secret: None,
            nats_connect_password: None,
            initial_status: AgentStatus::Idle,
        })
    }

    pub fn runtime_kind(&self) -> RuntimeKind {
        self.runtime_kind
    }

    pub fn cli_tool(&self) -> Option<&str> {
        self.cli_tool
    }

    pub fn runtime_id(&self) -> Option<&str> {
        self.runtime_id.as_deref()
    }

    pub fn hmac_secret(&self) -> Option<&str> {
        self.hmac_secret.as_deref()
    }

    pub fn nats_connect_password(&self) -> Option<&str> {
        self.nats_connect_password.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn workspace_id(&self) -> Uuid {
        self.workspace_id
    }

    pub fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    pub fn initial_status(&self) -> AgentStatus {
        self.initial_status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolStatusProjection {
    docker_available: bool,
}

impl PoolStatusProjection {
    pub(crate) fn new(docker_available: bool) -> Self {
        Self { docker_available }
    }
}

pub(crate) fn pool_status_response(projection: PoolStatusProjection) -> Value {
    // The warm pool (`platform/pool.rs`) is dormant — it is never instantiated
    // and not in the agent-start path, so every agent cold-starts. Report that
    // honestly rather than implying an integration is imminent.
    json!({
        "ok": true,
        "data": {
            "docker_available": projection.docker_available,
            "warm_pool_enabled": false,
            "message": "Warm pool disabled — agents start on demand."
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
            // #457: pin the sidecar's runtime kind explicitly. These are
            // Docker-backed agents, which are always `container` kind in the DB.
            // Making it explicit (rather than relying on the sidecar's default)
            // keeps the published subject `events.ingest.container.<uuid>` in
            // lockstep with the kind the auth callout grants from the DB row —
            // a divergence would get the publish denied by NATS.
            ("AGENTFORGE_RUNTIME_KIND".to_string(), agentforge_core::RuntimeKind::Container.as_str().to_string()),
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

pub(crate) struct AgentRepositoryPolicy;

impl AgentRepositoryPolicy {
    pub(crate) fn agent_not_found(id: AgentId) -> AppError {
        ErrorKind::NotFound(format!("agent {id}")).into()
    }

    pub(crate) fn agent_uuid_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("agent {id}")).into()
    }

    pub(crate) fn project_not_found(project_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("project {project_id}")).into()
    }

    pub(crate) fn workspace_not_found(workspace_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("workspace {workspace_id}")).into()
    }

    pub(crate) fn tenant_context_required() -> AppError {
        ErrorKind::Validation("project or tenant context is required".into()).into()
    }

    pub(crate) fn organization_member_not_found(org_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("organization member for {org_id}")).into()
    }

    pub(crate) fn collaborator_already_exists() -> AppError {
        ErrorKind::Conflict("user is already a collaborator on this agent".into()).into()
    }

    pub(crate) fn collaborator_not_found(agent_id: AgentId, user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("collaborator {user_id} on agent {agent_id}")).into()
    }

    /// Returned when an idempotent replay finds an agent row that is missing its
    /// stored `hmac_secret` or `nats_connect_password` columns.
    pub(crate) fn missing_host_cli_credentials() -> AppError {
        ErrorKind::Internal(anyhow::anyhow!(
            "Host CLI agent is missing stored credentials (hmac_secret or nats_connect_password)"
        ))
        .into()
    }
}

/// Idempotency-Key header policy.
///
/// Encodes the validation contract for the `Idempotency-Key` HTTP header so that
/// the error shape is owned by the domain layer rather than inline in `middleware.rs`.
pub(crate) struct IdempotencyKeyPolicy;

impl IdempotencyKeyPolicy {
    /// Error returned when the `Idempotency-Key` header is absent, empty, or
    /// longer than 256 bytes.
    pub(crate) fn missing_header_error() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.agent.enroll.missing_idempotency_key",
            message: "Idempotency-Key header is required and must be 1–256 characters".into(),
        }
        .into()
    }
}

/// Policy that enforces per-agent owner access control.
///
/// The check prevents intra-org callers who are NOT the agent owner from
/// executing lifecycle mutations. Returning 403 (not 404) is deliberate:
/// the caller is already authenticated within the same org and the agent
/// UUID has been validated by the tenant-scoped DB fetch, so the caller
/// knows the agent exists. Returning 403 does not disclose the runtime kind.
pub struct AgentOwnerPolicy;

impl AgentOwnerPolicy {
    /// Return `Ok(())` when the caller is the agent owner, or a uniform 403
    /// `AppError` that does NOT disclose the agent's runtime kind.
    ///
    /// The error carries the i18n code `errors.agent.lifecycle.not_permitted`
    /// so the frontend can display a localised message from the catalogue.
    ///
    /// `caller_user_id` is taken from `TenantScope::user_id()`.
    pub fn require_owner(caller_user_id: Uuid, owner_id: Uuid) -> AppResult<()> {
        if caller_user_id == owner_id {
            return Ok(());
        }
        Err(ErrorKind::ForbiddenWithCode {
            code: "errors.agent.lifecycle.not_permitted",
            message: "operation not permitted on this agent".into(),
        }
        .into())
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
    fn agent_container_lifecycle_policy_validates_container_ids() {
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
    fn host_agent_enrollment_policy_builds_ordered_shell_exports() {
        let env = HostAgentEnrollmentPolicy::env_map(vec![
            "NATS_URL=nats://agent:pw@nats:4222".to_string(),
            "AGENT_ID=11111111-2222-3333-4444-555555555555".to_string(),
            "HMAC_SECRET=value'with-quote".to_string(),
        ]);
        let shell = HostAgentEnrollmentPolicy::shell_exports(&env);

        assert!(shell.contains("export AGENT_ID='11111111-2222-3333-4444-555555555555'"));
        assert!(shell.contains("export HMAC_SECRET='value'\"'\"'with-quote'"));
        assert!(shell.ends_with("agentforge-sidecar"));
    }

    #[test]
    fn host_agent_enrollment_policy_requires_cli_tool() {
        assert_eq!(HostAgentEnrollmentPolicy::require_cli_tool(" Codex ").unwrap(), "codex");
        assert!(HostAgentEnrollmentPolicy::require_cli_tool("unknown").is_err());
    }

    #[test]
    fn host_agent_enrollment_policy_requires_reachable_nats_url() {
        let shared_url = ["nats://backend:", "secret", "@nats:4222"].concat();

        assert_eq!(
            HostAgentEnrollmentPolicy::require_nats_base_url(None, Some(&shared_url)).unwrap(),
            "nats://nats:4222"
        );
        assert!(HostAgentEnrollmentPolicy::require_nats_base_url(None, None).is_err());
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
        // #457: Docker-backed agents are always `container` kind, pinned so the
        // sidecar publishes on the subject the callout grants from the DB row.
        assert_eq!(env.get("AGENTFORGE_RUNTIME_KIND").map(String::as_str), Some("container"));
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

    fn test_tenant_scope() -> TenantScope {
        crate::test_support::tenant_scope()
    }

    #[test]
    fn join_code_generates_prefixed_high_entropy_codes_and_stable_hash() {
        let code = JoinCode::generate();
        assert!(code.as_str().starts_with(JoinCode::PREFIX));
        assert_eq!(code.as_str().len(), JoinCode::PREFIX.len() + 43, "32 bytes base64url, no padding");
        assert_ne!(code.as_str(), JoinCode::generate().as_str(), "codes are random");

        let parsed = JoinCode::parse(code.as_str()).expect("own code parses");
        assert_eq!(parsed.hash_hex(), code.hash_hex(), "hash is deterministic");
        assert_eq!(code.hash_hex().len(), 64, "hex sha-256");
        assert!(!format!("{code:?}").contains(code.as_str()), "Debug must not leak the code");
    }

    #[test]
    fn join_code_parse_rejects_bad_shapes_with_opaque_error() {
        for bad in ["", "afj_", "afj_short", "nope_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "afj_has space"] {
            let err = JoinCode::parse(bad).expect_err("bad shape rejected");
            assert!(matches!(err.kind, ErrorKind::NotFound(_)), "opaque NotFound for {bad:?}");
        }
    }

    #[test]
    fn join_commands_follow_pairing_code_format() {
        let bash = HostAgentEnrollmentPolicy::join_command("https://forge.example.com/", "afj_abc");
        assert_eq!(
            bash,
            "curl -fsSL https://forge.example.com/api/v1/agents/local-join/script | sh -s -- --code afj_abc"
        );
        let ps = HostAgentEnrollmentPolicy::join_command_powershell("https://forge.example.com", "afj_abc");
        assert_eq!(
            ps,
            "$env:AGENTFORGE_JOIN_CODE = 'afj_abc'; irm https://forge.example.com/api/v1/agents/local-join/script.ps1 | iex"
        );
    }

    #[test]
    fn export_line_renderings_quote_for_each_shell() {
        let env = HostAgentEnrollmentPolicy::env_map(vec![
            "HMAC_SECRET=value'with-quote".to_string(),
            "AGENT_ID=agent-1".to_string(),
        ]);
        let bash = HostAgentEnrollmentPolicy::shell_export_lines(&env);
        assert!(bash.contains("export HMAC_SECRET='value'\"'\"'with-quote'"));
        assert!(!bash.contains(HostAgentEnrollmentPolicy::SIDECAR_COMMAND), "no command suffix");
        let ps = HostAgentEnrollmentPolicy::powershell_export_lines(&env);
        assert!(ps.contains("$env:HMAC_SECRET = 'value''with-quote'"));
    }

    #[test]
    fn new_agent_container_validates_inputs() {
        let scope = test_tenant_scope();
        let ok =
            NewAgent::container(&scope, CliToolKind::Codex, Some("My Agent"), None, None, Uuid::new_v4(), None, None);
        assert!(ok.is_ok());

        // Name >255 chars rejected
        let long = "x".repeat(256);
        let err = NewAgent::container(&scope, CliToolKind::Codex, Some(&long), None, None, Uuid::new_v4(), None, None);
        assert!(err.is_err());
    }

    #[test]
    fn new_agent_host_cli_carries_identity_and_kind() {
        let scope = test_tenant_scope();
        let identity = HostCliIdentity::generate();
        let expected_runtime_id = identity.runtime_id().to_string();
        let na =
            NewAgent::host_cli(&scope, CliToolKind::Codex, identity, None, None, None, Uuid::new_v4(), None).unwrap();
        assert_eq!(na.runtime_kind(), RuntimeKind::Cli);
        assert_eq!(na.runtime_id(), Some(expected_runtime_id.as_str()));
        assert_eq!(na.cli_tool(), Some("codex"));
    }

    #[test]
    fn new_agent_api_rejects_empty_model() {
        let scope = test_tenant_scope();
        assert!(NewAgent::api(&scope, "anthropic", "", None, None, Uuid::new_v4(), None).is_err());
    }

    #[test]
    fn host_cli_identity_uses_full_uuid_v7() {
        let id = HostCliIdentity::generate();
        assert!(id.runtime_id().starts_with("host-"), "got: {}", id.runtime_id());
        // Full UUID after the prefix (36 chars), not 8.
        assert_eq!(id.runtime_id().len(), "host-".len() + 36);
        // UUIDv7 has version bits set
        assert_eq!(id.agent_id().get_version_num(), 7);
        assert!(!id.hmac_secret().is_empty());
        assert!(!id.nats_connect_password().is_empty());
    }

    #[test]
    fn host_cli_identity_debug_redacts_secrets() {
        let id = HostCliIdentity::generate();
        let dbg = format!("{id:?}");
        assert!(!dbg.contains(id.hmac_secret()), "hmac_secret leaked: {dbg}");
        assert!(!dbg.contains(id.nats_connect_password()), "nats_password leaked: {dbg}");
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn agent_runtime_projects_container_row() {
        let agent = AgentAggregate::for_test(RuntimeKind::Container, Some("codex"), Some("ctr-1"));
        assert_eq!(
            agent.runtime().unwrap(),
            AgentRuntime::Container { cli_tool: CliToolKind::Codex, container_id: Some("ctr-1".to_string()) }
        );

        // A not-yet-started container row has no container_id.
        let unstarted = AgentAggregate::for_test(RuntimeKind::Container, Some("claude"), None);
        assert_eq!(
            unstarted.runtime().unwrap(),
            AgentRuntime::Container { cli_tool: CliToolKind::Claude, container_id: None }
        );
    }

    #[test]
    fn agent_runtime_projects_host_cli_row() {
        // Host CLI row: cli_tool + runtime_id set, container_id NULL.
        let agent = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, Some("host-abc"));
        assert_eq!(
            agent.runtime().unwrap(),
            AgentRuntime::HostCli { cli_tool: CliToolKind::Codex, runtime_id: "host-abc".to_string() }
        );
    }

    #[test]
    fn agent_runtime_projects_api_row() {
        // API row: cli_tool NULL, no container, no runtime_id.
        let agent = AgentAggregate::for_test(RuntimeKind::Api, None, None);
        assert_eq!(agent.runtime().unwrap(), AgentRuntime::Api);
    }

    #[test]
    fn agent_runtime_defense_in_depth_rejects_incoherent_rows() {
        // runtime_kind=Container but cli_tool missing (DB CHECK should prevent this).
        let bad_container = AgentAggregate::for_test(RuntimeKind::Container, None, Some("ctr-1"));
        assert!(bad_container.runtime().is_err());

        // runtime_kind=Cli but runtime_id missing.
        let bad_host_cli = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, None);
        assert!(bad_host_cli.runtime().is_err());

        // runtime_kind=Cli but cli_tool missing.
        let bad_host_cli_tool = AgentAggregate::for_test_full(RuntimeKind::Cli, None, None, Some("host-1"));
        assert!(bad_host_cli_tool.runtime().is_err());

        // Unparseable cli_tool slug.
        let bad_slug = AgentAggregate::for_test(RuntimeKind::Container, Some("vim"), None);
        assert!(bad_slug.runtime().is_err());
    }

    #[test]
    fn container_agent_try_from_delegates_through_sum_type() {
        // Container variant accepted via the AgentRuntime::Container branch.
        let container = AgentAggregate::for_test(RuntimeKind::Container, Some("codex"), Some("ctr-1"));
        assert!(matches!(container.runtime().unwrap(), AgentRuntime::Container { .. }));
        assert!(ContainerAgent::try_from(container).is_ok());

        // HostCli/Api are rejected with the same LifecycleRejection variants the
        // sum type maps them to.
        let host_cli = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, Some("host-1"));
        assert!(matches!(host_cli.runtime().unwrap(), AgentRuntime::HostCli { .. }));
        match ContainerAgent::try_from(host_cli) {
            Err(LifecycleRejection::HostCli) => (),
            other => panic!("expected HostCli rejection, got {other:?}"),
        }

        let api = AgentAggregate::for_test(RuntimeKind::Api, None, None);
        assert!(matches!(api.runtime().unwrap(), AgentRuntime::Api));
        match ContainerAgent::try_from(api) {
            Err(LifecycleRejection::Api) => (),
            other => panic!("expected Api rejection, got {other:?}"),
        }
    }

    #[test]
    fn container_agent_try_from_only_accepts_container_kind() {
        let container = AgentAggregate::for_test(RuntimeKind::Container, Some("codex"), None);
        assert!(ContainerAgent::try_from(container).is_ok());

        let host_cli = AgentAggregate::for_test(RuntimeKind::Cli, Some("codex"), None);
        match ContainerAgent::try_from(host_cli) {
            Err(LifecycleRejection::HostCli) => (),
            other => panic!("expected HostCli rejection, got {other:?}"),
        }

        let api = AgentAggregate::for_test(RuntimeKind::Api, None, None);
        match ContainerAgent::try_from(api) {
            Err(LifecycleRejection::Api) => (),
            other => panic!("expected Api rejection, got {other:?}"),
        }
    }

    #[test]
    fn enrolled_host_cli_try_from_delegates_through_sum_type() {
        // HostCli variant (cli_tool + runtime_id present) is accepted and the
        // typestate captures the validated runtime_id from the sum type.
        let host_cli = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, Some("host-abc"));
        assert!(matches!(host_cli.runtime().unwrap(), AgentRuntime::HostCli { .. }));
        let enrolled = EnrolledHostCli::try_from(host_cli).expect("host-cli accepted");
        assert_eq!(enrolled.runtime_id(), "host-abc");

        // Container/Api are rejected with the matching HostCliRejection variants.
        let container = AgentAggregate::for_test(RuntimeKind::Container, Some("codex"), Some("ctr-1"));
        match EnrolledHostCli::try_from(container) {
            Err(HostCliRejection::Container) => (),
            other => panic!("expected Container rejection, got {other:?}"),
        }

        let api = AgentAggregate::for_test(RuntimeKind::Api, None, None);
        match EnrolledHostCli::try_from(api) {
            Err(HostCliRejection::Api) => (),
            other => panic!("expected Api rejection, got {other:?}"),
        }
    }

    #[test]
    fn enrolled_host_cli_runtime_id_comes_from_validated_variant() {
        let host_cli = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("claude"), None, Some("host-xyz-123"));
        let enrolled = EnrolledHostCli::try_from(host_cli).expect("host-cli accepted");
        assert_eq!(enrolled.runtime_id(), "host-xyz-123");
        // inner() exposes the validated aggregate for credential lookup by id.
        assert_eq!(enrolled.inner().runtime_kind(), RuntimeKind::Cli);
    }

    #[test]
    fn enrolled_host_cli_rejects_incoherent_cli_row_as_internal() {
        // runtime_kind=Cli but runtime_id missing -> runtime() errors -> Incoherent.
        let bad = AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, None);
        assert!(bad.runtime().is_err());
        match EnrolledHostCli::try_from(bad) {
            Err(HostCliRejection::Incoherent(_)) => (),
            other => panic!("expected Incoherent rejection, got {other:?}"),
        }

        // runtime_kind=Cli but cli_tool missing -> also Incoherent.
        let bad_tool = AgentAggregate::for_test_full(RuntimeKind::Cli, None, None, Some("host-1"));
        match EnrolledHostCli::try_from(bad_tool) {
            Err(HostCliRejection::Incoherent(_)) => (),
            other => panic!("expected Incoherent rejection, got {other:?}"),
        }
    }

    #[test]
    fn host_cli_rejection_into_app_error_carries_i18n_codes() {
        let container_err = HostCliRejection::Container.into_app_error();
        assert!(
            matches!(&container_err.kind, ErrorKind::ValidationWithCode { code, .. } if *code == "errors.agent.enroll.not_host_cli_container"),
            "expected not_host_cli_container code, got: {:?}",
            container_err.kind
        );

        let api_err = HostCliRejection::Api.into_app_error();
        assert!(
            matches!(&api_err.kind, ErrorKind::ValidationWithCode { code, .. } if *code == "errors.agent.enroll.not_host_cli_api"),
            "expected not_host_cli_api code, got: {:?}",
            api_err.kind
        );

        // Incoherent preserves the underlying internal (500) error rather than
        // surfacing an operator-facing validation code.
        let incoherent =
            AgentAggregate::for_test_full(RuntimeKind::Cli, Some("codex"), None, None).runtime().unwrap_err();
        let incoherent_err = HostCliRejection::Incoherent(incoherent).into_app_error();
        assert!(
            matches!(&incoherent_err.kind, ErrorKind::Internal(_)),
            "expected Internal error, got: {:?}",
            incoherent_err.kind
        );
    }

    #[test]
    fn lifecycle_rejection_into_app_error_carries_i18n_key() {
        let err = LifecycleRejection::HostCli.into_app_error("Restart");
        let msg = format!("{err}");
        assert!(msg.contains("Host CLI"), "msg: {msg}");

        // Must carry the structured i18n code, not the old Validation(String) variant.
        assert!(
            matches!(&err.kind, ErrorKind::ValidationWithCode { code, .. } if *code == "errors.agent.lifecycle.restart_host_cli"),
            "expected ValidationWithCode with restart_host_cli code, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn lifecycle_rejection_all_actions_emit_validation_with_code() {
        let cases = [
            (LifecycleRejection::HostCli, "Restart", "errors.agent.lifecycle.restart_host_cli"),
            (LifecycleRejection::Api, "Restart", "errors.agent.lifecycle.restart_api"),
            (LifecycleRejection::HostCli, "Start", "errors.agent.lifecycle.start_host_cli"),
            (LifecycleRejection::Api, "Start", "errors.agent.lifecycle.start_api"),
            (LifecycleRejection::HostCli, "Stop", "errors.agent.lifecycle.stop_host_cli"),
            (LifecycleRejection::Api, "Stop", "errors.agent.lifecycle.stop_api"),
            (LifecycleRejection::HostCli, "Resume", "errors.agent.lifecycle.start_host_cli"),
            (LifecycleRejection::Api, "Resume", "errors.agent.lifecycle.start_api"),
        ];
        for (rejection, action, expected_code) in cases {
            let err = rejection.into_app_error(action);
            match &err.kind {
                ErrorKind::ValidationWithCode { code, .. } => {
                    assert_eq!(*code, expected_code, "action={action}");
                }
                other => panic!("expected ValidationWithCode for action={action}, got: {other:?}"),
            }
        }
    }

    #[test]
    fn agent_owner_policy_require_owner_returns_forbidden_with_code() {
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();

        // Owner passes.
        assert!(AgentOwnerPolicy::require_owner(owner, owner).is_ok());

        // Non-owner gets ForbiddenWithCode with the i18n key.
        let err = AgentOwnerPolicy::require_owner(other, owner).unwrap_err();
        match &err.kind {
            ErrorKind::ForbiddenWithCode { code, .. } => {
                assert_eq!(*code, "errors.agent.lifecycle.not_permitted");
            }
            other => panic!("expected ForbiddenWithCode, got: {other:?}"),
        }
    }

    #[test]
    fn agent_repository_policy_owns_lookup_and_collaboration_error_contracts() {
        let agent_id = AgentId::new();
        let raw_agent_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        assert!(matches!(
            AgentRepositoryPolicy::agent_not_found(agent_id).kind,
            ErrorKind::NotFound(message) if message == format!("agent {agent_id}")
        ));
        assert!(matches!(
            AgentRepositoryPolicy::agent_uuid_not_found(raw_agent_id).kind,
            ErrorKind::NotFound(message) if message == format!("agent {raw_agent_id}")
        ));
        assert!(matches!(
            AgentRepositoryPolicy::project_not_found(project_id).kind,
            ErrorKind::NotFound(message) if message == format!("project {project_id}")
        ));
        assert!(matches!(
            AgentRepositoryPolicy::workspace_not_found(workspace_id).kind,
            ErrorKind::NotFound(message) if message == format!("workspace {workspace_id}")
        ));
        assert!(matches!(
            AgentRepositoryPolicy::tenant_context_required().kind,
            ErrorKind::Validation(message) if message == "project or tenant context is required"
        ));
        assert!(matches!(
            AgentRepositoryPolicy::organization_member_not_found(org_id).kind,
            ErrorKind::NotFound(message) if message == format!("organization member for {org_id}")
        ));
        assert!(matches!(AgentRepositoryPolicy::collaborator_already_exists().kind, ErrorKind::Conflict(_)));
        assert!(matches!(
            AgentRepositoryPolicy::collaborator_not_found(agent_id, user_id).kind,
            ErrorKind::NotFound(message) if message == format!("collaborator {user_id} on agent {agent_id}")
        ));
    }
}
