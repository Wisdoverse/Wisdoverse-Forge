use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_core::broadcast_protocol::ADMIN_CLI_IMAGE_SUBJECT;
use agentforge_core::ws_protocol::{ServerMessage, TerminalErrorPayload, TerminalOutputPayload};
use agentforge_core::{AppError, ErrorKind, TenantScope};

/// A browser → gateway control message (MS-3 PR-C). The five `terminal_*` tags are
/// the only client messages the WS handler acts on; every other tag deserializes to
/// nothing and is a silent no-op (the handler's `None` arm), preserving the historic
/// lenient parse. The wire shape is `{ "type": <tag>, "payload": { … } }` (adjacent
/// tagging), mirroring `ClientMessage` in `shared/types/protocol.ts`.
///
/// Field-level leniency is preserved from the previous dynamic-getter parse so a
/// malformed field degrades gracefully instead of dropping the whole message where
/// it used to be tolerated: `cols`/`rows` fall back to their default on any
/// non-`u16` value (see [`lenient_opt_u16`]) and `keys` silently drops non-string
/// entries (see [`lenient_string_vec`]). A missing/invalid `agentId`, however, makes
/// the whole message a no-op — exactly as the old `terminal_payload_agent_id` guard did.
///
/// Lives in the `api` crate (not `core::ws_protocol` with `ServerMessage`) because the
/// browser → Rust direction is consumed only here; `jobs` never parses it.
// The shared `Terminal` prefix is intentional: every current client message is a
// terminal-control frame whose wire tag is `terminal_*`, and a future non-terminal
// client message would not share it, so the prefix is meaningful, not redundant.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub(crate) enum ClientMessage {
    #[serde(rename = "terminal_attach")]
    TerminalAttach {
        #[serde(rename = "agentId")]
        agent_id: Uuid,
        #[serde(default, deserialize_with = "lenient_opt_u16")]
        cols: Option<u16>,
        #[serde(default, deserialize_with = "lenient_opt_u16")]
        rows: Option<u16>,
    },
    #[serde(rename = "terminal_data")]
    TerminalData {
        #[serde(rename = "agentId")]
        agent_id: Uuid,
        data: String,
    },
    #[serde(rename = "terminal_input")]
    TerminalInput {
        #[serde(rename = "agentId")]
        agent_id: Uuid,
        #[serde(default, deserialize_with = "lenient_string_vec")]
        keys: Vec<String>,
    },
    #[serde(rename = "terminal_resize")]
    TerminalResize {
        #[serde(rename = "agentId")]
        agent_id: Uuid,
        #[serde(default, deserialize_with = "lenient_opt_u16")]
        cols: Option<u16>,
        #[serde(default, deserialize_with = "lenient_opt_u16")]
        rows: Option<u16>,
    },
    #[serde(rename = "terminal_detach")]
    TerminalDetach {
        #[serde(rename = "agentId")]
        agent_id: Uuid,
    },
}

/// Deserialize a terminal dimension leniently: any value that is not a `u16`-range
/// unsigned integer (a string, float, negative, oversize, null, …) yields `None` so
/// the caller applies its default (80 cols / 24 rows), never failing the whole
/// message. Mirrors the old `terminal_payload_dimension` getter.
fn lenient_opt_u16<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_u64().and_then(|n| u16::try_from(n).ok()))
}

/// Deserialize terminal key chords, dropping any non-string entry (and treating a
/// non-array value as empty) rather than failing the message. Mirrors the old
/// `keys.iter().filter_map(Value::as_str)` extraction.
fn lenient_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default())
}

pub(crate) enum WebSocketOriginRejection {
    Disallowed(String),
    MissingInProduction,
}

impl WebSocketOriginRejection {
    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Forbidden("forbidden".into()).into()
    }
}

pub(crate) struct WebSocketOriginPolicy;

impl WebSocketOriginPolicy {
    pub(crate) fn validate(
        origin: Option<&str>,
        allowed_origin: Option<&str>,
        production: bool,
    ) -> Result<(), WebSocketOriginRejection> {
        let Some(allowed_origin) = allowed_origin else {
            return Ok(());
        };

        match origin {
            Some(value) if value == allowed_origin => Ok(()),
            Some(value) => Err(WebSocketOriginRejection::Disallowed(value.to_string())),
            None if production => Err(WebSocketOriginRejection::MissingInProduction),
            None => Ok(()),
        }
    }
}

pub(crate) enum GatewayTerminalAttachTarget {
    Ready { container_id: String },
    Rejected { message: String },
}

impl GatewayTerminalAttachTarget {
    pub(crate) fn ready(container_id: String) -> Self {
        Self::Ready { container_id }
    }

    pub(crate) fn missing_container() -> Self {
        Self::Rejected { message: "agent has no running container".to_string() }
    }

