//! Docker-backed runtime adapter for internal MCP-managed agents.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use bollard::errors::Error as BollardError;
use bollard::query_parameters::{
    AttachContainerOptions, InspectContainerOptions, LogsOptions, RemoveContainerOptions, StartContainerOptions,
};
use chrono::Utc;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use uuid::Uuid;

use agentforge_core::{AgentStatus, AppError, AppResult, CliToolKind};
use agentforge_platform::{
    ContainerConfig as PlatformContainerConfig, DockerClient, Mount as PlatformMount, ResourceLimits,
};

use crate::domain::mcp::{
    CompletionObservation, DockerCreateRequest, DockerMcpRuntimeOptions, DockerRuntimeSession, DockerSessionState,
    cli_ready_timeout_error, docker_create_plan, docker_runtime_error, has_any_indicator, hash_bytes, infer_cli_tool,
    io_runtime_error, is_not_found_error, missing_container_id_error, runtime_markers, stale_working_status,
};
use crate::services::container_image_config::capture_container_image_identity;
use crate::services::mcp_agent::{
    McpAgentRecord, McpAgentRuntime, McpAgentRuntimeCreate, McpAgentRuntimeCreateResult, McpAgentStore, SessionStatus,
};

const READY_LOG_TAIL: usize = 30;
const COMPLETION_LOG_TAIL: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedMcpImage {
    immutable_id: String,
    evidence: serde_json::Value,
}

pub(crate) fn docker_mcp_agent_runtime<S>(store: S, docker: Arc<DockerClient>) -> impl McpAgentRuntime + Clone
where
    S: McpAgentStore + Clone + Send + Sync + 'static,
{
    let options = DockerMcpRuntimeOptions::default();
    DockerMcpAgentRuntime::new(store, LiveDockerMcpRuntimeBackend::new(docker, options.prompt_chunk_delay), options)
}

#[async_trait]
trait DockerMcpRuntimeBackend: Send + Sync {
    async fn resolve_image(&self, image_ref: &str, cli_tool: &str) -> AppResult<ResolvedMcpImage>;
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
    prompt_chunk_delay: std::time::Duration,
}

