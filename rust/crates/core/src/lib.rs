//! Wisdoverse Forge Core — shared types, error definitions, configuration, and tenant scoping.
//!
//! This crate provides the foundational types used across all other Wisdoverse Forge crates.
//! It has no internal crate dependencies, making it the leaf of the dependency graph.

pub mod broadcast_protocol;
pub mod clone_protocol;
pub mod completion_verifier;
pub mod config;
pub mod context_envelope;
pub mod credential_protocol;
pub mod crypto;
pub mod error;
pub mod event_protocol;
pub mod orchestration_protocol;
pub mod runtime_capability;
pub mod self_fix_protocol;
pub mod tenant;
pub mod types;

// Convenient re-exports
pub use completion_verifier::{CompletionVerifier, ExpectedResult};
pub use config::{AppConfig, NatsCalloutConfig, StripeConfig, ensure_external_state_redis_ready};
pub use error::{AppError, AppResult, ErrorKind};
pub use runtime_capability::{CliToolKind, RuntimeCapability, RuntimeCapabilityError, RuntimeKind};
pub use self_fix_protocol::{SELF_FIX_PR_QUEUE, SelfFixPrJob};
pub use tenant::{ScopeKind, ScopedRead, ScopedWrite, ScopedWriteError, TenantScope};
pub use types::*;

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
