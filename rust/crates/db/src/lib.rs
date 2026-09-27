//! Wisdoverse Forge DB — SQLx connection pool, database migrations, and entity definitions.
//!
//! Manages the PostgreSQL connection pool and provides typed entity structs
//! for all database tables. Depends on `agentforge-core` for shared ID types.

pub mod agent_lifecycle_lock;
pub mod entities;
pub mod inbox_notifications;
pub mod manifest;
pub mod pool;

// Convenient re-exports
pub use agent_lifecycle_lock::{
    agent_work_admission_is_idle_in_tx, lock_agent_lifecycle_in_tx, try_lock_cli_image_roll_in_tx,
};
pub use entities::*;
pub use manifest::{ManifestError, verify_manifest};
pub use pool::{RunMigrationsError, check_health, create_pool, run_migrations};

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
