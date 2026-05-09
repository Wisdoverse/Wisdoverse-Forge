//! Container configuration types for the platform crate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for creating a Docker container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Docker image to use (e.g. "agentforge/agent-claude:latest").
    pub image: String,
    /// Optional container name.
    pub name: Option<String>,
    /// Optional working directory inside the container.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Environment variables in KEY=VALUE format.
    pub env: Vec<String>,
    /// Container labels for filtering and metadata.
    pub labels: HashMap<String, String>,
    /// Resource limits (CPU, memory, PIDs).
    pub resources: ResourceLimits,
    /// Docker network to attach to.
    pub network: Option<String>,
    /// Bind mounts.
    pub mounts: Vec<Mount>,
    /// Whether to run the container in privileged mode (always overridden to false).
    #[serde(default)]
    pub privileged: bool,
    /// Whether to use the host PID namespace (always denied).
    #[serde(default)]
    pub host_pid: bool,
    /// Allocate a TTY for interactive agent containers.
    #[serde(default)]
    pub tty: bool,
    /// Keep stdin open so browser terminal sessions can attach later.
    #[serde(default)]
    pub open_stdin: bool,
    /// Attach stdin to the container's primary process.
    #[serde(default)]
    pub attach_stdin: bool,
    /// Attach stdout to the container's primary process.
    #[serde(default)]
    pub attach_stdout: bool,
    /// Attach stderr to the container's primary process.
    #[serde(default)]
    pub attach_stderr: bool,
}

/// Resource limits enforced on every container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU quota in microseconds (100_000 = 1 CPU core).
    pub cpu_quota: Option<i64>,
    /// Memory limit in bytes.
    pub memory_bytes: Option<i64>,
    /// Memory + swap limit in bytes.
    pub memory_swap_bytes: Option<i64>,
    /// Maximum number of PIDs inside the container.
    pub pids_limit: Option<i64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_quota: Some(100_000),                    // 1 CPU
            memory_bytes: Some(512 * 1024 * 1024),       // 512 MB
            memory_swap_bytes: Some(1024 * 1024 * 1024), // 1 GB
            pids_limit: Some(256),
        }
    }
}

/// A bind mount specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mount {
    /// Host path.
    pub source: String,
    /// Container path.
    pub target: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

/// Information about an existing container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: ContainerState,
    pub created_at: Option<String>,
}

/// Container lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Stopped,
    Dead,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resource_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.cpu_quota, Some(100_000));
        assert_eq!(limits.memory_bytes, Some(512 * 1024 * 1024));
        assert_eq!(limits.memory_swap_bytes, Some(1024 * 1024 * 1024));
        assert_eq!(limits.pids_limit, Some(256));
    }

    #[test]
    fn container_state_serializes_lowercase() {
        let running = serde_json::to_string(&ContainerState::Running).unwrap();
        assert_eq!(running, r#""running""#);

        let stopped = serde_json::to_string(&ContainerState::Stopped).unwrap();
        assert_eq!(stopped, r#""stopped""#);

        let unknown = serde_json::to_string(&ContainerState::Unknown).unwrap();
        assert_eq!(unknown, r#""unknown""#);
    }

    #[test]
    fn container_state_deserializes_lowercase() {
        let state: ContainerState = serde_json::from_str(r#""running""#).unwrap();
        assert_eq!(state, ContainerState::Running);

        let state: ContainerState = serde_json::from_str(r#""dead""#).unwrap();
        assert_eq!(state, ContainerState::Dead);
    }

    #[test]
    fn container_info_roundtrip() {
        let info = ContainerInfo {
            id: "abc123".to_string(),
            name: "test-agent".to_string(),
            image: "agentforge/agent:latest".to_string(),
            status: ContainerState::Running,
            created_at: Some("2026-04-04T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ContainerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "abc123");
        assert_eq!(deserialized.status, ContainerState::Running);
    }
}
