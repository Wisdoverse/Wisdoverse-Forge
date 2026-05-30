//! Wisdoverse Forge Core — shared types, error definitions, configuration, and tenant scoping.
//!
//! This crate provides the foundational types used across all other Wisdoverse Forge crates.
//! It has no internal crate dependencies, making it the leaf of the dependency graph.

pub mod config;
pub mod context_envelope;
pub mod credential_protocol;
pub mod crypto;
pub mod error;
pub mod event_protocol;
pub mod orchestration_protocol;
pub mod runtime_capability;
pub mod tenant;
pub mod types;

// Convenient re-exports
pub use config::{AppConfig, NatsCalloutConfig, StripeConfig};
pub use error::{AppError, AppResult, ErrorKind};
pub use runtime_capability::{CliToolKind, RuntimeCapability, RuntimeCapabilityError, RuntimeKind};
pub use tenant::{ScopeKind, ScopedRead, ScopedWrite, ScopedWriteError, TenantScope};
pub use types::*;

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
