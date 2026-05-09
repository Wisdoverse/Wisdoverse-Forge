//! Wisdoverse Forge Infra — Redis, NATS, and object-storage client integrations.
//!
//! Provides infrastructure client wrappers with graceful degradation.
//! Redis is optional (circuit breaker pattern — degrades gracefully when unavailable).
//! NATS is used for the event pipeline and WebSocket broadcast.

pub mod nats;
pub mod object_storage;
pub mod redis_client;

pub use self::nats::NatsClient;
pub use self::object_storage::ObjectStorageClient;
pub use self::redis_client::RedisClient;

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
