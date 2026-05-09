//! NATS Authorization Callout service.
//!
//! Implements per-agent NATS authentication via the ADR-26 callout pattern
//! (https://github.com/nats-io/nats-architecture-and-design/blob/main/adr/ADR-26.md).
//!
//! On every CONNECT, the NATS server publishes a request to
//! `$SYS.REQ.USER.AUTH`, our callout service validates the presented password
//! against `agents.nats_connect_password`, and responds with a signed JWT user
//! claim that binds the connection to a subject allowlist scoped to the agent.
//!
//! Both directions of the callout exchange are end-to-end encrypted with NATS
//! XKeys (Curve25519 + XSalsa20-Poly1305 via NaCl box) — the `xkey` submodule
//! provides the byte-exact seal/open primitives.
//!
//! Phase 2 plan:
//! `docs/plans/2026-04-21-001-feat-nats-per-agent-auth-callout-plan.md`

pub mod handler;
pub mod jwt;
pub mod kick;
pub mod metrics;
pub mod perms;
pub mod worker;
pub mod xkey;

// Convenience re-exports for downstream units assembling the callout
// response. Keep this list narrow — only surface what Unit 7 / Unit 8 bind
// against so the module's public area stays auditable.
pub use handler::{CalloutResponse, CalloutSigningKeys, DEFAULT_JWT_TTL, handle_auth_request};
pub use jwt::{AuthorizationRequest, JwtError, NatsPermissions};
pub use kick::{ConnectionTracker, TrackedConnection};
pub use perms::build_agent_permissions;
pub use worker::{AuthCalloutService, AuthCalloutWorker};
