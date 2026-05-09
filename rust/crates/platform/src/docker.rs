//! Docker client wrapper around bollard.
//!
//! Connects to the Docker daemon via local socket (dev) or environment-configured
//! TLS endpoint (production).

use agentforge_core::AppConfig;
use bollard::Docker;

/// Thin wrapper around the bollard Docker client.
pub struct DockerClient {
    client: Docker,
}

impl DockerClient {
    /// Connect to the Docker daemon.
    ///
    /// - **Development**: uses local socket (`/var/run/docker.sock`).
    /// - **Production**: uses env vars (`DOCKER_HOST`, `DOCKER_TLS_VERIFY`,
    ///   `DOCKER_CERT_PATH`) via `Docker::connect_with_defaults()`.
    pub fn new(config: &AppConfig) -> Result<Self, bollard::errors::Error> {
        let client = if config.is_production() {
            Docker::connect_with_defaults()?
        } else {
            Docker::connect_with_local_defaults()?
        };

        tracing::info!("Docker client connected");
        Ok(Self { client })
    }

    /// Create a client from an existing bollard `Docker` instance (useful for testing).
    #[cfg(test)]
    pub fn from_bollard(client: Docker) -> Self {
        Self { client }
    }

    /// Access the underlying bollard client.
    pub fn inner(&self) -> &Docker {
        &self.client
    }

    /// Ping the Docker daemon to verify connectivity.
    pub async fn check_health(&self) -> bool {
        match self.client.ping().await {
            Ok(_) => true,
            Err(err) => {
                tracing::warn!(error = %err, "Docker health check failed");
                false
            }
        }
    }
}
