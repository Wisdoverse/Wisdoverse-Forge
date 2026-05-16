//! License domain rules.
//!
//! This module owns pure license key and validity policies that are independent
//! of repositories and HTTP route DTOs.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};

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
}
