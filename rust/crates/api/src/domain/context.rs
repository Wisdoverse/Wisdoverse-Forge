//! Context candidate and feedback input policies.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{ContextGovernancePolicy, Sensitivity};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub(crate) fn context_candidate_subject(org_id: Uuid, scope_kind: &str, scope_id: Uuid, event: &str) -> String {
    format!("broadcast.{org_id}.scope.{scope_kind}.{scope_id}.context_candidate.{event}")
}

pub(crate) fn redacted_proposal_preview(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return json!({});
    };
    let mut out = serde_json::Map::new();
    for key in ["title", "name", "description", "scope_kind", "visibility"] {
        if let Some(value) = map.get(key)
            && value.is_string()
        {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(content) = map.get("content").and_then(Value::as_str) {
        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        let preview = classification.redacted_preview.unwrap_or_else(|| content.chars().take(160).collect());
        out.insert("content_preview".to_string(), json!(preview));
    }
    Value::Object(out)
}

pub(crate) fn normalize_context_candidate_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub(crate) fn normalize_candidate_state_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("pending") {
        "all" => Ok(None),
        "pending" => Ok(Some("pending")),
        "approved" => Ok(Some("approved")),
        "rejected" => Ok(Some("rejected")),
        "superseded" => Ok(Some("superseded")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate state filter `{other}`")).into()),
    }
}

pub(crate) fn normalize_candidate_kind_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("all") {
        "all" => Ok(None),
        "memory" => Ok(Some("memory")),
        "skill" => Ok(Some("skill")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate kind filter `{other}`")).into()),
    }
}

pub(crate) fn normalize_scope_kind_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("all") {
        "all" => Ok(None),
        "user" => Ok(Some("user")),
        "team" => Ok(Some("team")),
        "project" => Ok(Some("project")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate scope filter `{other}`")).into()),
    }
}

pub(crate) fn ensure_pending_candidate(candidate_id: Uuid, state: &str) -> AppResult<()> {
    if state == "pending" {
        Ok(())
    } else {
        Err(ErrorKind::Conflict(format!("context candidate {candidate_id} is already {state}")).into())
    }
}

pub(crate) fn validate_candidate_content(value: &Value) -> AppResult<()> {
    if value.as_object().is_some() {
        Ok(())
    } else {
        Err(ErrorKind::Validation("proposed_content must be a JSON object".into()).into())
    }
}

pub(crate) fn validate_memory_title(title: &str) -> AppResult<&str> {
    let title = title.trim();
    if title.is_empty() || title.len() > 255 {
        return Err(ErrorKind::Validation("memory title must be 1-255 characters".into()).into());
    }
    Ok(title)
}

pub(crate) fn validate_memory_visibility(visibility: Option<&str>) -> AppResult<&str> {
    match visibility.unwrap_or("shared") {
        "private" => Ok("private"),
        "shared" => Ok("shared"),
        other => Err(ErrorKind::Validation(format!("unsupported memory visibility `{other}`")).into()),
    }
}

pub(crate) fn validate_confidence(confidence: Option<f64>) -> AppResult<()> {
    if let Some(value) = confidence
        && !(0.0..=1.0).contains(&value)
    {
        return Err(ErrorKind::Validation("confidence must be between 0 and 1".into()).into());
    }
    Ok(())
}

pub(crate) fn validate_ttl(ttl_at: Option<DateTime<Utc>>) -> AppResult<()> {
    if let Some(ttl) = ttl_at
        && ttl <= Utc::now()
    {
        return Err(ErrorKind::Validation("ttl_at must be in the future".into()).into());
    }
    Ok(())
}

pub(crate) fn validate_context_sensitivity(value: &str) -> AppResult<&str> {
    match value {
        "public" | "internal" | "confidential" | "secret_detected" => Ok(value),
        other => Err(ErrorKind::Validation(format!("unsupported context sensitivity `{other}`")).into()),
    }
}

pub(crate) fn normalize_reason(reason: Option<String>) -> AppResult<Option<String>> {
    let Some(reason) = reason else {
        return Ok(None);
    };
    let reason = reason.trim().to_string();
    if reason.len() > 500 {
        return Err(ErrorKind::Validation("rejection reason must be at most 500 characters".into()).into());
    }
    Ok((!reason.is_empty()).then_some(reason))
}

pub(crate) fn normalize_feedback_note(note: Option<String>) -> AppResult<Option<String>> {
    let Some(note) = note else {
        return Ok(None);
    };
    let note = note.trim().to_string();
    if note.len() > 4000 {
        return Err(ErrorKind::Validation("feedback note must be at most 4000 characters".into()).into());
    }
    Ok((!note.is_empty()).then_some(note))
}

pub(crate) fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Public => "public",
        Sensitivity::Internal => "internal",
        Sensitivity::Confidential => "confidential",
        Sensitivity::SecretDetected => "secret_detected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn candidate_subject_is_scope_keyed() {
        let org_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let scope_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

        assert_eq!(
            context_candidate_subject(org_id, "team", scope_id, "approved"),
            "broadcast.11111111-1111-4111-8111-111111111111.scope.team.22222222-2222-4222-8222-222222222222.context_candidate.approved"
        );
    }

    #[test]
    fn proposal_preview_redacts_content() {
        let preview = redacted_proposal_preview(&json!({
            "title": "Token",
            "content": "api_key=1234567890abcdef1234567890abcdef"
        }));

        assert_eq!(preview["title"], "Token");
        assert!(!preview["content_preview"].as_str().unwrap().contains("1234567890abcdef1234567890abcdef"));
    }

    #[test]
    fn candidate_filters_validate_allowed_values() {
        assert_eq!(normalize_context_candidate_limit(None), 50);
        assert_eq!(normalize_context_candidate_limit(Some(999)), 200);
        assert_eq!(normalize_candidate_state_filter(Some("all")).unwrap(), None);
        assert_eq!(normalize_candidate_state_filter(Some("pending")).unwrap(), Some("pending"));
        assert!(normalize_candidate_state_filter(Some("unknown")).is_err());
        assert_eq!(normalize_candidate_kind_filter(Some("memory")).unwrap(), Some("memory"));
        assert!(normalize_candidate_kind_filter(Some("other")).is_err());
        assert_eq!(normalize_scope_kind_filter(Some("project")).unwrap(), Some("project"));
        assert!(normalize_scope_kind_filter(Some("org")).is_err());
    }

    #[test]
    fn pending_candidate_policy_preserves_conflict_message() {
        let id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        assert!(ensure_pending_candidate(id, "pending").is_ok());
        let err = ensure_pending_candidate(id, "approved").unwrap_err();
        assert!(format!("{}", err.kind).contains("already approved"));
    }

    #[test]
    fn memory_candidate_fields_are_validated_and_normalized() {
        assert_eq!(validate_memory_title("  hello  ").unwrap(), "hello");
        assert!(validate_memory_title("").is_err());
        assert_eq!(validate_memory_visibility(None).unwrap(), "shared");
        assert_eq!(validate_memory_visibility(Some("private")).unwrap(), "private");
        assert!(validate_memory_visibility(Some("org")).is_err());
        assert!(validate_confidence(Some(0.0)).is_ok());
        assert!(validate_confidence(Some(1.0)).is_ok());
        assert!(validate_confidence(Some(1.1)).is_err());
    }

    #[test]
    fn ttl_and_sensitivity_are_validated() {
        assert!(validate_ttl(Some(Utc::now() + Duration::seconds(60))).is_ok());
        assert!(validate_ttl(Some(Utc::now() - Duration::seconds(60))).is_err());
        assert_eq!(validate_context_sensitivity("internal").unwrap(), "internal");
        assert!(validate_context_sensitivity("private").is_err());
        assert_eq!(sensitivity_label(Sensitivity::SecretDetected), "secret_detected");
    }

    #[test]
    fn reasons_and_notes_are_trimmed_and_bounded() {
        assert_eq!(normalize_reason(Some("  no  ".to_string())).unwrap().as_deref(), Some("no"));
        assert_eq!(normalize_reason(Some("   ".to_string())).unwrap(), None);
        assert!(normalize_reason(Some("x".repeat(501))).is_err());
        assert_eq!(normalize_feedback_note(Some("  useful  ".to_string())).unwrap().as_deref(), Some("useful"));
        assert_eq!(normalize_feedback_note(Some("   ".to_string())).unwrap(), None);
        assert!(normalize_feedback_note(Some("x".repeat(4001))).is_err());
    }

    #[test]
    fn candidate_content_must_be_object() {
        assert!(validate_candidate_content(&json!({"title": "x"})).is_ok());
        assert!(validate_candidate_content(&json!("x")).is_err());
    }
}
