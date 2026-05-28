//! Wisdoverse Forge API — Axum HTTP routes, request handlers, and WebSocket gateway.
//!
//! Defines the REST API surface and real-time WebSocket connections for the
//! Wisdoverse Forge platform. Depends on `agentforge-core`, `agentforge-db`,
//! `agentforge-auth`, and `agentforge-infra`.
//!
//! User- and operator-facing terminology in route/service/repository docs must
//! follow `docs/architecture/glossary.md` in the repository root.

pub mod domain;
pub mod gateway;
pub mod health;
pub mod mcp;
pub mod middleware;
pub mod observability;
pub mod repositories;
pub mod router;
pub mod routes;
pub mod services;
mod state_services;

pub mod testing;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod router_tests;

// Convenient re-exports
pub use health::AppState;
pub use router::create_router;

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
