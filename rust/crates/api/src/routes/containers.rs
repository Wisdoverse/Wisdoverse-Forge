//! Container control endpoints for agent lifecycle management (nested under `/api/v1`).
//!
//! - `POST /api/v1/agents/{id}/start` — Start an agent container
//! - `POST /api/v1/agents/{id}/stop`  — Stop an agent container

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult, CliToolKind, ErrorKind};
use agentforge_db::entities::Agent;
use agentforge_platform::{ContainerConfig, ContainerState, Mount};
use secrecy::{ExposeSecret, SecretString};

use crate::domain::agent::AgentContainerImagePolicy;
use crate::health::AppState;
use crate::repositories::agent::AgentRepository;
use crate::repositories::cli_credential::CliCredentialRepository;
use crate::repositories::git_credential::GitCredentialRepository;
use crate::repositories::orchestration::ParticipantRepository;
use crate::repositories::user_llm_config::UserLlmConfigRepository;
use crate::services::agent::AgentService;
use crate::services::agent_workspace::{
    CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, ensure_workspace_belongs_to_org, host_path_for_container_cwd,
    resolve_agent_workspace_paths,
};
use crate::services::cli_credential::CliCredentialService;
use crate::services::git_credential::GitCredentialService;

/// Duplicate a `SecretString` by exposing and rewrapping. We can't derive
/// `Clone` on the secret-bearing `AppConfig` (see its docstring) so the route
/// layer does the explicit copy at the tenant boundary — the wrapper keeps the
/// `Debug` redaction guarantee for everything downstream.
fn clone_secret(s: &Option<SecretString>) -> Option<SecretString> {
    s.as_ref().map(|v| SecretString::from(v.expose_secret().to_string()))
}

/// Default host directory used when `OAUTH_MOUNT_DIR` is not configured.
/// Mirrors the legacy `<dataDir>/oauth-mounts` location; chosen to stay under
/// `/tmp` so the container runtime can always mount it without extra setup.
const DEFAULT_OAUTH_MOUNT_ROOT: &str = "/tmp/agentforge/oauth-mounts";
const DEFAULT_WORKSPACE_ROOT: &str = "/data/agentforge/workspaces";

pub(crate) fn workspace_root_from_env() -> String {
    std::env::var("AGENTFORGE_WORKSPACE_ROOT").unwrap_or_else(|_| DEFAULT_WORKSPACE_ROOT.to_string())
}

