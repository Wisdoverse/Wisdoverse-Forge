//! Container lifecycle management — create, start, stop, remove, inspect.

use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};

use crate::docker::DockerClient;
use crate::security;
use crate::types::{ContainerConfig, ContainerInfo, ContainerState};

/// Errors that can occur during platform operations.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Docker error: {0}")]
    Docker(#[from] bollard::errors::Error),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Container not found: {0}")]
    NotFound(String),

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Invalid stop timeout {0}s: must fit in i32 (Docker engine API range)")]
    InvalidTimeout(i64),

    #[error("Internal error: {0}")]
    Internal(String),

    /// Pulling an image from its registry failed (network, auth, or the
    /// registry rejected the request). Carries the daemon's message.
    #[error("Image pull failed: {0}")]
    Pull(String),

    /// A registry/distribution inspect (remote digest lookup, no pull) failed.
    #[error("Registry inspect failed: {0}")]
    Registry(String),

    /// A daemon-side `docker build` failed (transport error or an in-stream
    /// `errorDetail` frame). Carries the daemon's message.
    #[error("Image build failed: {0}")]
    Build(String),

    /// A registry image was not accepted by the pinned Sigstore signer policy.
    #[error("Image signature verification failed: {0}")]
    ImageVerification(String),
}

impl PlatformError {
    pub fn image_verification_code(&self) -> Option<&str> {
        match self {
            Self::ImageVerification(code) => Some(code),
            _ => None,
        }
    }

    /// True when Docker reported that the referenced container no longer exists.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
            || matches!(self, Self::Docker(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }))
    }

    /// True when Docker rejected container creation because the requested image
    /// is not installed on this host.
    pub fn is_missing_image(&self) -> bool {
        matches!(
            self,
            Self::Docker(bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                message,
                ..
            }) if message.contains("No such image")
        )
    }

    /// True when Docker rejected an operation with 409 Conflict — e.g. removing
    /// an image that is still referenced by another tag or a child image. The
    /// prune path treats this as "leave it" rather than an error.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Docker(bollard::errors::Error::DockerResponseServerError { status_code: 409, .. }))
    }
}

/// Build the hardened `HostConfig` for an agent container.
///
/// Defense-in-depth that must stay at container creation (F031/F032/F037):
/// resource limits, never privileged, never host PID, drop ALL Linux
/// capabilities, and forbid setuid privilege gain (`no-new-privileges`). The
/// callers run untrusted/LLM-driven Container CLI processes, so the in-container
/// attack surface and any container-escape blast radius are minimized. Mirrors
/// `clone_runtime`'s posture.
pub(crate) fn agent_host_config(config: &ContainerConfig) -> HostConfig {
    // Translate bind mounts into Docker's legacy `HostConfig.Binds` format
    // (`/host:/container[:ro]`). `security::validate_security` already rejects
    // `/var/run/docker.sock` and other dangerous paths.
    let binds: Vec<String> = config
        .mounts
        .iter()
        .map(|m| {
            let suffix = if m.read_only { ":ro" } else { "" };
            format!("{}:{}{}", m.source, m.target, suffix)
        })
        .collect();

    HostConfig {
        memory: config.resources.memory_bytes,
        memory_swap: config.resources.memory_swap_bytes,
        cpu_quota: config.resources.cpu_quota,
        pids_limit: config.resources.pids_limit,
        binds: if binds.is_empty() { None } else { Some(binds) },
        // Defense-in-depth: always override to false regardless of config.
        privileged: Some(false),
        // Defense-in-depth: never allow host PID namespace regardless of config.
        pid_mode: None,
        // Drop every Linux capability — the Container CLIs (node/python/git
        // userland) need none — and forbid setuid privilege gain.
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges".to_string()]),
        network_mode: config.network.clone(),
        ..Default::default()
    }
}

