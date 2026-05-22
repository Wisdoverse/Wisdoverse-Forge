//! NATS auth-callout protocol payloads.

pub(crate) fn nats_kick_payload(client_cid: u64) -> String {
    serde_json::json!({ "cid": client_cid }).to_string()
}

/// Pub/sub allow/deny arrays embedded inside the inner NATS User JWT.
#[derive(Debug, Clone, Default)]
pub struct NatsPermissions {
    pub pub_allow: Vec<String>,
    pub pub_deny: Vec<String>,
    pub sub_allow: Vec<String>,
    pub sub_deny: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct NatsJwtHeader {
    typ: &'static str,
    alg: &'static str,
}

pub(crate) fn nats_jwt_header() -> NatsJwtHeader {
    NatsJwtHeader { typ: "JWT", alg: "ed25519-nkey" }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct UserJwtClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
    jti: &'a str,
    name: &'a str,
    nats: UserJwtNats<'a>,
}

#[derive(Debug, serde::Serialize)]
struct UserJwtNats<'a> {
    #[serde(rename = "pub")]
    pub_permissions: PermissionRules<'a>,
    #[serde(rename = "sub")]
    sub_permissions: PermissionRules<'a>,
    #[serde(rename = "type")]
    kind: &'static str,
    version: u8,
}

#[derive(Debug, serde::Serialize)]
struct PermissionRules<'a> {
    allow: &'a [String],
    deny: &'a [String],
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn user_jwt_claims<'a>(
    issuer_pub: &'a str,
    subject_nkey: &'a str,
    audience_account_name: &'a str,
    name: &'a str,
    iat: u64,
    exp: u64,
    jti: &'a str,
    permissions: &'a NatsPermissions,
) -> UserJwtClaims<'a> {
    UserJwtClaims {
        iss: issuer_pub,
        sub: subject_nkey,
        aud: audience_account_name,
        iat,
        exp,
        jti,
        name,
        nats: UserJwtNats {
            pub_permissions: PermissionRules { allow: &permissions.pub_allow, deny: &permissions.pub_deny },
            sub_permissions: PermissionRules { allow: &permissions.sub_allow, deny: &permissions.sub_deny },
            kind: "user",
            version: 2,
        },
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct AuthorizationResponseClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
    jti: &'a str,
    nats: AuthorizationResponseNats<'a>,
}

#[derive(Debug, serde::Serialize)]
struct AuthorizationResponseNats<'a> {
    jwt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    #[serde(rename = "type")]
    kind: &'static str,
    version: u8,
}

pub(crate) fn authorization_response_claims<'a>(
    issuer_pub: &'a str,
    subject_user_nkey: &'a str,
    audience_server_id: &'a str,
    iat: u64,
    exp: u64,
    jti: &'a str,
    inner_user_jwt: &'a str,
    error: Option<&'a str>,
) -> AuthorizationResponseClaims<'a> {
    AuthorizationResponseClaims {
        iss: issuer_pub,
        sub: subject_user_nkey,
        aud: audience_server_id,
        iat,
        exp,
        jti,
        nats: AuthorizationResponseNats { jwt: inner_user_jwt, error, kind: "authorization_response", version: 2 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nats_kick_payload_owns_sys_request_shape() {
        assert_eq!(nats_kick_payload(42), r#"{"cid":42}"#);
    }

    #[test]
    fn nats_jwt_header_owns_canonical_header_shape() {
        let value = serde_json::to_value(nats_jwt_header()).expect("header serializes");

        assert_eq!(value["typ"], "JWT");
        assert_eq!(value["alg"], "ed25519-nkey");
    }

    #[test]
    fn user_jwt_claims_own_permission_shape() {
        let permissions = NatsPermissions {
            pub_allow: vec!["events.ingest.agent-1".to_string()],
            pub_deny: vec!["$SYS.>".to_string()],
            sub_allow: vec!["sidecar.agent-1.cmd".to_string()],
            sub_deny: vec!["broadcast.>".to_string()],
        };
        let claims = user_jwt_claims("issuer", "user-nkey", "AGENTFORGE", "agent-1", 10, 70, "jti-1", &permissions);
        let value = serde_json::to_value(claims).expect("claims serialize");

        assert_eq!(value["iss"], "issuer");
        assert_eq!(value["aud"], "AGENTFORGE");
        assert_eq!(value["nats"]["type"], "user");
        assert_eq!(value["nats"]["version"], 2);
        assert_eq!(value["nats"]["pub"]["allow"][0], "events.ingest.agent-1");
        assert_eq!(value["nats"]["sub"]["deny"][0], "broadcast.>");
    }

    #[test]
    fn authorization_response_claims_omit_error_on_allow() {
        let claims =
            authorization_response_claims("issuer", "user-nkey", "server-nkey", 10, 40, "jti-2", "inner-jwt", None);
        let value = serde_json::to_value(claims).expect("claims serialize");

        assert_eq!(value["nats"]["jwt"], "inner-jwt");
        assert_eq!(value["nats"]["type"], "authorization_response");
        assert!(value["nats"].get("error").is_none());
    }

    #[test]
    fn authorization_response_claims_include_error_on_deny() {
        let claims = authorization_response_claims(
            "issuer",
            "user-nkey",
            "server-nkey",
            10,
            40,
            "jti-3",
            "",
            Some("unauthorized"),
        );
        let value = serde_json::to_value(claims).expect("claims serialize");

        assert_eq!(value["nats"]["error"], "unauthorized");
    }
}
