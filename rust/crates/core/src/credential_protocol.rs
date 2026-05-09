//! Shared credential-sync protocol. Kept in `agentforge_core` so the sidecar
//! (which publishes) and the backend (which consumes) cannot drift.
//!
//! Issue #41: the sidecar watches the CLI's credential directory, packages
//! updated files into `CredentialSyncMessage`, wraps them in the existing
//! `orchestration_protocol::SignedEnvelope`, and publishes to
//! `creds.<agent_id>`. The consumer verifies the envelope against the agent's
//! HMAC secret, resolves the agent's `(org_id, user_id)` from the DB, and
//! upserts the encrypted blob into `user_cli_credentials`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Subject prefix every credential-sync message addresses, without the
/// per-agent suffix. Kept separate so callers can build both single-agent
/// subjects and wildcards without string surgery.
pub const CREDS_SUBJECT_PREFIX: &str = "creds.";

/// Maximum file count in a single `CredentialSyncMessage`. Claude / codex /
/// gemini all ship <=4 files in practice; 10 is the generous hard cap.
pub const MAX_CREDENTIAL_FILES: usize = 10;

/// Maximum byte length per file. Current CLIs ship auth blobs in the low KB
/// range; 64 KiB absorbs future growth with headroom.
pub const MAX_CREDENTIAL_FILE_BYTES: usize = 64 * 1024;

/// Maximum aggregate byte length across all files. Bounded separately so one
/// 64 KiB file does not coexist with nine others at the same limit.
pub const MAX_CREDENTIAL_TOTAL_BYTES: usize = 256 * 1024;

// Compile-time invariants — enforced here so no runtime test is needed.
const _: () = assert!(MAX_CREDENTIAL_FILES > 0);
const _: () = assert!(MAX_CREDENTIAL_FILE_BYTES > 0);
const _: () = assert!(MAX_CREDENTIAL_TOTAL_BYTES >= MAX_CREDENTIAL_FILE_BYTES);

/// Wire shape matching the legacy `shared/types.ts CredentialSyncMessage`.
/// The `agent_id` and `org_id` fields are defensive: the consumer always
/// cross-checks them against the subject + DB lookup, never trusts the
/// payload alone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialSyncMessage {
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
    #[serde(rename = "orgId")]
    pub organization_id: Uuid,
    #[serde(rename = "cliTool")]
    pub cli_tool: String,
    pub files: std::collections::BTreeMap<String, String>,
}

/// Build the NATS subject for publishing a credential sync for `agent_id`.
pub fn creds_subject(agent_id: Uuid) -> String {
    format!("{CREDS_SUBJECT_PREFIX}{agent_id}")
}

/// Build the wildcard subject the backend consumer filters on.
/// Uses NATS `>` wildcard (matches one or more tokens) rather than `*`
/// (single token only) so the stream captures all credential subjects
/// including any future sub-paths, and the consumer filter aligns exactly.
pub fn creds_subject_wildcard() -> String {
    format!("{CREDS_SUBJECT_PREFIX}>")
}

/// Extract `agent_id` from a credential-sync subject. Returns `None` for any
/// subject that does not match `creds.<uuid>` exactly (including
/// multi-segment variants that a stray wildcard subscription would capture).
pub fn parse_agent_id_from_creds_subject(subject: &str) -> Option<Uuid> {
    let rest = subject.strip_prefix(CREDS_SUBJECT_PREFIX)?;
    if rest.contains('.') {
        return None;
    }
    Uuid::parse_str(rest).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_round_trip() {
        let id = Uuid::new_v4();
        let subject = creds_subject(id);
        assert_eq!(parse_agent_id_from_creds_subject(&subject), Some(id));
    }

    #[test]
    fn wildcard_subject_rejects_non_uuid_suffix() {
        assert!(parse_agent_id_from_creds_subject("creds.not-a-uuid").is_none());
        assert!(parse_agent_id_from_creds_subject("creds.").is_none());
    }

    #[test]
    fn multi_segment_subject_is_rejected() {
        let id = Uuid::new_v4();
        let fake = format!("creds.org.{id}");
        assert!(
            parse_agent_id_from_creds_subject(&fake).is_none(),
            "two-segment subject must not parse (would hide the tenant id)"
        );
    }

    #[test]
    fn sync_message_serializes_with_camel_case_keys() {
        let msg = CredentialSyncMessage {
            agent_id: Uuid::nil(),
            organization_id: Uuid::nil(),
            cli_tool: "claude".into(),
            files: [("auth.json".to_string(), "{}".to_string())].into(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("agentId").is_some(), "agent_id must serialize as camelCase");
        assert!(json.get("orgId").is_some(), "organization_id must serialize as orgId");
        assert!(json.get("cliTool").is_some(), "cli_tool must serialize as cliTool");
    }
}
