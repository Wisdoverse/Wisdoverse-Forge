//! Unified serde model for the platform → browser WebSocket frames (MS-3 PR-B/PR-D).
//!
//! The WS gateway (`api` `gateway/ws.rs`) forwards frames to the browser three
//! ways: NATS pass-through of upstream-serialized bytes, direct-to-socket writes,
//! and out-of-band `{ok,error}` control frames. Historically each producer
//! hand-rolled its frame with a `serde_json::json!` literal or an ad-hoc struct,
//! so the wire contract lived in scattered string literals with no single source
//! of truth and no compiler check that a rename stayed in sync with the browser
//! dispatch (`shared/types/protocol.ts`) or the golden fixtures.
//!
//! [`ServerMessage`] is internally tagged on `type` — the discriminator the
//! browser dispatch switches on. Most frames carry their detail under a single
//! `payload` field; the `event` frame is FLAT (its four fields sit beside `type`),
//! which is exactly why internal tagging is used rather than adjacent
//! `content = "payload"` tagging. The golden fixtures under
//! `tests/fixtures/ws-protocol/` are round-tripped through this enum by
//! [`tests::server_message_roundtrips_every_fixture`], which is what makes the
//! `scripts/check-protocol-contract.mjs` drift gate authoritative: the enum
//! variants are the compiler-guaranteed source of truth for these `type` tags.
//!
//! Scope note (MS-3 phasing): `orchestration:task_update` /
//! `orchestration:participant_update` still have scattered producers with a known
//! internal divergence; they join this enum in PR-E once reconciled.
//!
//! Byte note: internal tagging emits `type` first, then fields in declaration
//! order; some legacy `json!` producers emitted `payload` first (a
//! `serde_json::Value` object sorts keys). That difference is purely cosmetic —
//! every consumer (`JSON.parse` in the browser, the gateway's verbatim relay,
//! `serde_json::Value` equality in the round-trip test) is key-order-insensitive —
//! so the round-trip assertion uses `to_value == fixture` rather than a byte compare.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A platform → browser WebSocket frame, internally tagged on `type`. Each
/// variant's serde rename is the exact `type` discriminator on the wire and in
/// `shared/types/protocol.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// A relayed agent activity event — the most-trafficked frame. FLAT on the
    /// wire: `eventType`/`eventData`/`agentId`/`orgId` sit beside `type`, with no
    /// `payload` wrapper. `event_data` is the normalized event object
    /// (`normalize_event_data` in `jobs::event_consumer` guarantees injected
    /// `type`/`orgId`/`sessionId`/`timestamp`/`id` fields). `agent_id` carries the
    /// CLI session id when known, else the agent UUID.
    #[serde(rename = "event")]
    Event {
        #[serde(rename = "eventType")]
        event_type: String,
        #[serde(rename = "eventData")]
        event_data: Value,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "orgId")]
        org_id: String,
    },
    /// Invalidates the browser's cached turn projection for an agent after a
    /// persisted event lands. Producer: `jobs::event_consumer`, published right
    /// after the `event` frame for persistable events.
    #[serde(rename = "turn_invalidate")]
    TurnInvalidate { payload: TurnInvalidatePayload },
    /// Admin-only CLI agent-image auto-updater toast, delivered on the global
    /// `broadcast.admin.cli_image` subject. Producer: `jobs::cli_image_updater`.
    #[serde(rename = "cli_image.updated")]
    CliImageUpdated { payload: CliImageUpdatedPayload },
    /// Project git-clone status transition. Producer: `api` `domain::project_clone`.
    #[serde(rename = "project_clone:status_update")]
    ProjectCloneStatusUpdate { payload: ProjectCloneStatusPayload },
    /// A chunk of terminal output for an attached agent container. Producer:
    /// `api` `domain::gateway`, written straight to the socket.
    #[serde(rename = "terminal_output")]
    TerminalOutput { payload: TerminalOutputPayload },
    /// A terminal attach/input failure for an agent container. Producer: `api`
    /// `domain::gateway`, written straight to the socket.
    #[serde(rename = "terminal_error")]
    TerminalError { payload: TerminalErrorPayload },
}

