//! Container lifecycle management — create, start, stop, remove, inspect.

use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::models::HostConfig;

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

    #[error("Internal error: {0}")]
    Internal(String),
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

        // Translate bind mounts into Docker's legacy `HostConfig.Binds` format
        // (`/host:/container[:ro]`). `security::validate_security` already
        // rejects `/var/run/docker.sock` and other dangerous paths.
        let binds: Vec<String> = config
            .mounts
            .iter()
            .map(|m| {
                let suffix = if m.read_only { ":ro" } else { "" };
                format!("{}:{}{}", m.source, m.target, suffix)
            })
            .collect();

        let host_config = HostConfig {
            memory: config.resources.memory_bytes,
            memory_swap: config.resources.memory_swap_bytes,
            cpu_quota: config.resources.cpu_quota,
            pids_limit: config.resources.pids_limit,
            binds: if binds.is_empty() { None } else { Some(binds) },
            // Defense-in-depth: always override to false regardless of config
            privileged: Some(false),
            // Defense-in-depth: never allow host PID namespace regardless of config
            pid_mode: None,
            network_mode: config.network.clone(),
            ..Default::default()
        };

        let create_config = Config {
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

        let options = config.name.as_ref().map(|n| CreateContainerOptions { name: n.as_str(), platform: None });

        let response = self.inner().create_container(options, create_config).await.map_err(PlatformError::Docker)?;

        tracing::info!(
            container_id = %response.id,
            image = %config.image,
            "Container created"
        );
        Ok(response.id)
    }

    /// Start a previously created container.
    pub async fn start_container(&self, id: &str) -> Result<(), PlatformError> {
        self.inner().start_container(id, None::<StartContainerOptions<String>>).await.map_err(PlatformError::Docker)?;

        tracing::info!(container_id = %id, "Container started");
        Ok(())
    }

    /// Stop a running container with a timeout in seconds.
    pub async fn stop_container(&self, id: &str, timeout_secs: i64) -> Result<(), PlatformError> {
        self.inner()
            .stop_container(id, Some(StopContainerOptions { t: timeout_secs }))
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
            image: info.config.and_then(|c| c.image).unwrap_or_default(),
            status: state,
            created_at: info.created,
        })
    }
}
