use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_core::broadcast_protocol::ADMIN_CLI_IMAGE_SUBJECT;
use agentforge_core::{AppError, ErrorKind, TenantScope};

#[derive(Debug, Deserialize)]
pub(crate) struct GatewayClientMessage {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) payload: Value,
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

pub(crate) fn parse_gateway_client_message(text: &str) -> Option<GatewayClientMessage> {
    serde_json::from_str(text).ok()
}

pub(crate) fn terminal_payload_agent_id(payload: &Value) -> Option<Uuid> {
    payload.get("agentId").and_then(Value::as_str).and_then(|id| Uuid::parse_str(id).ok())
}

pub(crate) fn terminal_payload_dimension(payload: &Value, key: &str) -> Option<u16> {
    payload.get(key).and_then(Value::as_u64).and_then(|value| u16::try_from(value).ok())
}

pub(crate) fn terminal_output_frame(agent_id: Uuid, output: &[u8]) -> String {
    json!({
        "type": "terminal_output",
        "payload": {
            "agentId": agent_id,
            "data": BASE64.encode(output),
        }
    })
    .to_string()
}

pub(crate) fn terminal_error_frame(agent_id: Uuid, message: impl Into<String>) -> String {
    json!({
        "type": "terminal_error",
        "payload": {
            "agentId": agent_id,
            "message": message.into(),
        }
    })
    .to_string()
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
    fn terminal_payload_helpers_parse_browser_shape() {
        let agent_id = Uuid::now_v7();
        let payload = json!({ "agentId": agent_id.to_string(), "cols": 120_u16, "rows": 33_u16 });

        assert_eq!(terminal_payload_agent_id(&payload), Some(agent_id));
        assert_eq!(terminal_payload_dimension(&payload, "cols"), Some(120));
        assert_eq!(terminal_payload_dimension(&payload, "rows"), Some(33));
    }

    #[test]
    fn client_message_parser_accepts_terminal_payloads() {
        let agent_id = Uuid::now_v7();
        let message = parse_gateway_client_message(&format!(
            r#"{{"type":"terminal_attach","payload":{{"agentId":"{agent_id}","cols":80}}}}"#
        ))
        .expect("client message");

        assert_eq!(message.kind, "terminal_attach");
        assert_eq!(terminal_payload_agent_id(&message.payload), Some(agent_id));
        assert_eq!(terminal_payload_dimension(&message.payload, "cols"), Some(80));
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