impl ServerMessage {
    /// Serialize to the exact JSON string the gateway forwards to the browser.
    /// Serializing a fixed-shape struct is infallible in practice; the `Result`
    /// is surfaced so callers keep their existing error path rather than panic.
    pub fn to_frame_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to a `serde_json::Value` for producers that build a `Value`
    /// (e.g. the project-clone worker, which later `to_vec`s it).
    pub fn to_frame_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Payload of [`ServerMessage::TurnInvalidate`]. `agent_id` is a string (the
/// agent UUID rendered as text) and `timestamp` is unix milliseconds — both
/// mirror the legacy `TurnInvalidateMessage` wire shape exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnInvalidatePayload {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub timestamp: i64,
}

/// Payload of [`ServerMessage::CliImageUpdated`]. `Option` fields serialize as
/// `null` (not omitted) to match the legacy `json!` producer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliImageUpdatedPayload {
    pub tool: String,
    pub state: String,
    #[serde(rename = "localDigest")]
    pub local_digest: Option<String>,
    #[serde(rename = "remoteDigest")]
    pub remote_digest: Option<String>,
    #[serde(rename = "localVersion")]
    pub local_version: Option<String>,
    #[serde(rename = "remoteVersion")]
    pub remote_version: Option<String>,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
    #[serde(rename = "eventId")]
    pub event_id: String,
    pub unix: i64,
}

/// Payload of [`ServerMessage::ProjectCloneStatusUpdate`]. `details` is an opaque
/// snake_case audit object passed through verbatim from the clone worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCloneStatusPayload {
    pub action: String,
    #[serde(rename = "eventId")]
    pub event_id: Uuid,
    #[serde(rename = "projectId")]
    pub project_id: Uuid,
    #[serde(rename = "cloneStatus")]
    pub clone_status: String,
    pub details: Value,
}

/// Payload of [`ServerMessage::TerminalOutput`]. `data` is standard-alphabet
/// base64 (with `=` padding) of the raw PTY bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalOutputPayload {
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
    pub data: String,
}

/// Payload of [`ServerMessage::TerminalError`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalErrorPayload {
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each golden fixture must be a lossless round-trip through `ServerMessage`:
    /// deserialize the pinned wire shape into the enum, re-serialize, and assert
    /// structural equality with the fixture. This is the authoritative proof that
    /// the enum reproduces every pinned frame — the JS drift gate
    /// (`scripts/check-protocol-contract.mjs`) relies on it. `Value` equality is
    /// key-order-insensitive, so it tolerates cosmetic key-ordering shifts (see
    /// the module byte note).
    #[test]
    fn server_message_roundtrips_every_fixture() {
        // Paths are relative to THIS source file: src → core → crates → rust → repo root.
        let fixtures: [(&str, &str); 6] = [
            ("event", include_str!("../../../../tests/fixtures/ws-protocol/event.json")),
            ("turn_invalidate", include_str!("../../../../tests/fixtures/ws-protocol/turn_invalidate.json")),
            ("cli_image.updated", include_str!("../../../../tests/fixtures/ws-protocol/cli_image.updated.json")),
            (
                "project_clone:status_update",
                include_str!("../../../../tests/fixtures/ws-protocol/project_clone_status_update.json"),
            ),
            ("terminal_output", include_str!("../../../../tests/fixtures/ws-protocol/terminal_output.json")),
            ("terminal_error", include_str!("../../../../tests/fixtures/ws-protocol/terminal_error.json")),
        ];

        for (tag, raw) in fixtures {
            let fixture: Value =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("{tag}: fixture is not valid JSON: {e}"));
            let msg: ServerMessage = serde_json::from_value(fixture.clone())
                .unwrap_or_else(|e| panic!("{tag}: fixture does not deserialize into ServerMessage: {e}"));
            let reserialized =
                serde_json::to_value(&msg).unwrap_or_else(|e| panic!("{tag}: ServerMessage does not serialize: {e}"));
            assert_eq!(
                reserialized, fixture,
                "{tag}: ServerMessage round-trip is not structurally identical to the golden fixture"
            );
            // The re-serialized frame must carry the tag as its `type` (the browser switches on it).
            assert_eq!(
                reserialized.get("type").and_then(Value::as_str),
                Some(tag),
                "{tag}: serialized frame has the wrong `type` discriminator"
            );
        }
    }
}
