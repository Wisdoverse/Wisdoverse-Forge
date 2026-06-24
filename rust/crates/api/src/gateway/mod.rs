//! WebSocket gateway for real-time event broadcasting.
//!
//! Provides a NATS-backed WebSocket endpoint that streams events to
//! authenticated clients, filtered by tenant (org_id).

pub mod ws;