impl DockerClient {
    /// Create a container after validating the security policy.
    ///
    /// Returns the container ID on success.
    pub async fn create_container(&self, config: ContainerConfig) -> Result<String, PlatformError> {
        // Validate security policy first — reject before touching Docker.
        security::validate_security(&config).map_err(|violations| {
            PlatformError::SecurityViolation(
                violations.into_iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "),
            )
        })?;

        let host_config = agent_host_config(&config);

        let create_config = ContainerCreateBody {
            image: Some(config.image.clone()),
            working_dir: config.working_dir.clone(),
            env: Some(config.env.clone()),
            labels: Some(config.labels.clone()),
            tty: Some(config.tty),
            open_stdin: Some(config.open_stdin),
            attach_stdin: Some(config.attach_stdin),
            attach_stdout: Some(config.attach_stdout),
            attach_stderr: Some(config.attach_stderr),
            host_config: Some(host_config),
            ..Default::default()
        };

        // bollard 0.21 makes `platform` a plain `String` (was `Option<&str>`).
        // The Docker Engine API treats an empty `?platform=` query parameter as
        // unspecified — same semantics as the previous `None`. We keep it
        // unset until/unless `ContainerConfig` carries an explicit platform.
        let options =
            config.name.as_ref().map(|n| CreateContainerOptions { name: Some(n.clone()), platform: String::new() });

        let response = self.inner().create_container(options, create_config).await.map_err(PlatformError::Docker)?;

        tracing::info!(
            container_id = %response.id,
            image = %config.image,
            "Container created"
        );
        Ok(response.id)
    }

    /// Verify actual security settings without preventing inspection or cleanup.
    pub async fn validate_container_security(&self, id: &str) -> Result<(), PlatformError> {
        let info =
            self.inner().inspect_container(id, None::<InspectContainerOptions>).await.map_err(PlatformError::Docker)?;
        security::validate_runtime_security(&info).map_err(|_| {
            // Do not return inspect fields, mount paths, or daemon configuration.
            PlatformError::SecurityViolation("container does not meet the runtime security requirements".to_string())
        })
    }

    /// Start a previously created container after verifying its actual settings.
    pub async fn start_container(&self, id: &str) -> Result<(), PlatformError> {
        self.validate_container_security(id).await?;
        self.inner().start_container(id, None::<StartContainerOptions>).await.map_err(PlatformError::Docker)?;

        tracing::info!(container_id = %id, "Container started");
        Ok(())
    }

    /// Stop a running container with a timeout in seconds.
    ///
    /// Returns `PlatformError::InvalidTimeout` if `timeout_secs` does not fit
    /// in `i32`. The Docker Engine API encodes the stop timeout as a signed
    /// 32-bit integer, so an `as i32` truncation cast would silently turn a
    /// large grace period into a negative value and trigger an immediate
    /// SIGKILL — exactly the opposite of a graceful shutdown.
    pub async fn stop_container(&self, id: &str, timeout_secs: i64) -> Result<(), PlatformError> {
        let timeout_i32 = i32::try_from(timeout_secs).map_err(|_| PlatformError::InvalidTimeout(timeout_secs))?;
        self.inner()
            .stop_container(id, Some(StopContainerOptions { t: Some(timeout_i32), signal: None }))
            .await
            .map_err(PlatformError::Docker)?;

        tracing::info!(container_id = %id, "Container stopped");
        Ok(())
    }

    /// Remove a container, optionally forcing removal of running containers.
    pub async fn remove_container(&self, id: &str, force: bool) -> Result<(), PlatformError> {
        self.inner()
            .remove_container(id, Some(RemoveContainerOptions { force, ..Default::default() }))
            .await
            .map_err(PlatformError::Docker)?;

        tracing::info!(container_id = %id, "Container removed");
        Ok(())
    }