    pub(crate) fn lookup_failed(kind: &ErrorKind) -> Self {
        Self::Rejected { message: format!("agent lookup failed: {kind}") }
    }
}

pub(crate) fn websocket_unauthorized_error() -> AppError {
    ErrorKind::Unauthorized.into()
}

pub(crate) fn realtime_unavailable_frame() -> String {
    json!({"ok": false, "error": "real-time updates unavailable"}).to_string()
}

pub(crate) fn realtime_disconnected_frame() -> String {
    json!({"ok": false, "error": "real-time updates disconnected"}).to_string()
}

pub(crate) fn subscription_subjects(scope: &TenantScope) -> Vec<String> {
    let org_id = scope.org_id().as_uuid();
    let mut subjects =
        vec![format!("broadcast.{org_id}"), format!("broadcast.{org_id}.scope.user.{}", scope.user_id().as_uuid())];
    if let Some(team_id) = scope.team_id() {
        subjects.push(format!("broadcast.{org_id}.scope.team.{}", team_id.as_uuid()));
    }
    if let Some(project_id) = scope.project_id() {
        subjects.push(format!("broadcast.{org_id}.scope.project.{}", project_id.as_uuid()));
    }
    subjects
}

/// Audience-scoped subjects this connection should additionally subscribe to,
/// based on its JWT role. The CLI agent-image toast is delivered on a single
/// global subject only `owner`/`admin` connections join (mirrors the backend
/// `AdminService::require_admin` audience). Every agent JWT denies `broadcast.>`,
/// so a sidecar can neither read nor spoof this subject.
pub(crate) fn admin_subscription_subjects(role: &str) -> Vec<String> {
    if role == "owner" || role == "admin" { vec![ADMIN_CLI_IMAGE_SUBJECT.to_string()] } else { Vec::new() }
}

/// Parse a browser control frame into a typed [`ClientMessage`], or `None` for an
/// unparseable / unknown-tag frame (the handler treats `None` as a silent no-op, the
/// historic D7 behaviour). A frame with a known tag but a missing/invalid `agentId`
/// also fails to `None` here, matching the old per-handler `agentId` guard.
pub(crate) fn parse_gateway_client_message(text: &str) -> Option<ClientMessage> {
    serde_json::from_str(text).ok()
}

pub(crate) fn terminal_output_frame(agent_id: Uuid, output: &[u8]) -> String {
    // Serializing a fixed-shape `{agentId, data}` payload cannot fail; a failure
    // here would mean a corrupt build, so surface it loudly rather than emit a
    // malformed frame the browser would fail to `JSON.parse`.
    ServerMessage::TerminalOutput(TerminalOutputPayload { agent_id, data: BASE64.encode(output) })
        .to_frame_string()
        .expect("terminal_output frame serialization is infallible")
}

pub(crate) fn terminal_error_frame(agent_id: Uuid, message: impl Into<String>) -> String {
    ServerMessage::TerminalError(TerminalErrorPayload { agent_id, message: message.into() })
        .to_frame_string()
        .expect("terminal_error frame serialization is infallible")
}

