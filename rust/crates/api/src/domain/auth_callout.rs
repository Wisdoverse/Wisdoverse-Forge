//! NATS auth-callout protocol payloads.

pub(crate) fn nats_kick_payload(client_cid: u64) -> String {
    serde_json::json!({ "cid": client_cid }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nats_kick_payload_owns_sys_request_shape() {
        assert_eq!(nats_kick_payload(42), r#"{"cid":42}"#);
    }
}