/// `POST /api/agents/{id}/start` — Start an agent container.
///
/// Creates and starts a Docker container for the specified agent. Returns
/// immediately if the agent already has an associated container.
pub async fn start_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let docker = state.docker.as_ref().ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("Docker not available")))?;

    // Get agent to verify ownership and get container config.
    let service = AgentService::new(AgentRepository::new(state.pool.clone()));
    let agent = service.get(&auth.scope, AgentId::from(id)).await?;
    let repo = AgentRepository::new(state.pool.clone());

    // Reconcile stale DB container references before deciding whether to create
    // a new container. A host prune/manual removal can leave `agents.container_id`
    // set while Docker has no such container; returning `already_running` here
    // makes the UI think the agent is selectable even though no sidecar exists.
    if let Some(container_id) = &agent.container_id {
        match docker.inspect_container(container_id).await {
            Ok(info) if info.status == ContainerState::Running => {
                if let Err(err) = register_started_agent_participant(&state, &auth, &agent).await {
                    tracing::warn!(error = ?err, agent_id = %id, "agent already had a running container but participant registration failed");
                }
                return Ok(Json(json!({
                    "ok": true,
                    "container_id": container_id,
                    "status": "already_running"
                })));
            }
            Ok(info) => {
                tracing::info!(agent_id = %id, container_id = %container_id, status = ?info.status, "agent container is not running; replacing it");
                if let Err(cleanup_err) = docker.remove_container(container_id, true).await {
                    tracing::warn!(error = %cleanup_err, container_id = %container_id, "failed to remove non-running existing container");
                }
                repo.clear_container(&auth.scope, AgentId::from(id)).await?;
                mark_participant_offline(&state, &auth, AgentId::from(id)).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, agent_id = %id, container_id = %container_id, "agent container reference is stale; creating a replacement");
                repo.clear_container(&auth.scope, AgentId::from(id)).await?;
                mark_participant_offline(&state, &auth, AgentId::from(id)).await;
            }
        }
    }

    // Resolve docker image from cli_tool. Falls back to `model` only if it
    // already looks like an explicit `agentforge-agent:<tool>` image string —
    // refuses everything else so we never try to pull `claude-sonnet-4-6:latest`
    // (the bug pre-fix here used `model` directly).
    let image = AgentContainerImagePolicy::resolve(agent.cli_tool.as_deref(), agent.model.as_deref()).map_err(|err| {
        ErrorKind::Validation(format!(
            "{} — set cli_tool to one of: claude, codex, gemini, opencode (this agent has cli_tool={:?}, model={:?})",
            err.message(),
            agent.cli_tool, agent.model
        ))
    })?;

    let container_name = format!("agentforge-agent-{id}");
    let nats_base_url = pick_nats_base_url(state.config.nats_agent_url.as_deref(), state.config.nats_url.as_deref());
    // Generate the per-container HMAC secret HERE so we can inject it into
    // the container env and persist it alongside `container_id` in the same
    // `set_container` call below — if the two diverge the result consumer
    // would reject every envelope (issue #39).
    let hmac_secret = Uuid::new_v4().to_string();
    // Per-container NATS connect password (issue #38 phase 2). Embedded into
    // the sidecar's NATS_URL as the user-info password; the auth callout
    // service validates the (agent_uuid, password) pair on CONNECT and mints
    // a per-agent User JWT with subject permissions scoped to this UUID.
    // Separate from hmac_secret so a DB read of one does not grant both
    // attacker surfaces; see `AgentRepository::set_container` docstring.
    let nats_connect_password = Uuid::new_v4().to_string();
    let workspace_scope =
        WorkspaceMountScope { org_id: auth.scope.org_id().as_uuid(), workspace_id: agent.workspace_id.as_uuid() };
    ensure_workspace_belongs_to_org(&state.pool, workspace_scope.org_id, workspace_scope.workspace_id).await?;
    let workspace_paths =
        resolve_agent_workspace_paths(&workspace_root_from_env(), workspace_scope, agent.cwd.as_deref())?;
    tokio::fs::create_dir_all(&workspace_paths.host_projects_root).await.map_err(|err| {
        ErrorKind::Internal(anyhow::anyhow!(
            "failed to prepare agent workspace {}: {err}",
            workspace_paths.host_projects_root.display()
        ))
    })?;
    let container_cwd_host_path =
        host_path_for_container_cwd(&workspace_paths.host_projects_root, &workspace_paths.container_cwd)?;
    tokio::fs::create_dir_all(&container_cwd_host_path).await.map_err(|err| {
        ErrorKind::Internal(anyhow::anyhow!(
            "failed to prepare agent working directory {}: {err}",
            container_cwd_host_path.display()
        ))
    })?;
    let container_working_dir = workspace_paths.container_cwd.clone();
    let workspace_host_path = workspace_paths.host_projects_root.to_string_lossy().into_owned();
    let mut env = build_agent_env(AgentEnvInput {
        agent_id: id,
        org_id: auth.scope.org_id().as_uuid(),
        cli_tool: agent.cli_tool.as_deref(),
        cli_model: agent.model.as_deref(),
        codex_default_model: Some(state.config.codex_default_model.as_str()),
        nats_base_url,
        nats_connect_password: &nats_connect_password,
        container_server_url: state.config.container_server_url.as_deref(),
        workspace_host_path: Some(&workspace_host_path),
        hmac_secret: hmac_secret.clone(),
        context_injection_enabled: state.context_features.injection,
    });
    let mut mounts: Vec<Mount> =
        vec![Mount { source: workspace_host_path, target: CONTAINER_WORKSPACE_ROOT.to_string(), read_only: false }];

    // Inject credential-sync env vars (issue #41). These let the sidecar
    // know whether to spawn its watcher and where to watch.
    env.push(format!("CREDENTIAL_SYNC_ENABLED={}", state.config.credential_sync_enabled));
    if let Some(cli) = agent.cli_tool.as_deref()
        && let Some(dir) = creds_dir_for_cli_tool(cli)
    {
        env.push(format!("CREDS_DIR={dir}"));
    }

    // Resolve per-user credentials (tier 1–3). Best-effort: infra errors log
    // and fall through — the container still boots, just without injected
    // auth (matches the legacy TS warning-and-continue path).
    if let Some(cli_tool) = agent.cli_tool.as_deref() {
        let oauth_mount_root = state
            .config
            .oauth_mount_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OAUTH_MOUNT_ROOT));
        let creds = CliCredentialService::new(
            CliCredentialRepository::new(state.pool.clone()),
            UserLlmConfigRepository::new(state.pool.clone()),
            state.encryption_key,
            oauth_mount_root,
            clone_secret(&state.config.container_anthropic_api_key),
            clone_secret(&state.config.container_google_api_key),
            clone_secret(&state.config.container_openai_api_key),
        );
        match creds.resolve(&auth.scope, cli_tool, &container_name).await {
            Ok(injection) => {
                for (k, v) in injection.env {
                    env.push(format!("{k}={v}"));
                }
                if let Some(host_dir) = injection.oauth_mount_host_dir {
                    mounts.push(Mount {
                        source: host_dir.to_string_lossy().into_owned(),
                        target: "/run/secrets/oauth-credentials".to_string(),
                        read_only: true,
                    });
                }
            }
            Err(err) => {
                tracing::warn!(error = ?err, agent_id = %id, cli_tool, "Failed to resolve Container CLI credentials — container will boot without injected auth");
            }
        }
    }

    let git_creds = GitCredentialService::new(GitCredentialRepository::new(state.pool.clone()));
    match git_creds.resolve_cli_env(&auth.scope, state.encryption_key).await {
        Ok(injection) => {
            for (k, v) in injection.env {
                env.push(format!("{k}={v}"));
            }
        }
        Err(err) => {
            tracing::warn!(error = ?err, agent_id = %id, "Failed to resolve Git platform CLI credentials - container will boot without gh/glab token injection");
        }
    }

    let config = ContainerConfig {
        image: image.clone(),
        name: Some(container_name),
        working_dir: Some(container_working_dir),
        env,
        labels: [
            ("agentforge.agent_id".to_string(), id.to_string()),
            ("agentforge.org_id".to_string(), auth.scope.org_id().as_uuid().to_string()),
        ]
        .into_iter()
        .collect(),
        resources: Default::default(),
        network: Some("agentforge-agents".to_string()),
        mounts,
        privileged: false,
        host_pid: false,
        tty: true,
        open_stdin: true,
        attach_stdin: true,
        attach_stdout: true,
        attach_stderr: true,
    };

    let container_id = docker
        .create_container(config)
        .await
        .map_err(|e| {
            if e.is_missing_image() {
                ErrorKind::Validation(format!(
                    "agent image '{image}' is not installed on this host; run `make update-agents AGENT_TOOLS={}` or `make build-agent CLI_TOOL={}` before starting this agent",
                    agent.cli_tool.as_deref().unwrap_or("claude"),
                    agent.cli_tool.as_deref().unwrap_or("claude")
                ))
            } else {
                ErrorKind::Internal(anyhow::anyhow!("Failed to create container: {e}"))
            }
        })?;

    // Persist before starting: the sidecar connects to NATS immediately on
    // boot, and auth callout reads this DB row to validate the per-agent
    // password. Starting first leaves a race where NATS rejects the sidecar.
    if let Err(err) =
        repo.set_container(&auth.scope, AgentId::from(id), &container_id, &hmac_secret, &nats_connect_password).await
    {
        if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
            tracing::warn!(error = %cleanup_err, container_id = %container_id, "failed to clean up container after DB persist failure");
        }
        return Err(err);
    }

    if let Err(err) = docker.start_container(&container_id).await {
        if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
            tracing::warn!(error = %cleanup_err, container_id = %container_id, "failed to clean up container after start failure");
        }
        if let Err(clear_err) = repo.clear_container(&auth.scope, AgentId::from(id)).await {
            tracing::warn!(error = ?clear_err, agent_id = %id, "failed to clear container metadata after start failure");
        }
        return Err(ErrorKind::Internal(anyhow::anyhow!("Failed to start container: {err}")).into());
    }

    if let Err(err) = register_started_agent_participant(&state, &auth, &agent).await {
        tracing::warn!(error = ?err, agent_id = %id, "started agent container before participant registration completed");
    }

    Ok(Json(json!({
        "ok": true,
        "container_id": container_id,
        "status": "started"
    })))
}