pub(crate) fn docker_unavailable_message() -> &'static str {
    "Docker is not available"
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentforge_core::{OrgId, ProjectId, TeamId, UserId, WorkspaceId};

    #[test]
    fn origin_policy_rejects_cross_origin_and_missing_production_origin() {
        assert!(WebSocketOriginPolicy::validate(Some("https://app.test"), Some("https://app.test"), true).is_ok());
        assert!(matches!(
            WebSocketOriginPolicy::validate(Some("https://evil.test"), Some("https://app.test"), false),
            Err(WebSocketOriginRejection::Disallowed(origin)) if origin == "https://evil.test"
        ));
        assert!(matches!(
            WebSocketOriginPolicy::validate(None, Some("https://app.test"), true),
            Err(WebSocketOriginRejection::MissingInProduction)
        ));
        assert!(WebSocketOriginPolicy::validate(None, Some("https://app.test"), false).is_ok());
    }

    #[test]
    fn subscription_subjects_include_available_scope_axes() {
        let org_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let team_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let scope = TenantScope::with_axes(
            OrgId::from(org_id),
            UserId::from(user_id),
            Some(WorkspaceId::from(Uuid::now_v7())),
            Some(TeamId::from(team_id)),
            Some(ProjectId::from(project_id)),
        );

        assert_eq!(
            subscription_subjects(&scope),
            vec![
                format!("broadcast.{org_id}"),
                format!("broadcast.{org_id}.scope.user.{user_id}"),
                format!("broadcast.{org_id}.scope.team.{team_id}"),
                format!("broadcast.{org_id}.scope.project.{project_id}"),
            ]
        );
    }

    #[test]
    fn admin_subjects_gated_to_owner_and_admin() {
        assert_eq!(admin_subscription_subjects("owner"), vec![ADMIN_CLI_IMAGE_SUBJECT.to_string()]);
        assert_eq!(admin_subscription_subjects("admin"), vec![ADMIN_CLI_IMAGE_SUBJECT.to_string()]);
        // every non-admin role gets no admin subject — the toast never leaks.
        for role in ["member", "viewer", "billing", ""] {
            assert!(admin_subscription_subjects(role).is_empty(), "role {role} must not join the admin subject");
        }
    }

    #[test]
    fn realtime_warning_frames_keep_browser_contract() {
        assert_eq!(realtime_unavailable_frame(), r#"{"error":"real-time updates unavailable","ok":false}"#);
        assert_eq!(realtime_disconnected_frame(), r#"{"error":"real-time updates disconnected","ok":false}"#);
    }

    #[test]
    fn client_message_parses_every_terminal_variant() {
        let agent_id = Uuid::now_v7();
        let id = agent_id.to_string();

        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_attach","payload":{{"agentId":"{id}","cols":120,"rows":33}}}}"#
            )),
            Some(ClientMessage::TerminalAttach { agent_id, cols: Some(120), rows: Some(33) })
        );
        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_data","payload":{{"agentId":"{id}","data":"ls\n"}}}}"#
            )),
            Some(ClientMessage::TerminalData { agent_id, data: "ls\n".to_string() })
        );
        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_input","payload":{{"agentId":"{id}","keys":["a","b"]}}}}"#
            )),
            Some(ClientMessage::TerminalInput { agent_id, keys: vec!["a".to_string(), "b".to_string()] })
        );
        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_resize","payload":{{"agentId":"{id}","cols":90,"rows":20}}}}"#
            )),
            Some(ClientMessage::TerminalResize { agent_id, cols: Some(90), rows: Some(20) })
        );
        assert_eq!(
            parse_gateway_client_message(&format!(r#"{{"type":"terminal_detach","payload":{{"agentId":"{id}"}}}}"#)),
            Some(ClientMessage::TerminalDetach { agent_id })
        );
    }

    #[test]
    fn client_message_preserves_field_level_leniency() {
        let agent_id = Uuid::now_v7();
        let id = agent_id.to_string();

        // Missing cols/rows → None so the handler applies its 80/24 default.
        assert_eq!(
            parse_gateway_client_message(&format!(r#"{{"type":"terminal_attach","payload":{{"agentId":"{id}"}}}}"#)),
            Some(ClientMessage::TerminalAttach { agent_id, cols: None, rows: None })
        );
        // A non-u16 cols (string / float / oversize) degrades to None, never fails
        // the whole message.
        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_resize","payload":{{"agentId":"{id}","cols":"80","rows":99999999}}}}"#
            )),
            Some(ClientMessage::TerminalResize { agent_id, cols: None, rows: None })
        );
        // Non-string entries in `keys` are dropped, not fatal.
        assert_eq!(
            parse_gateway_client_message(&format!(
                r#"{{"type":"terminal_input","payload":{{"agentId":"{id}","keys":["a",1,null,"b"]}}}}"#
            )),
            Some(ClientMessage::TerminalInput { agent_id, keys: vec!["a".to_string(), "b".to_string()] })
        );
    }

    #[test]
    fn client_message_is_none_for_unhandled_or_malformed_frames() {
        let id = Uuid::now_v7().to_string();
        // Unknown tag → no-op.
        assert_eq!(parse_gateway_client_message(r#"{"type":"subscribe","payload":{}}"#), None);
        // Missing `type` → no-op.
        assert_eq!(parse_gateway_client_message(r#"{"payload":{"agentId":"x"}}"#), None);
        // Invalid JSON → no-op.
        assert_eq!(parse_gateway_client_message("not json"), None);
        // Known tag but a missing/invalid agentId makes the whole message a no-op.
        assert_eq!(parse_gateway_client_message(r#"{"type":"terminal_detach","payload":{}}"#), None);
        assert_eq!(
            parse_gateway_client_message(&format!(r#"{{"type":"terminal_data","payload":{{"agentId":"{id}"}}}}"#)),
            None,
            "terminal_data without required `data` is a no-op"
        );
    }

    #[test]
    fn terminal_frames_preserve_wire_shape() {
        let agent_id = Uuid::now_v7();
        let output: Value = serde_json::from_str(&terminal_output_frame(agent_id, b"\x1b[?25hhello\r\n")).unwrap();
        assert_eq!(output["type"], "terminal_output");
        assert_eq!(output["payload"]["agentId"], agent_id.to_string());
        assert_eq!(output["payload"]["data"], BASE64.encode(b"\x1b[?25hhello\r\n"));

        let error: Value =
            serde_json::from_str(&terminal_error_frame(agent_id, "terminal input stream is closed")).unwrap();
        assert_eq!(error["type"], "terminal_error");
        assert_eq!(error["payload"]["agentId"], agent_id.to_string());
        assert_eq!(error["payload"]["message"], "terminal input stream is closed");
    }
}