    /// Inspect a container and return structured info.
    pub async fn inspect_container(&self, id: &str) -> Result<ContainerInfo, PlatformError> {
        let info =
            self.inner().inspect_container(id, None::<InspectContainerOptions>).await.map_err(PlatformError::Docker)?;

        let state = match info.state.and_then(|s| s.status).map(|s| s.to_string()).as_deref() {
            Some("running") => ContainerState::Running,
            Some("created") => ContainerState::Created,
            Some("paused") => ContainerState::Paused,
            Some("exited") | Some("stopped") => ContainerState::Stopped,
            Some("dead") => ContainerState::Dead,
            _ => ContainerState::Unknown,
        };

        Ok(ContainerInfo {
            id: info.id.unwrap_or_default(),
            name: info.name.unwrap_or_default(),
            image_id: info.image.unwrap_or_default(),
            image: info.config.and_then(|c| c.image).unwrap_or_default(),
            status: state,
            created_at: info.created,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_requires_verified_runtime_security_before_sending_start() {
        use bollard::{API_DEFAULT_VERSION, Docker};
        use serde_json::json;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::sync::oneshot;

        let valid = json!({"HostConfig": {
            "Privileged": false, "Memory": 67108864, "PidsLimit": 16, "NetworkMode": "bridge",
            "CapDrop": ["ALL"], "SecurityOpt": ["no-new-privileges"]
        }, "Mounts": [], "Config": {"Env": ["PRIVATE_TEST_VALUE=sentinel"]}});
        let mut unsafe_config = valid.clone();
        unsafe_config["HostConfig"]["CapAdd"] = json!(["SYS_ADMIN"]);
        for (status, body, allowed) in [
            (200, valid.to_string(), true),
            (200, unsafe_config.to_string(), false),
            (200, json!({}).to_string(), false),
            (503, json!({"message": "private daemon detail"}).to_string(), false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (shutdown, mut stopped) = oneshot::channel();
            let server = tokio::spawn(async move {
                let mut requests = Vec::new();
                loop {
                    let (mut stream, _) = tokio::select! {
                        _ = &mut stopped => break,
                        accepted = listener.accept() => accepted.unwrap(),
                    };
                    let mut header = Vec::new();
                    while !header.windows(4).any(|part| part == b"\r\n\r\n") {
                        let mut buffer = [0; 4096];
                        let read = stream.read(&mut buffer).await.unwrap();
                        assert!(read > 0 && header.len() < 16384);
                        header.extend_from_slice(&buffer[..read]);
                    }
                    let request = String::from_utf8(header).unwrap().lines().next().unwrap().to_owned();
                    let response = if request.contains("/json") {
                        format!(
                            "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        assert!(request.contains("/start"));
                        "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_owned()
                    };
                    requests.push(request);
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                requests
            });
            let docker = DockerClient::from_bollard(
                Docker::connect_with_http(&format!("http://{address}"), 5, API_DEFAULT_VERSION).unwrap(),
            );
            let result = docker.start_container("runtime-fixture").await;
            shutdown.send(()).unwrap();
            let requests = server.await.unwrap();
            assert_eq!(result.is_ok(), allowed);
            assert_eq!(requests.len(), if allowed { 2 } else { 1 });
            assert!(requests[0].contains("/containers/runtime-fixture/json"));
            if let Err(PlatformError::SecurityViolation(message)) = result {
                assert!(!message.contains("PRIVATE_TEST_VALUE"));
                assert!(!message.contains("sentinel"));
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires a local Docker daemon and FORGE_RUNTIME_ADMISSION_TEST_IMAGE"]
    async fn runtime_security_checks_actual_docker_settings() {
        use bollard::{Docker, models::ContainerInspectResponse};
        let image = std::env::var("FORGE_RUNTIME_ADMISSION_TEST_IMAGE").expect("set an approved local system image");
        let docker = DockerClient::from_bollard(Docker::connect_with_local_defaults().unwrap());
        for hardened in [true, false] {
            let host = HostConfig {
                privileged: Some(false),
                memory: Some(64 * 1024 * 1024),
                pids_limit: Some(16),
                cap_drop: Some(vec!["ALL".to_owned()]),
                security_opt: hardened.then(|| vec!["no-new-privileges".to_owned()]),
                network_mode: Some("none".to_owned()),
                ..Default::default()
            };
            let created = docker
                .inner()
                .create_container(
                    None::<CreateContainerOptions>,
                    ContainerCreateBody {
                        image: Some(image.clone()),
                        user: Some("1004:1003".to_owned()),
                        entrypoint: Some(vec!["/bin/sh".to_owned()]),
                        cmd: Some(vec!["-c".to_owned(), "true".to_owned()]),
                        labels: Some(std::collections::HashMap::from([(
                            "forge.test".to_owned(),
                            "runtime-security-admission".to_owned(),
                        )])),
                        host_config: Some(host),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            let result = docker.validate_container_security(&created.id).await;
            // Unsafe fixtures remain created and are never started.
            let start = if hardened && result.is_ok() { Some(docker.start_container(&created.id).await) } else { None };
            let inspected: Result<ContainerInspectResponse, _> =
                docker.inner().inspect_container(&created.id, None::<InspectContainerOptions>).await;
            let removed = docker
                .inner()
                .remove_container(
                    &created.id,
                    Some(RemoveContainerOptions { force: true, v: true, ..Default::default() }),
                )
                .await;
            assert!(removed.is_ok(), "owned fixture cleanup failed");
            assert_eq!(result.is_ok(), hardened);
            if hardened {
                assert!(start.unwrap().is_ok());
            } else {
                assert!(matches!(result, Err(PlatformError::SecurityViolation(_))));
                let info = inspected.unwrap();
                assert_eq!(info.state.unwrap().status.unwrap().to_string(), "created");
            }
        }
    }

    #[test]
    fn platform_not_found_error_is_classified() {
        assert!(PlatformError::NotFound("missing-container".into()).is_not_found());
    }

    #[test]
    fn agent_host_config_is_hardened() {
        // F032/F037: agent containers drop ALL caps, forbid setuid privilege
        // gain, are never privileged or host-PID, and carry resource limits.
        use crate::types::ResourceLimits;
        let config = ContainerConfig {
            image: "agentforge/agent:latest".to_string(),
            name: Some("agent".to_string()),
            working_dir: None,
            env: vec![],
            labels: std::collections::HashMap::new(),
            resources: ResourceLimits::default(),
            network: None,
            mounts: vec![],
            privileged: true, // attacker-supplied; must be overridden
            host_pid: true,   // attacker-supplied; must be overridden
            tty: false,
            open_stdin: false,
            attach_stdin: false,
            attach_stdout: false,
            attach_stderr: false,
        };
        let hc = agent_host_config(&config);
        assert_eq!(hc.cap_drop, Some(vec!["ALL".to_string()]), "must drop ALL capabilities");
        assert_eq!(hc.security_opt, Some(vec!["no-new-privileges".to_string()]), "must forbid setuid privilege gain");
        assert_eq!(hc.privileged, Some(false), "must never be privileged");
        assert_eq!(hc.pid_mode, None, "must never share the host PID namespace");
        assert!(hc.memory.is_some(), "memory limit must be set");
        assert!(hc.pids_limit.is_some(), "pids limit must be set");
    }

    #[test]
    fn platform_internal_error_is_not_classified_as_not_found() {
        assert!(!PlatformError::Internal("docker socket unavailable".into()).is_not_found());
    }

    #[test]
    fn platform_missing_image_error_is_classified() {
        let err = PlatformError::Docker(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            message: "No such image: agentforge-agent:codex".into(),
        });
        assert!(err.is_missing_image());
    }

    #[test]
    fn invalid_stop_timeout_renders_a_typed_error() {
        let err = PlatformError::InvalidTimeout(i64::MAX);
        let rendered = err.to_string();
        assert!(rendered.contains("Invalid stop timeout"));
        assert!(rendered.contains(&i64::MAX.to_string()));
    }
}
