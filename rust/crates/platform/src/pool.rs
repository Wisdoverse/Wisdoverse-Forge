//! Warm container pool for fast agent startup.
//!
//! Keeps a pool of pre-created containers ready for immediate allocation,
//! reducing cold-start latency for agent execution.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::container::PlatformError;
use crate::docker::DockerClient;
use crate::types::ContainerConfig;

/// A pool of warm (pre-created) containers that can be allocated on demand.
pub struct ContainerPool {
    docker: DockerClient,
    warm_containers: Mutex<VecDeque<String>>,
    config_template: ContainerConfig,
    max_pool_size: usize,
    min_pool_size: usize,
}

impl ContainerPool {
    /// Create a new container pool.
    ///
    /// - `config_template`: the config used to create warm containers.
    /// - `min_pool_size`: the pool will be warmed up to at least this many containers.
    /// - `max_pool_size`: released containers beyond this count are destroyed.
    pub fn new(
        docker: DockerClient,
        config_template: ContainerConfig,
        min_pool_size: usize,
        max_pool_size: usize,
    ) -> Self {
        Self { docker, warm_containers: Mutex::new(VecDeque::new()), config_template, max_pool_size, min_pool_size }
    }

    /// Allocate a container from the pool.
    ///
    /// If a warm container is available it is returned immediately.
    /// Otherwise a new container is created and started on the fly.
    pub async fn allocate(&self) -> Result<String, PlatformError> {
        let mut pool = self.warm_containers.lock().await;

        if let Some(container_id) = pool.pop_front() {
            tracing::debug!(container_id = %container_id, "Allocated warm container");
            return Ok(container_id);
        }
        drop(pool); // Release lock before creating

        tracing::info!("Pool empty, creating new container");
        let container_id = self.docker.create_container(self.config_template.clone()).await?;
        self.docker.start_container(&container_id).await?;
        Ok(container_id)
    }

    /// Release a container back to the pool.
    ///
    /// If the pool is below `max_pool_size`, the container is stopped and kept.
    /// Otherwise it is stopped and removed.
    pub async fn release(&self, container_id: String) -> Result<(), PlatformError> {
        let mut pool = self.warm_containers.lock().await;

        if pool.len() < self.max_pool_size {
            self.docker.stop_container(&container_id, 10).await?;
            pool.push_back(container_id);
            tracing::debug!(pool_size = pool.len(), "Container returned to pool");
        } else {
            drop(pool);
            self.docker.stop_container(&container_id, 10).await?;
            self.docker.remove_container(&container_id, true).await?;
            tracing::debug!("Pool full, container destroyed");
        }
        Ok(())
    }

    /// Warm up the pool to `min_pool_size` by pre-creating containers.
    pub async fn warm(&self) -> Result<(), PlatformError> {
        let current = self.warm_containers.lock().await.len();
        let needed = self.min_pool_size.saturating_sub(current);

        for _ in 0..needed {
            let id = self.docker.create_container(self.config_template.clone()).await?;
            self.warm_containers.lock().await.push_back(id);
        }

        if needed > 0 {
            tracing::info!(added = needed, total = current + needed, "Pool warmed");
        }
        Ok(())
    }

    /// Drain all containers from the pool, removing them from Docker.
    ///
    /// Used during graceful shutdown.
    pub async fn drain(&self) -> Result<(), PlatformError> {
        let mut pool = self.warm_containers.lock().await;
        let ids: Vec<String> = pool.drain(..).collect();
        drop(pool);

        for id in ids {
            if let Err(err) = self.docker.remove_container(&id, true).await {
                tracing::warn!(
                    error = %err,
                    container_id = %id,
                    "Failed to remove container during drain"
                );
            }
        }
        tracing::info!("Pool drained");
        Ok(())
    }

    /// Get current pool status.
    pub async fn status(&self) -> PoolStatus {
        let pool = self.warm_containers.lock().await;
        PoolStatus { warm_count: pool.len(), min_size: self.min_pool_size, max_size: self.max_pool_size }
    }
}

/// Snapshot of pool state, suitable for health checks and metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatus {
    pub warm_count: usize,
    pub min_size: usize,
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_status_serialization() {
        let status = PoolStatus { warm_count: 3, min_size: 2, max_size: 10 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"warm_count\":3"));
        assert!(json.contains("\"min_size\":2"));
        assert!(json.contains("\"max_size\":10"));

        let deserialized: PoolStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.warm_count, 3);
        assert_eq!(deserialized.min_size, 2);
        assert_eq!(deserialized.max_size, 10);
    }
}
