//! Wisdoverse Forge Platform — Docker container orchestration, security policy, and warm pool.
//!
//! Manages the full container lifecycle for agent execution:
//! - **types**: Container configuration, resource limits, mount specifications
//! - **security**: Policy validation (forbidden mounts, capabilities, resource limits)
//! - **docker**: Bollard-based Docker client with dev/prod connection modes
//! - **container**: Create, start, stop, remove, inspect containers
//! - **pool**: Warm container pool for fast agent startup
//! - **clone_runtime**: Ephemeral git-clone container runtime (project-git-clone)

pub mod clone_runtime;
pub mod container;
pub mod docker;
pub mod grpc;
pub mod image;
pub mod pool;
pub mod security;
pub mod types;

pub use clone_runtime::{
    CLONE_EGRESS_NETWORK, CLONE_LABEL_KEY, CLONE_MAX_BYTES_ENV, CloneContainerConfig, CloneContainerState,
    CloneContainerSummary, CloneDockerBackend, CloneRunOutcome, CloneRunSpec, CloneRuntime, CloneSecretRoot,
    DEFAULT_CLONE_MAX_BYTES, LiveCloneDockerBackend, NetworkInspectInfo, RawStderr, SecretBytes,
};
pub use container::PlatformError;
pub use docker::DockerClient;
pub use image::{LocalImage, RemoveOutcome};
pub use pool::{ContainerPool, PoolStatus};
pub use security::{SecurityViolation, validate_security};
pub use types::*;