/// `POST /api/agents/{id}/stop` — Stop an agent container.
///
/// Stops the Docker container associated with the specified agent. Returns
/// an error if the agent has no running container.
pub async fn stop_agent(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let docker = state.docker.as_ref().ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("Docker not available")))?;

    let service = AgentService::new(AgentRepository::new(state.pool.clone()));
    let agent = service.get(&auth.scope, AgentId::from(id)).await?;

    let container_id =
        agent.container_id.as_ref().ok_or_else(|| ErrorKind::Validation("agent has no running container".into()))?;

    docker
        .stop_container(container_id, 30)
        .await
        .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("Failed to stop container: {e}")))?;

    // Remove the host-side OAuth mount directory so the decrypted file map
    // doesn't linger on disk after the container is torn down. Best-effort:
    // on FS failure we log and continue — the row is still in the DB and
    // the next start will recreate the dir with fresh contents.
    let oauth_mount_root = state
        .config
        .oauth_mount_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OAUTH_MOUNT_ROOT));
    let cleanup_service = CliCredentialService::new(
        CliCredentialRepository::new(state.pool.clone()),
        UserLlmConfigRepository::new(state.pool.clone()),
        state.encryption_key,
        oauth_mount_root,
        clone_secret(&state.config.container_anthropic_api_key),
        clone_secret(&state.config.container_google_api_key),
        clone_secret(&state.config.container_openai_api_key),
    );
    let container_name = format!("agentforge-agent-{id}");
    if let Err(err) = cleanup_service.cleanup_oauth_mount(&container_name).await {
        tracing::warn!(error = %err, agent_id = %id, "Failed to clean up OAuth mount dir — decrypted blob may linger on disk");
    }

    docker
        .remove_container(container_id, true)
        .await
        .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("Failed to remove container after stop: {e}")))?;

    let repo = AgentRepository::new(state.pool.clone());
    repo.clear_container(&auth.scope, AgentId::from(id)).await?;
    mark_participant_offline(&state, &auth, AgentId::from(id)).await;

    // Publish `$SYS.REQ.SERVER.<name>.KICK` targeted at the agent's live
    // NATS connection so the sidecar disconnects within ~2s rather than
    // waiting for its 15-min JWT to expire naturally. Best-effort: the
    // `clear_container` call above has already removed the password, so
    // any reconnect attempt will be denied by the callout handler — the
    // KICK is a latency optimisation, not a correctness requirement.
    match state.auth_callout.as_ref() {
        Some(callout) => callout.revoke(id).await,
        None => tracing::info!(
            %id,
            "stop_agent: auth callout disabled — revocation falls back to JWT TTL (dev profile or NATS unconfigured)"
        ),
    }

    Ok(Json(json!({ "ok": true, "status": "stopped" })))
}

