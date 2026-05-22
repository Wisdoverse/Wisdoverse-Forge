//! License domain rules.
//!
//! This module owns pure license key and validity policies that are independent
//! of repositories and HTTP route DTOs.

use agentforge_core::{AppError, AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn license_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) struct LicenseRepositoryPolicy;

impl LicenseRepositoryPolicy {
    pub(crate) fn license_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("license {id}")).into()
    }

    pub(crate) fn license_key_not_found(license_key: &str) -> AppError {
        ErrorKind::NotFound(format!("license with key '{license_key}'")).into()
    }
}

/// Validated license key input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LicenseKey<'a> {
    value: &'a str,
}

impl<'a> LicenseKey<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(ErrorKind::Validation("license_key must not be empty".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// License lifecycle policy.
pub(crate) struct LicenseValidityPolicy;

impl LicenseValidityPolicy {
    pub(crate) fn is_valid(is_active: bool, valid_until: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        is_active && valid_until.is_none_or(|until| until > now)
    }
}

/// License-key validation projection exposed by the license API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LicenseValidation {
    pub(crate) valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plan_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_agents: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_users: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) valid_until: Option<DateTime<Utc>>,
}

impl LicenseValidation {
    pub(crate) fn known(
        valid: bool,
        plan_name: String,
        max_agents: i32,
        max_users: i32,
        is_active: bool,
        valid_until: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            valid,
            reason: None,
            plan_name: Some(plan_name),
            max_agents: Some(max_agents),
            max_users: Some(max_users),
            is_active: Some(is_active),
            valid_until,
        }
    }

    pub(crate) fn unknown_key() -> Self {
        Self {
            valid: false,
            reason: Some("unknown_key".to_string()),
            plan_name: None,
            max_agents: None,
            max_users: None,
            is_active: None,
            valid_until: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn license_key_trims_and_rejects_empty_values() {
        assert_eq!(LicenseKey::parse(" LIC-ABC-123 ").unwrap().value(), "LIC-ABC-123");
        assert!(LicenseKey::parse("").is_err());
        assert!(LicenseKey::parse("   ").is_err());
    }

    #[test]
    fn license_validity_requires_active_license() {
        let now = Utc::now();

        assert!(!LicenseValidityPolicy::is_valid(false, None, now));
        assert!(!LicenseValidityPolicy::is_valid(false, Some(now + Duration::days(1)), now));
    }

    #[test]
    fn license_validity_accepts_active_unexpired_license() {
        let now = Utc::now();

        assert!(LicenseValidityPolicy::is_valid(true, None, now));
        assert!(LicenseValidityPolicy::is_valid(true, Some(now + Duration::seconds(1)), now));
    }

    #[test]
    fn license_validity_rejects_expired_or_boundary_license() {
        let now = Utc::now();

        assert!(!LicenseValidityPolicy::is_valid(true, Some(now - Duration::seconds(1)), now));
        assert!(!LicenseValidityPolicy::is_valid(true, Some(now), now));
    }

    #[test]
    fn license_repository_policy_owns_lookup_errors() {
        let id = Uuid::new_v4();

        assert!(matches!(
            LicenseRepositoryPolicy::license_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("license {id}")
        ));
        assert!(matches!(
            LicenseRepositoryPolicy::license_key_not_found("LIC-123").kind,
            ErrorKind::NotFound(message) if message == "license with key 'LIC-123'"
        ));
    }
}
