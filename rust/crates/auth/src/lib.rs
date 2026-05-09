//! Wisdoverse Forge Auth — JWT authentication, Argon2id password hashing, and Axum middleware.
//!
//! This crate provides:
//! - [`Claims`] — JWT token payload structure
//! - [`JwtManager`] — token creation and verification (HS256, ES256 migration planned)
//! - [`AuthUser`] — Axum extractor that validates JWT and provides [`TenantScope`]
//! - [`password`] — Argon2id password hashing and verification
//!
//! # Usage
//!
//! ```ignore
//! use agentforge_auth::{JwtManager, AuthUser};
//!
//! // In app setup:
//! let jwt = Arc::new(JwtManager::new(&config.jwt_secret, config.jwt_expiry_seconds));
//! let app = Router::new()
//!     .route("/api/agents", get(list_agents))
//!     .layer(Extension(jwt));
//!
//! // In handlers:
//! async fn list_agents(auth: AuthUser) -> impl IntoResponse {
//!     let scope = auth.scope; // TenantScope for DB queries
//! }
//! ```

pub mod claims;
pub mod jwt;
pub mod middleware;
pub mod password;

pub use claims::Claims;
pub use jwt::JwtManager;
pub use middleware::AuthUser;