impl LiveDockerMcpRuntimeBackend {
    fn new(docker: Arc<DockerClient>, prompt_chunk_delay: std::time::Duration) -> Self {
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

/// Translate an MCP `DockerCreateRequest` into the platform `ContainerConfig`
/// so MCP-managed agent containers go through the single validated, hardened
/// creation path (`DockerClient::create_container`) instead of a raw bollard
/// spec that skipped the security policy and resource limits (F031/F037).
///
/// Resource limits default to the bounded platform defaults; `privileged` and
/// `host_pid` are forced off and re-asserted by the platform layer.
fn mcp_container_config(request: DockerCreateRequest) -> PlatformContainerConfig {
    PlatformContainerConfig {
        image: request.image,
        name: Some(request.name),
        working_dir: Some(request.working_dir),
        env: request.env.into_iter().map(|(key, value)| format!("{key}={value}")).collect(),
        labels: request.labels,
        resources: ResourceLimits::default(),
        network: None,
        mounts: request
            .mounts
            .into_iter()
            .map(|mount| PlatformMount { source: mount.source, target: mount.target, read_only: mount.read_only })
            .collect(),
        privileged: false,
        host_pid: false,
        tty: request.tty,
        open_stdin: request.open_stdin,
        attach_stdin: request.attach_stdin,
        attach_stdout: request.attach_stdout,
        attach_stderr: request.attach_stderr,
    }
}

#[async_trait]
impl DockerMcpRuntimeBackend for LiveDockerMcpRuntimeBackend {
    async fn resolve_image(&self, image_ref: &str, cli_tool: &str) -> AppResult<ResolvedMcpImage> {
        let tool = CliToolKind::parse_legacy(cli_tool)
            .map_err(|err| docker_runtime_error(format!("unsupported Container CLI tool {cli_tool:?}: {err}")))?;
        let identity = self
            .docker
            .local_image_identity(image_ref)
            .await
            .map_err(|err| docker_runtime_error(err.to_string()))?
            .ok_or_else(|| docker_runtime_error(format!("container image {image_ref} is not available")))?;
        let evidence = capture_container_image_identity(tool, image_ref, &identity).await?;
        let evidence = serde_json::to_value(evidence)
            .map_err(|err| docker_runtime_error(format!("could not serialize container image identity: {err}")))?;
        Ok(ResolvedMcpImage { immutable_id: identity.id, evidence })
    }

    async fn create_container(&self, request: DockerCreateRequest) -> AppResult<String> {
        // Route through the single hardened chokepoint: DockerClient::create_container
        // runs validate_security and applies resource limits + cap_drop ALL +
        // no-new-privileges + privileged=false / pid_mode=None. Previously this
        // path built a raw bollard spec that skipped the policy entirely (F031/F037).
        let expected_image_id = request.image.clone();
        let container_id = self
            .docker
            .create_container(mcp_container_config(request))
            .await
            .map_err(|err| docker_runtime_error(err.to_string()))?;
        let verified =
            self.docker.inspect_container(&container_id).await.is_ok_and(|info| info.image_id == expected_image_id);
        if !verified {
            let _ = self.docker.remove_container(&container_id, true).await;
            return Err(docker_runtime_error("created MCP Agent container image identity did not match".to_string()));
        }
        Ok(container_id)
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

impl<S, D> DockerMcpAgentRuntime<S, D>
where
    S: McpAgentStore + Clone + Send + Sync + 'static,
    D: DockerMcpRuntimeBackend + Clone + Send + Sync + 'static,
{
    fn spawn_completion_monitor(
        &self,
        agent_id: Uuid,
        expected_container_id: String,
        expected_lease: chrono::DateTime<Utc>,
    ) {
        let runtime = (*self).clone();
        tokio::spawn(async move {
            runtime.monitor_prompt(agent_id, expected_container_id, expected_lease).await;
        });
    }

    async fn monitor_prompt(
        &self,
        agent_id: Uuid,
        expected_container_id: String,
        mut expected_lease: chrono::DateTime<Utc>,
    ) {
        loop {
            sleep(self.options.completion_poll_interval).await;
            let record = match self.store.get_agent(agent_id).await {
                Ok(record) if record.container_id.as_deref() == Some(expected_container_id.as_str()) => record,
                Ok(_) => break,
                Err(err) => {
                    tracing::warn!(error = ?err, %agent_id, "MCP completion monitor could not read Agent; stopping renewal");
                    break;
                }
            };
            let state = match self.docker.inspect_state(&expected_container_id).await {
                Ok(state) => state,
                Err(err) => {
                    // A transient observation failure must not renew forever.
                    // Retry while the existing 60s lease remains; the CAS below
                    // refuses to revive it after expiry.
                    tracing::warn!(error = ?err, %agent_id, "MCP completion monitor observation failed");
                    continue;
                }
            };
            if !matches!(state, DockerSessionState::Running | DockerSessionState::Created) {
                let _ = self
                    .store
                    .finish_agent_work(agent_id, &expected_container_id, expected_lease, AgentStatus::Offline)
                    .await;
                self.clear_tracking(agent_id);
                break;
            }

            let cli_tool = infer_cli_tool(
                record.model.as_deref(),
                self.session_meta(agent_id).as_ref().map(|session| session.cli_tool.as_str()),
            );
            match self.prompt_completed(agent_id, &expected_container_id, &cli_tool, record.updated_at).await {
                Ok(true) => {
                    let _ = self
                        .store
                        .finish_agent_work(agent_id, &expected_container_id, expected_lease, AgentStatus::Idle)
                        .await;
                    self.observations.lock().expect("observation lock").remove(&agent_id);
                    break;
                }
                Ok(false) => {
                    match self.store.renew_agent_work_lease(agent_id, &expected_container_id, expected_lease).await {
                        Ok(Some(renewed_lease)) => expected_lease = renewed_lease,
                        Ok(None) => {
                            // Process pause/restart or another authoritative owner
                            // change expired/replaced this lease. Never revive it.
                            self.observations.lock().expect("observation lock").remove(&agent_id);
                            break;
                        }
                        Err(err) => {
                            tracing::warn!(error = ?err, %agent_id, "MCP completion monitor lease renewal failed");
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(error = ?err, %agent_id, "MCP completion monitor could not determine completion");
                }
            }
        }
    }
}

#[async_trait]
impl<S, D> McpAgentRuntime for DockerMcpAgentRuntime<S, D>
where
    S: McpAgentStore + Clone + Send + Sync + 'static,
    D: DockerMcpRuntimeBackend + Clone + Send + Sync + 'static,
{
    async fn create_agent(&self, req: McpAgentRuntimeCreate) -> AppResult<McpAgentRuntimeCreateResult> {
        let source_image = req.image.clone();
        let mut plan = docker_create_plan(req.agent_id, req.org_id, req.project_id, req.image, req.cwd, req.env);
        let resolved = self.docker.resolve_image(&source_image, &plan.cli_tool).await?;
        plan.request.image = resolved.immutable_id;
        if let Some(source) = resolved.evidence.get("source").and_then(serde_json::Value::as_str) {
            plan.request.labels.insert("agentforge.image.source".to_string(), source.to_string());
        }
        if let Some(image_id) = resolved.evidence.get("imageId").and_then(serde_json::Value::as_str) {
            plan.request.labels.insert("agentforge.image.id".to_string(), image_id.to_string());
        }

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

        Ok(McpAgentRuntimeCreateResult { container_id, image_identity: resolved.evidence })
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        let record = self.store.get_agent(agent_id).await?;
        let container_id = require_container_id(&record)?;
        let cli_tool = infer_cli_tool(
            record.model.as_deref(),
            self.session_meta(agent_id).as_ref().map(|session| session.cli_tool.as_str()),
        );

        self.wait_for_cli_ready(&container_id, &cli_tool).await?;
        let owner_lease = self.store.begin_agent_work(agent_id, &container_id).await?;
        if let Err(err) = self
            .docker
            .write_stdin_sequence(&container_id, vec![b"\x15".to_vec(), prompt.as_bytes().to_vec(), b"\r".to_vec()])
            .await
        {
            if let Err(reset_err) =
                self.store.finish_agent_work(agent_id, &container_id, owner_lease, AgentStatus::Idle).await
            {
                tracing::warn!(error = ?reset_err, %agent_id, "failed to restore MCP Agent status after prompt write failure");
            }
            return Err(err);
        }

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
        self.spawn_completion_monitor(agent_id, require_container_id(&record)?, owner_lease);
        Ok(())
    }

    async fn destroy_agent(&self, agent_id: Uuid, expected_container_id: Option<&str>) -> AppResult<()> {
        let container_id = expected_container_id
            .map(str::to_owned)
            .or_else(|| self.session_meta(agent_id).map(|session| session.container_id))
            .ok_or_else(|| missing_container_id_error(agent_id))?;
        self.clear_tracking(agent_id);
        match self.docker.remove_container(&container_id, true).await {
            Ok(()) => Ok(()),
            Err(err) if is_not_found_error(&err) => Ok(()),
            Err(err) => Err(err),
        }
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
            return Ok(SessionStatus { agent_id, status: AgentStatus::Offline.to_string() });
        }
        if self.observations.lock().expect("observation lock").contains_key(&agent_id) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use agentforge_core::ErrorKind;
    use async_trait::async_trait;
    use chrono::{Duration as ChronoDuration, Utc};

    use crate::domain::mcp::DockerMount;
    use crate::services::mcp_agent::ProjectRuntimeContext;

    type StdinWriteRecord = (String, Vec<Vec<u8>>);

    #[test]
    fn mcp_container_config_routes_through_hardened_policy() {
        // F031/F037: the MCP create path produces a bounded, non-privileged config
        // that passes the platform security policy (the single chokepoint).
        let request = DockerCreateRequest {
            image: "agentforge-agent-codex:latest".to_string(),
            name: "agentforge-agent-x".to_string(),
            working_dir: "/workspace".to_string(),
            env: HashMap::from([("AGENTFORGE_AGENT_ID".to_string(), "abc".to_string())]),
            labels: HashMap::new(),
            mounts: vec![DockerMount {
                source: "/data/agentforge/workspaces/x".to_string(),
                target: "/workspace".to_string(),
                read_only: false,
            }],
            tty: true,
            open_stdin: true,
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
        };
        let config = mcp_container_config(request);
        assert!(!config.privileged, "MCP containers must never be privileged");
        assert!(!config.host_pid, "MCP containers must never share host PID");
        assert!(config.resources.memory_bytes.is_some(), "memory must be bounded");
        assert!(config.resources.pids_limit.is_some(), "pids must be bounded");
        assert_eq!(config.env, vec!["AGENTFORGE_AGENT_ID=abc".to_string()]);
        assert!(agentforge_platform::validate_security(&config).is_ok(), "must pass the platform security policy");
    }

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
        async fn resolve_image(&self, image_ref: &str, _cli_tool: &str) -> AppResult<ResolvedMcpImage> {
            Ok(ResolvedMcpImage {
                immutable_id: "sha256:test-image".to_string(),
                evidence: serde_json::json!({
                    "source": image_ref,
                    "imageId": "sha256:test-image",
                    "versionSource": "not-reported",
                    "trust": "host-local"
                }),
            })
        }

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
        leases: Arc<Mutex<HashMap<Uuid, chrono::DateTime<Utc>>>>,
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

        async fn begin_agent_work(
            &self,
            agent_id: Uuid,
            expected_container_id: &str,
        ) -> AppResult<chrono::DateTime<Utc>> {
            let mut agents = self.agents.lock().expect("agents lock");
            let agent = agents.get_mut(&agent_id).ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")))?;
            if agent.container_id.as_deref() != Some(expected_container_id) {
                return Err(ErrorKind::Conflict("agent container changed".into()).into());
            }
            agent.status = AgentStatus::Working;
            agent.updated_at = Some(Utc::now());
            let lease = Utc::now() + ChronoDuration::seconds(60);
            self.leases.lock().expect("leases lock").insert(agent_id, lease);
            Ok(lease)
        }

        async fn renew_agent_work_lease(
            &self,
            agent_id: Uuid,
            expected_container_id: &str,
            expected_lease: chrono::DateTime<Utc>,
        ) -> AppResult<Option<chrono::DateTime<Utc>>> {
            let current_container =
                self.agents.lock().expect("agents lock").get(&agent_id).and_then(|agent| agent.container_id.clone());
            let mut leases = self.leases.lock().expect("leases lock");
            if current_container.as_deref() != Some(expected_container_id)
                || leases.get(&agent_id) != Some(&expected_lease)
            {
                return Ok(None);
            }
            let renewed = Utc::now() + ChronoDuration::seconds(60);
            leases.insert(agent_id, renewed);
            Ok(Some(renewed))
        }

        async fn finish_agent_work(
            &self,
            agent_id: Uuid,
            expected_container_id: &str,
            expected_lease: chrono::DateTime<Utc>,
            status: AgentStatus,
        ) -> AppResult<bool> {
            let mut agents = self.agents.lock().expect("agents lock");
            let Some(agent) = agents.get_mut(&agent_id) else { return Ok(false) };
            if agent.container_id.as_deref() != Some(expected_container_id) {
                return Ok(false);
            }
            let mut leases = self.leases.lock().expect("leases lock");
            if leases.get(&agent_id) != Some(&expected_lease) {
                return Ok(false);
            }
            leases.remove(&agent_id);
            agent.status = status;
            agent.updated_at = Some(Utc::now());
            Ok(true)
        }

        async fn delete_agent(&self, agent_id: Uuid, _expected_container_id: Option<&str>) -> AppResult<()> {
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
        assert_eq!(creates[0].image, "sha256:test-image");
        assert_eq!(creates[0].labels.get("agentforge.image.id").map(String::as_str), Some("sha256:test-image"));
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
            container_image_identity: None,
            cli_tool: Some("codex".to_string()),
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
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if store.get_agent(agent_id).await.unwrap().status == AgentStatus::Idle {
                    break;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("server-owned completion monitor must finish without client polling");
        let idle = runtime.session_status(agent_id).await.expect("idle status");
        runtime.destroy_agent(agent_id, Some("ctr-test")).await.expect("destroy");

        assert_eq!(working.status, "working");
        assert_eq!(idle.status, "idle");
        assert_eq!(
            docker.take_stdin_writes(),
            vec![("ctr-test".to_string(), vec![b"\x15".to_vec(), b"ship it".to_vec(), b"\r".to_vec()],)]
        );
        assert_eq!(docker.take_removes(), vec![("ctr-test".to_string(), true)]);
    }

    #[tokio::test]
    async fn destroy_cleans_created_container_before_store_insert() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let docker = TestDockerBackend::default();
        let runtime = runtime(TestStore::default(), docker.clone());

        runtime
            .create_agent(McpAgentRuntimeCreate {
                agent_id,
                org_id,
                user_id: Uuid::now_v7(),
                project_id: None,
                name: "Unpersisted worker".to_string(),
                image: "agentforge-agent-codex:latest".to_string(),
                cwd: format!("/data/agentforge/workspaces/orgs/{org_id}/workspaces/{org_id}/projects"),
                env: HashMap::from([("AGENTFORGE_CLI_TOOL".to_string(), "codex".to_string())]),
            })
            .await
            .expect("create agent");

        runtime.destroy_agent(agent_id, None).await.expect("destroy unpersisted agent");

        assert_eq!(docker.take_removes(), vec![("ctr-test".to_string(), true)]);
    }
}
