//! Skill domain rules.
//!
//! This module owns pure skill input, lifecycle, and version policies that are
//! independent of repositories, authorization, and audit emission.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// Validated skill name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkillName<'a> {
    value: &'a str,
}

impl<'a> SkillName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("skill name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Skill TTL policy.
pub(crate) struct SkillTtlPolicy;

impl SkillTtlPolicy {
    pub(crate) fn validate(ttl_expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> AppResult<()> {
        if let Some(ttl) = ttl_expires_at
            && ttl <= now
        {
            return Err(ErrorKind::Validation("ttl_expires_at must be in the future".into()).into());
        }
        Ok(())
    }
}

/// Supported persisted skill sensitivity labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillSensitivity {
    Public,
    Internal,
    Confidential,
    SecretDetected,
}

impl SkillSensitivity {
    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        match value {
            "public" => Ok(Self::Public),
            "internal" => Ok(Self::Internal),
            "confidential" => Ok(Self::Confidential),
            "secret_detected" => Ok(Self::SecretDetected),
            other => Err(ErrorKind::Validation(format!("unsupported skill sensitivity `{other}`")).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::SecretDetected => "secret_detected",
        }
    }
}

/// JSON-object policy for structured skill metadata fields.
pub(crate) struct SkillJsonObjectPolicy;

impl SkillJsonObjectPolicy {
    pub(crate) fn validate(name: &str, value: &Value) -> AppResult<()> {
        if value.as_object().is_some() {
            Ok(())
        } else {
            Err(ErrorKind::Validation(format!("{name} must be a JSON object")).into())
        }
    }
}

/// Restore-version request policy.
pub(crate) struct SkillRestoreVersionPolicy;

impl SkillRestoreVersionPolicy {
    pub(crate) fn validate(version: i32, expected_current_version: Option<i32>) -> AppResult<()> {
        if version < 1 {
            return Err(ErrorKind::Validation("version must be >= 1".into()).into());
        }
        if matches!(expected_current_version, Some(version) if version < 1) {
            return Err(ErrorKind::Validation("expected_current_version must be >= 1".into()).into());
        }
        Ok(())
    }
}

/// Skill enabled/state update result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkillStateChange {
    enabled: Option<bool>,
    state: Option<&'static str>,
}

impl SkillStateChange {
    pub(crate) fn enabled(self) -> Option<bool> {
        self.enabled
    }

    pub(crate) fn state(self) -> Option<&'static str> {
        self.state
    }
}

/// Skill enabled/state transition policy.
pub(crate) struct SkillStateTransitionPolicy;

impl SkillStateTransitionPolicy {
    pub(crate) fn next(current_state: &str, enabled: Option<bool>) -> AppResult<SkillStateChange> {
        let change = match enabled {
            Some(true) if current_state == "candidate" => {
                return Err(ErrorKind::Unprocessable("candidate skill promotion requires approval queue".into()).into());
            }
            Some(true) => SkillStateChange { enabled: Some(true), state: Some("active") },
            Some(false) => SkillStateChange { enabled: Some(false), state: Some("deprecated") },
            None => SkillStateChange { enabled: None, state: None },
        };
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn skill_name_trims_and_checks_bounds() {
        assert_eq!(SkillName::parse(" my-skill ").unwrap().value(), "my-skill");
        assert!(SkillName::parse("").is_err());
        assert!(SkillName::parse("   ").is_err());
        assert!(SkillName::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn skill_ttl_requires_future_expiry_when_present() {
        let now = Utc::now();

        assert!(SkillTtlPolicy::validate(None, now).is_ok());
        assert!(SkillTtlPolicy::validate(Some(now + Duration::seconds(1)), now).is_ok());
        assert!(SkillTtlPolicy::validate(Some(now), now).is_err());
        assert!(SkillTtlPolicy::validate(Some(now - Duration::seconds(1)), now).is_err());
    }

    #[test]
    fn skill_sensitivity_accepts_supported_labels_only() {
        assert_eq!(SkillSensitivity::parse("public").unwrap().as_str(), "public");
        assert_eq!(SkillSensitivity::parse("internal").unwrap().as_str(), "internal");
        assert_eq!(SkillSensitivity::parse("confidential").unwrap().as_str(), "confidential");
        assert_eq!(SkillSensitivity::parse("secret_detected").unwrap().as_str(), "secret_detected");
        assert!(SkillSensitivity::parse("private").is_err());
    }

    #[test]
    fn skill_json_object_policy_rejects_non_objects() {
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!({})).is_ok());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!([])).is_err());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!("value")).is_err());
    }

    #[test]
    fn skill_restore_version_policy_rejects_non_positive_versions() {
        assert!(SkillRestoreVersionPolicy::validate(1, None).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(1)).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(0, None).is_err());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(0)).is_err());
    }

    #[test]
    fn skill_state_transition_maps_enabled_updates() {
        let active = SkillStateTransitionPolicy::next("deprecated", Some(true)).unwrap();
        assert_eq!(active.enabled(), Some(true));
        assert_eq!(active.state(), Some("active"));

        let deprecated = SkillStateTransitionPolicy::next("active", Some(false)).unwrap();
        assert_eq!(deprecated.enabled(), Some(false));
        assert_eq!(deprecated.state(), Some("deprecated"));

        let unchanged = SkillStateTransitionPolicy::next("active", None).unwrap();
        assert_eq!(unchanged.enabled(), None);
        assert_eq!(unchanged.state(), None);
    }

    #[test]
    fn skill_state_transition_rejects_direct_candidate_promotion() {
        assert!(SkillStateTransitionPolicy::next("candidate", Some(true)).is_err());
    }
}