async fn mark_participant_offline(state: &AppState, auth: &AuthUser, agent_id: AgentId) {
    let participant_repo = ParticipantRepository::new(state.pool.clone());
    if let Err(err) = participant_repo.update_status(&auth.scope, agent_id, "offline").await {
        tracing::warn!(error = ?err, %agent_id, "failed to mark participant offline");
    }
}

async fn register_started_agent_participant(state: &AppState, auth: &AuthUser, agent: &Agent) -> AppResult<()> {
    let participant_repo = ParticipantRepository::new(state.pool.clone());
    let agent_id = agent.id;
    let fallback_name = format!("agent-{}", &agent_id.as_uuid().to_string()[..8]);
    let name = agent.name.as_deref().map(str::trim).filter(|name| !name.is_empty()).unwrap_or(fallback_name.as_str());
    let capabilities: Vec<String> = agent.cli_tool.clone().into_iter().collect();

    participant_repo.register(&auth.scope, agent_id, name, &capabilities).await?;
    participant_repo.heartbeat(&auth.scope, agent_id).await?;
    Ok(())
}

/// Build the env var list injected into a spawned agent container.
///
/// The sidecar worker bridge inside the agent image only starts when
/// `AGENTFORGE_NATS_URL` is set (entrypoint gate) AND the typed sidecar config
/// (`NATS_URL`, `HMAC_SECRET`) deserializes. Without those env vars the task
/// lands in `working` and never executes — matching the pre-worker-bridge
/// "UI lie" behaviour.
///
/// `HMAC_SECRET` is a fresh per-container UUID, generated by the caller and
/// persisted by `AgentRepository::set_container` alongside `container_id`.
/// The orchestration result consumer fetches it back via
/// `HmacSecretLookup` to verify `SignedEnvelope` signatures (issue #39).
/// An envelope signed with a stale or forged key fails verification and is
/// dropped with an `orchestration_result_unauthorized_total` counter bump.
///
/// SECURITY — the `NATS_URL` injected here comes from
/// `AppConfig::nats_agent_url` (issue #38), which carries the AGENTS-role
/// credential whose subject permissions are allowlisted in `docker/nats.conf`
/// to `events.ingest.>`, `orchestration.result.>`, `sidecar.*.heartbeat` (+
/// `_INBOX.>`) on publish and the dispatch side on subscribe. A compromised
/// container can still read this URL from `/proc/self/environ`, but the
/// captured credential no longer grants access to `broadcast.>` (per-tenant
/// WebSocket fanout) or `$SYS.>`. Phase 2 of issue #38 replaces the shared
/// `agent` role with per-agent identities minted by the auth callout
/// service on CONNECT — each sidecar's captured NATS_URL now only
/// authorises subjects scoped to that sidecar's own UUID.
/// Pick the NATS base URL (scheme + host + port, without user-info) for
/// per-agent credential interpolation (issue #38 phase 2).
///
/// Prefers `nats_agent_url` over the backend URL for backwards
/// compatibility with single-tenant dev deployments that only set one env
/// var. Whichever source wins, its user-info segment is stripped so
/// `build_agent_env` can interpolate the per-container `<agent_uuid>:
/// <nats_connect_password>` credentials the auth callout service expects.
/// Returns `None` when neither is configured (the sidecar entrypoint then
/// skips NATS wiring and the container boots with orchestration disabled).
fn pick_nats_base_url(agent_url: Option<&str>, backend_url: Option<&str>) -> Option<String> {
    let source = agent_url.or(backend_url)?;
    strip_nats_url_user_info(source)
}

