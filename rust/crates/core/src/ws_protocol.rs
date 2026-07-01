//! Unified serde model for the platform → browser WebSocket frames (MS-3 PR-B).
//!
//! The WS gateway (`api` `gateway/ws.rs`) forwards frames to the browser three
//! ways: NATS pass-through of upstream-serialized bytes, direct-to-socket writes,
//! and out-of-band `{ok,error}` control frames. Historically each producer
//! hand-rolled its frame with a `serde_json::json!` literal or an ad-hoc struct,
//! so the wire contract lived in scattered string literals with no single source
//! of truth and no compiler check that a rename stayed in sync with the browser
//! dispatch (`shared/types/protocol.ts`) or the golden fixtures.
//!
//! [`ServerMessage`] unifies the frames that share the simple `{type, payload}`
//! envelope. Adjacent tagging (`tag = "type"`, `content = "payload"`) reproduces
//! that exact envelope, and each variant's rename is the wire `type` discriminator
//! the browser switches on. The golden fixtures under
//! `tests/fixtures/ws-protocol/` are round-tripped through this enum by
//! [`tests::server_message_roundtrips_every_fixture`], which is what makes the
//! `scripts/check-protocol-contract.mjs` drift gate authoritative: the enum
//! variants are the compiler-guaranteed source of truth for these `type` tags.
//!
//! Scope note (MS-3 phasing): this covers the four frames whose producers are
//! standalone builder functions — `cli_image.updated`, `project_clone:status_update`,
//! `terminal_output`, `terminal_error`. `event` and `turn_invalidate` still live in
//! `jobs::event_consumer`'s `BroadcastEnvelope` (they are coupled through the
//! `BroadcastBus::publish` trait and the `DecodedEvent` build path) and fold into
//! this enum in PR-D; `orchestration:*` reconciles in PR-E.
//!
//! Byte note: adjacent tagging emits `type` before `payload`; some legacy `json!`
//! producers emitted `payload` first (a `serde_json::Value` object sorts keys).
//! That difference is purely cosmetic — every consumer (`JSON.parse` in the
//! browser, the gateway's verbatim relay, `serde_json::Value` equality in the
//! round-trip test) is key-order-insensitive — so the round-trip assertion uses
//! `to_value == fixture` rather than a byte compare.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A platform → browser WebSocket frame with the `{ "type": ..., "payload": ... }`
/// envelope. Each variant's serde rename is the exact `type` discriminator on the
/// wire and in `shared/types/protocol.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMessage {
    /// Admin-only CLI agent-image auto-updater toast, delivered on the global
    /// `broadcast.admin.cli_image` subject. Producer: `jobs::cli_image_updater`.
    #[serde(rename = "cli_image.updated")]
    CliImageUpdated(CliImageUpdatedPayload),
    /// Project git-clone status transition. Producer: `api` `domain::project_clone`.
    #[serde(rename = "project_clone:status_update")]
    ProjectCloneStatusUpdate(ProjectCloneStatusPayload),
    /// A chunk of terminal output for an attached agent container. Producer:
    /// `api` `domain::gateway`, written straight to the socket.
    #[serde(rename = "terminal_output")]
    TerminalOutput(TerminalOutputPayload),
    /// A terminal attach/input failure for an agent container. Producer: `api`
    /// `domain::gateway`, written straight to the socket.
    #[serde(rename = "terminal_error")]
    TerminalError(TerminalErrorPayload),
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
    /// key-order-insensitive, so it tolerates the cosmetic `type`/`payload`
    /// ordering shift from adjacent tagging (see the module byte note).
    #[test]
    fn server_message_roundtrips_every_fixture() {
        // Paths are relative to THIS source file: src → core → crates → rust → repo root.
        let fixtures: [(&str, &str); 4] = [
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
