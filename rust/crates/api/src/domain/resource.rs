//! Tenant resource domain rules.
//!
//! This module owns organization, workspace, project, team, group, settings,
//! favorites, and membership policies that are independent of repositories and
//! HTTP route DTOs.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use uuid::Uuid;

const VALID_FAVORITE_TARGET_TYPES: &[&str] = &["agent", "project", "workspace"];

/// Validated pagination request for tenant resource lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceListPage {
    limit: i64,
    offset: i64,
}

impl ResourceListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Shared display name policy for first-class tenant resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceName<'a> {
    value: &'a str,
}

impl<'a> ResourceName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Organization slug policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrganizationSlug<'a> {
    value: &'a str,
}

impl<'a> OrganizationSlug<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if value.len() < 3 || value.len() > 50 {
            return Err(ErrorKind::Validation("slug must be between 3 and 50 characters".into()).into());
        }
        if !value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(ErrorKind::Validation(
                "slug must contain only lowercase alphanumeric characters and hyphens".into(),
            )
            .into());
        }
        if value.starts_with('-') || value.ends_with('-') {
            return Err(ErrorKind::Validation("slug must not start or end with a hyphen".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Project repository URL policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectRepositoryUrl;

impl ProjectRepositoryUrl {
    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        if value.is_empty() {
            return Err(ErrorKind::Validation("repository URL must not be empty".into()).into());
        }
        if !value.starts_with("https://") && !value.starts_with("http://") && !value.starts_with("git@") {
            return Err(
                ErrorKind::Validation("repository URL must start with https://, http://, or git@".into()).into()
            );
        }
        if value.len() > 2048 {
            return Err(ErrorKind::Validation("repository URL must be 2048 characters or less".into()).into());
        }
        Ok(Self)
    }
}

/// Group membership role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupMemberRole {
    Member,
    Admin,
}

impl GroupMemberRole {
    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        match value {
            "member" => Ok(Self::Member),
            "admin" => Ok(Self::Admin),
            _ => Err(ErrorKind::Validation("role must be 'member' or 'admin'".into()).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Admin => "admin",
        }
    }
}

/// Team/project membership role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceMemberRole {
    Owner,
    Admin,
    Maintainer,
    Member,
}

impl ResourceMemberRole {
    pub(crate) fn normalize(role: Option<&str>) -> AppResult<Self> {
        match role.unwrap_or("member").trim().to_ascii_lowercase().as_str() {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "maintainer" | "editor" => Ok(Self::Maintainer),
            "member" | "viewer" => Ok(Self::Member),
            _ => Err(ErrorKind::Validation("role must be owner, admin, maintainer, or member".into()).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Maintainer => "maintainer",
            Self::Member => "member",
        }
    }
}

/// Tenant organization guard for org-scoped resource mutations.
pub(crate) struct ResourceOrganizationPolicy;

impl ResourceOrganizationPolicy {
    pub(crate) fn ensure_current_org(scope: &TenantScope, org_id: Uuid) -> AppResult<()> {
        if scope.org_id().as_uuid() == org_id {
            return Ok(());
        }
        Err(ErrorKind::Forbidden.into())
    }
}

/// Favorite target type policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FavoriteTargetType<'a> {
    value: &'a str,
}

impl<'a> FavoriteTargetType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_FAVORITE_TARGET_TYPES.contains(&value) {
            return Err(ErrorKind::Validation(format!(
                "target_type must be one of: {:?}",
                VALID_FAVORITE_TARGET_TYPES
            ))
            .into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Feature flag name value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FeatureFlagName<'a> {
    value: &'a str,
}

impl<'a> FeatureFlagName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Setting key value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettingKey<'a> {
    value: &'a str,
}

impl<'a> SettingKey<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("key must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_list_page_clamps_bounds() {
        assert_eq!(ResourceListPage::new(0, -10).limit(), 1);
        assert_eq!(ResourceListPage::new(200, 50).limit(), 100);
        assert_eq!(ResourceListPage::new(50, -10).offset(), 0);
        assert_eq!(ResourceListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn resource_name_bounds_match_existing_services() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse(&"a".repeat(255)).is_ok());
        assert!(ResourceName::parse("").is_err());
        assert!(ResourceName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn organization_slug_validation_matches_existing_rules() {
        assert!(OrganizationSlug::parse("abc").is_ok());
        assert!(OrganizationSlug::parse("my-org").is_ok());
        assert!(OrganizationSlug::parse("org-123").is_ok());
        assert!(OrganizationSlug::parse(&"a".repeat(50)).is_ok());
        assert!(OrganizationSlug::parse("ab").is_err());
        assert!(OrganizationSlug::parse(&"a".repeat(51)).is_err());
        assert!(OrganizationSlug::parse("My-Org").is_err());
        assert!(OrganizationSlug::parse("my org").is_err());
        assert!(OrganizationSlug::parse("-my-org").is_err());
        assert!(OrganizationSlug::parse("my-org-").is_err());
        assert!(OrganizationSlug::parse("my_org").is_err());
    }

    #[test]
    fn project_repository_url_accepts_http_https_and_git_ssh() {
        assert!(ProjectRepositoryUrl::parse("https://github.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("http://gitlab.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("git@github.com:org/repo.git").is_ok());
        assert!(ProjectRepositoryUrl::parse("").is_err());
        assert!(ProjectRepositoryUrl::parse("ftp://example.com/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("not-a-url").is_err());
        assert!(ProjectRepositoryUrl::parse(&format!("https://{}", "a".repeat(2048))).is_err());
    }

    #[test]
    fn group_member_role_allows_only_member_and_admin() {
        assert_eq!(GroupMemberRole::parse("member").unwrap().as_str(), "member");
        assert_eq!(GroupMemberRole::parse("admin").unwrap().as_str(), "admin");
        assert!(GroupMemberRole::parse("").is_err());
        assert!(GroupMemberRole::parse("owner").is_err());
    }

    #[test]
    fn resource_member_role_normalizes_legacy_labels() {
        assert_eq!(ResourceMemberRole::normalize(None).unwrap().as_str(), "member");
        assert_eq!(ResourceMemberRole::normalize(Some(" OWNER ")).unwrap().as_str(), "owner");
        assert_eq!(ResourceMemberRole::normalize(Some("editor")).unwrap().as_str(), "maintainer");
        assert_eq!(ResourceMemberRole::normalize(Some("viewer")).unwrap().as_str(), "member");
        assert!(ResourceMemberRole::normalize(Some("root")).is_err());
    }

    #[test]
    fn favorite_target_type_is_case_sensitive() {
        assert_eq!(FavoriteTargetType::parse("agent").unwrap().value(), "agent");
        assert!(FavoriteTargetType::parse("project").is_ok());
        assert!(FavoriteTargetType::parse("workspace").is_ok());
        assert!(FavoriteTargetType::parse("user").is_err());
        assert!(FavoriteTargetType::parse("Agent").is_err());
    }

    #[test]
    fn feature_flag_name_and_setting_key_trim_bounds() {
        assert_eq!(FeatureFlagName::parse(" dark-mode ").unwrap().value(), "dark-mode");
        assert!(FeatureFlagName::parse("").is_err());
        assert!(FeatureFlagName::parse(&"x".repeat(256)).is_err());

        assert_eq!(SettingKey::parse(" theme.color ").unwrap().value(), "theme.color");
        assert!(SettingKey::parse("").is_err());
        assert!(SettingKey::parse(&"x".repeat(256)).is_err());
    }
}
