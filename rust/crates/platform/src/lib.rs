//! Wisdoverse Forge Platform — Docker container orchestration, security policy, and warm pool.
//!
//! Manages the full container lifecycle for agent execution:
//! - **types**: Container configuration, resource limits, mount specifications
//! - **security**: Policy validation (forbidden mounts, capabilities, resource limits)
//! - **docker**: Bollard-based Docker client with dev/prod connection modes
//! - **container**: Create, start, stop, remove, inspect containers
//! - **pool**: Warm container pool for fast agent startup

pub mod container;
pub mod docker;
pub mod grpc;
pub mod pool;
pub mod security;
pub mod types;

pub use container::PlatformError;
pub use docker::DockerClient;
pub use pool::{ContainerPool, PoolStatus};
pub use security::{SecurityViolation, validate_security};
pub use types::*;