/// Strip any `user:password@` user-info segment from a `nats://...` URL.
/// Returns `None` when the input cannot be parsed as `scheme://rest`.
/// `rsplit_once('@')` is used so passwords containing `:` are handled —
/// RFC 3986 places the user-info / host boundary at the last `@` before the
/// path segment. Password values in this codebase are UUIDv4 strings with
/// no `@` characters, so the rsplit is unambiguous in practice.
fn strip_nats_url_user_info(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host_part = match rest.rsplit_once('@') {
        Some((_user_info, host)) => host,
        None => rest,
    };
    Some(format!("{scheme}://{host_part}"))
}

struct AgentEnvInput<'a> {
    agent_id: Uuid,
    org_id: Uuid,
    cli_tool: Option<&'a str>,
    cli_model: Option<&'a str>,
    codex_default_model: Option<&'a str>,
    nats_base_url: Option<String>,
    nats_connect_password: &'a str,
    container_server_url: Option<&'a str>,
    workspace_host_path: Option<&'a str>,
    hmac_secret: String,
    context_injection_enabled: bool,
}

fn build_agent_env(input: AgentEnvInput<'_>) -> Vec<String> {
    let mut env = vec![
        format!("AGENT_ID={}", input.agent_id),
        format!("ORG_ID={}", input.org_id),
        format!("HMAC_SECRET={}", input.hmac_secret),
        format!("AGENTFORGE_CONTEXT_INJECTION_ENABLED={}", input.context_injection_enabled),
    ];
    if let Some(base) = input.nats_base_url.as_deref() {
        // Interpolate per-agent credentials into the URL. The auth callout
        // service (issue #38 phase 2) validates (agent_id, password) against
        // `agents.nats_connect_password` on CONNECT and mints a per-UUID
        // scoped User JWT. Shared credentials must NEVER reach the container
        // — the `strip_nats_url_user_info` preprocessing above is the guard.
        if let Some((scheme, host)) = base.split_once("://") {
            let url = format!("{scheme}://{}:{}@{host}", input.agent_id, input.nats_connect_password);
            env.push(format!("AGENTFORGE_NATS_URL={url}"));
            env.push(format!("NATS_URL={url}"));
        }
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
    if let Some(model) = cli_model_env_value(input.cli_tool, input.cli_model, input.codex_default_model) {
        env.push(format!("AGENTFORGE_CLI_MODEL={model}"));
    }
    env
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

/// CREDS_DIR per Container CLI matches the canonical paths in
/// `docker/scripts/agent-entrypoint.sh` (case `claude`, `gemini`,
/// `opencode`, `codex`). Returns `None` for unknown Container CLI tools so the
/// caller can skip CREDS_DIR injection entirely — scanning `$HOME` for
/// `.json` files on an unrecognised tool would expose non-credential state.
///
/// Paths are cross-referenced against `agent-entrypoint.sh`:
/// - claude:    `~/.claude`               (CLAUDE_CONFIG_DIR)
/// - gemini:    `~/.gemini`               (GEMINI_CONFIG_DIR)
/// - opencode:  `~/.local/share/opencode` (`opencode login` writes auth.json here)
/// - codex:     `~/.codex`                (CODEX_CONFIG_DIR)
fn creds_dir_for_cli_tool(cli_tool: &str) -> Option<&'static str> {
    match CliToolKind::parse_legacy(cli_tool).ok()? {
        CliToolKind::Claude => Some("/home/agent/.claude"),
        CliToolKind::Gemini => Some("/home/agent/.gemini"),
        CliToolKind::Opencode => Some("/home/agent/.local/share/opencode"),
        CliToolKind::Codex => Some("/home/agent/.codex"),
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentEnvInput, build_agent_env, creds_dir_for_cli_tool, pick_nats_base_url, strip_nats_url_user_info};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn strip_user_info_removes_backend_creds() {
        assert_eq!(strip_nats_url_user_info("nats://backend:pw@nats:4222").unwrap(), "nats://nats:4222");
    }

    #[test]
    fn strip_user_info_noop_when_absent() {
        assert_eq!(strip_nats_url_user_info("nats://nats:4222").unwrap(), "nats://nats:4222");
    }

    #[test]
    fn strip_user_info_rejects_non_scheme_url() {
        assert_eq!(strip_nats_url_user_info("not-a-url"), None);
    }

    #[test]
    fn pick_base_url_prefers_agent_over_backend() {
        // Issue #38: agent-URL wins when both are set. Phase 2 strips the
        // credentials from whichever we pick, so either source is safe.
        let agent = Some("nats://agent:pw1@nats:4222");
        let backend = Some("nats://backend:pw2@nats:4222");
        assert_eq!(pick_nats_base_url(agent, backend).unwrap(), "nats://nats:4222");
    }

    #[test]
    fn pick_base_url_falls_back_to_backend_url() {
        let backend = Some("nats://backend:pw@nats:4222");
        assert_eq!(pick_nats_base_url(None, backend).unwrap(), "nats://nats:4222");
    }

    #[test]
    fn pick_base_url_is_none_when_neither_set() {
        assert_eq!(pick_nats_base_url(None, None), None);
    }

    #[test]
    fn agent_env_includes_identity_and_hmac_only_without_nats() {
        let agent = Uuid::new_v4();
        let org = Uuid::new_v4();
        let nats_credential = ["pw", "-ignored"].concat();
        let hmac_value = ["secret", "-xyz"].concat();
        let env = build_agent_env(AgentEnvInput {
            agent_id: agent,
            org_id: org,
            cli_tool: Some("claude"),
            cli_model: None,
            codex_default_model: None,
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: hmac_value.clone(),
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
    fn agent_env_injects_per_agent_nats_url() {
        let agent = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let nats_credential = ["pw", "-abc", "-123"].concat();
        let env = build_agent_env(AgentEnvInput {
            agent_id: agent,
            org_id: Uuid::new_v4(),
            cli_tool: Some("codex"),
            cli_model: Some("gpt-5.4-mini"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: Some("nats://nats:4222".to_string()),
            nats_connect_password: &nats_credential,
            container_server_url: Some("http://agentforge:4003"),
            workspace_host_path: Some("/data/agentforge/workspaces/orgs/o/workspaces/w/projects"),
            hmac_secret: "hmac".into(),
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
    fn agent_env_nats_url_never_leaks_shared_credentials() {
        // Regression: even when the operator supplies `nats_url` with an
        // embedded `backend:<pw>@`, the per-agent env must strip it and
        // interpolate the per-container creds instead. A compromised sidecar
        // must never read another tenant's creds out of /proc/self/environ.
        let shared_url = ["nats://backend:", "super-secret", "@nats:4222"].concat();
        let base = pick_nats_base_url(None, Some(&shared_url));
        let nats_credential = ["per-agent", "-pw"].concat();
        let env = build_agent_env(AgentEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some("claude"),
            cli_model: None,
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: base,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac".into(),
            context_injection_enabled: false,
        });
        for entry in &env {
            assert!(!entry.contains("backend:super-secret"), "leaked shared backend creds in env entry {entry}");
            assert!(!entry.contains("super-secret@"), "leaked shared backend creds in env entry {entry}");
        }
    }

    #[test]
    fn agent_env_skips_cli_tool_when_unknown() {
        let nats_credential = "pw".to_string();
        let env = build_agent_env(AgentEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: None,
            cli_model: Some("gpt-5.4-mini"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac".into(),
            context_injection_enabled: false,
        });
        assert!(!env.iter().any(|v| v.starts_with("CLI_TOOL=")));
        assert!(!env.iter().any(|v| v.starts_with("AGENTFORGE_CLI_MODEL=")));
    }

    #[test]
    fn agent_env_uses_codex_default_for_legacy_image_model_values() {
        let nats_credential = "pw".to_string();
        let env = build_agent_env(AgentEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some("codex"),
            cli_model: Some("agentforge-agent:codex"),
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac".into(),
            context_injection_enabled: false,
        });
        assert!(env.contains(&"AGENTFORGE_CLI_MODEL=gpt-5.5".to_string()));
    }

    #[test]
    fn agent_env_canonicalizes_legacy_cli_tool_values() {
        let nats_credential = "pw".to_string();
        let env = build_agent_env(AgentEnvInput {
            agent_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            cli_tool: Some(" CODEX "),
            cli_model: None,
            codex_default_model: Some("gpt-5.5"),
            nats_base_url: None,
            nats_connect_password: &nats_credential,
            container_server_url: None,
            workspace_host_path: None,
            hmac_secret: "hmac".into(),
            context_injection_enabled: false,
        });
        assert!(env.contains(&"CLI_TOOL=codex".to_string()));
        assert!(env.contains(&"AGENTFORGE_CLI_MODEL=gpt-5.5".to_string()));
    }

    #[test]
    fn start_response_format() {
        let response = json!({
            "ok": true,
            "container_id": "abc123",
            "status": "started"
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["container_id"], "abc123");
        assert_eq!(response["status"], "started");
    }

    #[test]
    fn stop_response_format() {
        let response = json!({ "ok": true, "status": "stopped" });
        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "stopped");
    }

    #[test]
    fn already_running_response_format() {
        let response = json!({
            "ok": true,
            "container_id": "existing-id",
            "status": "already_running"
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["status"], "already_running");
    }

    #[test]
    fn creds_dir_matches_entrypoint_paths() {
        // Source of truth: docker/scripts/agent-entrypoint.sh lines 20-50.
        // Update this table whenever the entrypoint changes a credential path.
        assert_eq!(creds_dir_for_cli_tool("claude"), Some("/home/agent/.claude"));
        assert_eq!(creds_dir_for_cli_tool("gemini"), Some("/home/agent/.gemini"));
        assert_eq!(creds_dir_for_cli_tool("opencode"), Some("/home/agent/.local/share/opencode"));
        assert_eq!(creds_dir_for_cli_tool("codex"), Some("/home/agent/.codex"));
        assert_eq!(creds_dir_for_cli_tool("unknown"), None);
    }
}
