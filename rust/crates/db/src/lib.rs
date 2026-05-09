//! Wisdoverse Forge DB — SQLx connection pool, database migrations, and entity definitions.
//!
//! Manages the PostgreSQL connection pool and provides typed entity structs
//! for all database tables. Depends on `agentforge-core` for shared ID types.

pub mod entities;
pub mod inbox_notifications;
pub mod pool;

// Convenient re-exports
pub use entities::*;
pub use pool::{check_health, create_pool, run_migrations};

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
