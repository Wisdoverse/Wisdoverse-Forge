//! Governed context audit response projections.
//!
//! Owns the wire shape of `GET /governance/audit` and `POST
//! /governance/audit/export`, plus the pure projection policy that turns a
//! [`GovernanceAuditRow`] into the camelCase response entry the frontend
//! consumes.
//!
//! The HMAC computation and tamper-status check live here because they are
//! deterministic policy, not I/O. The route owns the key derivation
//! (`audit_hmac_key`) since that reads `AppState` and environment.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::domain::context_governance::ContextGovernancePolicy;
use crate::repositories::governance_audit::GovernanceAuditRow;

type HmacSha256 = Hmac<Sha256>;

/// Top-level response body for the governance audit list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditResponse {
    pub entries: Vec<GovernanceAuditEntry>,
    pub query: GovernanceAuditQuery,
}

/// Echoed query summary that surfaces the normalized filter values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditQuery {
    pub event_prefix: String,
    pub limit: i64,
    pub offset: i64,
    pub redacted: bool,
}

/// One row in the audit list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditEntry {
    pub id: Uuid,
    pub event_type: String,
    pub actor_user_id: Option<Uuid>,
    pub item_kind: Option<String>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub raw_item_id: Option<Uuid>,
    pub audit_subject_hash: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Value,
    pub details_redacted: bool,
    pub tamper_status: AuditTamperStatus,
    pub created_at: DateTime<Utc>,
}

/// Tamper-check verdict for an audit row.
///
/// `NotConfigured` is returned when the row predates HMAC signing; `Valid`
/// when the signature matches; `Invalid` when it does not.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditTamperStatus {
    NotConfigured,
    Valid,
    Invalid,
}

/// Pure projection policy for audit rows. The key and the redaction flag
/// are inputs; no I/O happens here.
pub struct GovernanceAuditProjection;

impl GovernanceAuditProjection {
    /// Project a raw audit row into the wire entry, applying HMAC subject
    /// hashing, tamper verification, and optional secret redaction.
    pub fn project_row(row: GovernanceAuditRow, key: &[u8], redact: bool) -> GovernanceAuditEntry {
        let hash_subject = row.subject_item_id.unwrap_or(row.id);
        let scope_kind = row.subject_scope_kind.clone().unwrap_or_else(|| "unknown".to_string());
        let scope_id = row.subject_scope_id.map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_string());
        let audit_subject_hash = hmac_hex(key, &format!("{hash_subject}|{scope_kind}|{scope_id}"));
        let raw_item_id = row.subject_item_id.filter(|_| row.visible_by_scope);
        let tamper_status = Self::tamper_status(&row, key);
        let (details, details_redacted) =
            if redact { ContextGovernancePolicy::redact_audit_details(row.details) } else { (row.details, false) };

        GovernanceAuditEntry {
            id: row.id,
            event_type: row.event_type,
            actor_user_id: row.actor_user_id,
            item_kind: row.item_kind,
            scope_kind: row.subject_scope_kind,
            scope_id: row.subject_scope_id,
            raw_item_id,
            audit_subject_hash,
            resource_type: row.resource_type,
            resource_id: row.resource_id,
            details,
            details_redacted,
            tamper_status,
            created_at: row.created_at,
        }
    }

    fn tamper_status(row: &GovernanceAuditRow, key: &[u8]) -> AuditTamperStatus {
        let Some(signature) = row.details.get("hmac_signature").and_then(Value::as_str) else {
            return AuditTamperStatus::NotConfigured;
        };
        let subject = row.subject_item_id.unwrap_or(row.id);
        let expected = hmac_hex(key, &format!("{}|{}|{}", row.event_type, subject, row.created_at.to_rfc3339()));
        if constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
            AuditTamperStatus::Valid
        } else {
            AuditTamperStatus::Invalid
        }
    }
}

fn hmac_hex(key: &[u8], message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right.iter()).fold(0_u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_row(details: Value) -> GovernanceAuditRow {
        GovernanceAuditRow {
            id: Uuid::nil(),
            event_type: "governance.context.touched".to_string(),
            actor_user_id: None,
            item_kind: None,
            subject_scope_kind: Some("group".to_string()),
            subject_scope_id: None,
            subject_item_id: None,
            resource_type: "context_candidate".to_string(),
            resource_id: None,
            details,
            ip_address: None,
            visible_by_scope: true,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn project_row_emits_subject_hash() {
        let entry = GovernanceAuditProjection::project_row(sample_row(json!({})), b"key", false);
        assert_eq!(entry.audit_subject_hash.len(), 64);
        assert!(entry.audit_subject_hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(entry.tamper_status, AuditTamperStatus::NotConfigured);
    }

    #[test]
    fn redact_flag_controls_details() {
        let entry = GovernanceAuditProjection::project_row(
            sample_row(json!({ "classification": { "token": "github-token-placeholder" } })),
            b"key",
            true,
        );
        assert!(entry.details_redacted);
        assert_eq!(entry.details["classification"]["token"], "[REDACTED]");
    }

    #[test]
    fn tamper_status_validates_matching_signature() {
        let key = b"some-key";
        let id = Uuid::new_v4();
        let created_at = chrono::Utc::now();
        let event_type = "governance.context.touched".to_string();
        let expected = hmac_hex(key, &format!("{}|{}|{}", event_type, id, created_at.to_rfc3339()));
        let row = GovernanceAuditRow {
            id,
            event_type,
            actor_user_id: None,
            item_kind: None,
            subject_scope_kind: None,
            subject_scope_id: None,
            subject_item_id: None,
            resource_type: "context_candidate".to_string(),
            resource_id: None,
            details: json!({ "hmac_signature": expected }),
            ip_address: None,
            visible_by_scope: true,
            created_at,
        };

        let entry = GovernanceAuditProjection::project_row(row, key, false);
        assert_eq!(entry.tamper_status, AuditTamperStatus::Valid);
    }

    #[test]
    fn tamper_status_flags_bad_signature() {
        let row = GovernanceAuditRow {
            id: Uuid::new_v4(),
            event_type: "governance.context.touched".to_string(),
            actor_user_id: None,
            item_kind: None,
            subject_scope_kind: None,
            subject_scope_id: None,
            subject_item_id: None,
            resource_type: "context_candidate".to_string(),
            resource_id: None,
            details: json!({ "hmac_signature": "deadbeef" }),
            ip_address: None,
            visible_by_scope: true,
            created_at: chrono::Utc::now(),
        };
        let entry = GovernanceAuditProjection::project_row(row, b"key", false);
        assert_eq!(entry.tamper_status, AuditTamperStatus::Invalid);
    }
}
