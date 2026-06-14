//! Tenant resource domain rules.
//!
//! This module owns organization, workspace, project, team, group, settings,
//! favorites, and membership policies that are independent of repositories and
//! HTTP route DTOs.

use agentforge_core::{AppError, AppResult, ErrorKind, GroupId, OrgId, ProjectId, TeamId, TenantScope, WorkspaceId};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

const VALID_FAVORITE_TARGET_TYPES: &[&str] = &["agent", "project", "workspace"];

pub(crate) fn resource_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn resource_members_response<T: Serialize>(members: T) -> Value {
    json!({ "ok": true, "members": members })
}

pub(crate) fn resource_member_response<T: Serialize>(member: T) -> Value {
    json!({ "ok": true, "member": member })
}

pub(crate) fn resource_delete_response() -> Value {
    json!({ "ok": true })
}

/// Legacy project-scoped group projection consumed by the tree-pane frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectGroupSummary {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) project_id: Uuid,
}

impl ProjectGroupSummary {
    pub(crate) fn new(id: Uuid, name: String, project_id: Uuid) -> Self {
        Self { id, name, project_id }
    }
}

pub(crate) fn resource_project_groups_response(groups: Vec<ProjectGroupSummary>) -> Value {
    json!({ "ok": true, "data": groups.clone(), "groups": groups })
}

pub(crate) fn resource_group_created_response<T: Serialize>(
    group: T,
    legacy_group: Option<ProjectGroupSummary>,
) -> Value {
    json!({ "ok": true, "data": group, "group": legacy_group })
}

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

/// Default slug derivation for team/project resources.
pub(crate) struct ResourceSlugPolicy;

impl ResourceSlugPolicy {
    pub(crate) fn derive(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        let mut prev_dash = true;

        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }

        if out.ends_with('-') {
            out.pop();
        }

        if out.is_empty() { "untitled".to_string() } else { out }
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
///
/// v1 clone only supports HTTPS token auth (GitHub/GitLab), so the URL must be
/// `https://` with a non-empty host. Rejecting `http`/`git`/`ssh`/`file`/
/// scheme-less here is parse-time defense-in-depth; the clone container's
/// restricted egress network (design spec §10) is the real SSRF control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectRepositoryUrl;

impl ProjectRepositoryUrl {
    const HTTPS_PREFIX: &'static str = "https://";

    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        if value.is_empty() {
            return Err(ErrorKind::Validation("repository URL must not be empty".into()).into());
        }
        if value.len() > 2048 {
            return Err(ErrorKind::Validation("repository URL must be 2048 characters or less".into()).into());
        }
        let rest = value
            .strip_prefix(Self::HTTPS_PREFIX)
            .ok_or_else(|| AppError::from(ErrorKind::Validation("repository URL must start with https://".into())))?;
        // The host is the authority up to the first '/', '?', or '#'. It must be
        // present and non-empty (`https://` and `https:///path` are rejected).
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        // Strip any `userinfo@` prefix so `https://user@host` still has a host.
        let host = host.rsplit('@').next().unwrap_or(host);
        if host.is_empty() {
            return Err(ErrorKind::Validation("repository URL must include a host".into()).into());
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

/// Team/project membership lookup policy.
pub(crate) struct ResourceMemberPolicy;

impl ResourceMemberPolicy {
    pub(crate) fn missing_org_user(email: &str) -> ErrorKind {
        ErrorKind::NotFound(format!("org user {}", email.trim()))
    }
}

pub(crate) struct ResourceRepositoryPolicy;

impl ResourceRepositoryPolicy {
    pub(crate) fn organization_not_found(id: OrgId) -> AppError {
        ErrorKind::NotFound(format!("organization {id}")).into()
    }

    pub(crate) fn organization_uuid_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("organization {id}")).into()
    }

    pub(crate) fn team_not_found(id: TeamId) -> AppError {
        ErrorKind::NotFound(format!("team {id}")).into()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn team_uuid_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("team {id}")).into()
    }

    pub(crate) fn group_not_found(id: GroupId) -> AppError {
        ErrorKind::NotFound(format!("group {id}")).into()
    }

    pub(crate) fn project_not_found(id: ProjectId) -> AppError {
        ErrorKind::NotFound(format!("project {id}")).into()
    }

    pub(crate) fn default_project_team_required() -> AppError {
        ErrorKind::Validation("cannot create project: organization has no teams — create a team first".into()).into()
    }

    pub(crate) fn workspace_not_found(id: WorkspaceId) -> AppError {
        ErrorKind::NotFound(format!("workspace {id}")).into()
    }

    pub(crate) fn resource_profile_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("resource_profile {id}")).into()
    }

    pub(crate) fn favorite_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("favorite {id}")).into()
    }

    pub(crate) fn favorite_already_exists() -> AppError {
        ErrorKind::Validation("favorite already exists".into()).into()
    }

    pub(crate) fn team_or_user_not_found(team_id: TeamId, user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("team {team_id} or user {user_id}")).into()
    }

    pub(crate) fn project_or_user_not_found(project_id: ProjectId, user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("project {project_id} or user {user_id}")).into()
    }

    pub(crate) fn team_member_not_found(user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("team member {user_id}")).into()
    }

    pub(crate) fn project_member_not_found(user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("project member {user_id}")).into()
    }

    pub(crate) fn group_member_not_found(group_id: GroupId, user_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("member {user_id} in group {group_id}")).into()
    }

    pub(crate) fn group_member_already_exists() -> AppError {
        ErrorKind::Conflict("user is already a member of this group".into()).into()
    }
}

/// Tenant organization guard for org-scoped resource mutations.
pub(crate) struct ResourceOrganizationPolicy;

impl ResourceOrganizationPolicy {
    pub(crate) fn ensure_current_org(scope: &TenantScope, org_id: Uuid) -> AppResult<()> {
        if scope.org_id().as_uuid() == org_id {
            return Ok(());
        }
        Err(ErrorKind::Forbidden("forbidden".into()).into())
    }
}

pub(crate) struct ResourcePermissionPolicy;

impl ResourcePermissionPolicy {
    pub(crate) fn ensure_can_manage_org(can_manage: bool) -> AppResult<()> {
        Self::ensure_allowed(can_manage)
    }

    pub(crate) fn ensure_can_manage_team(can_manage: bool) -> AppResult<()> {
        Self::ensure_allowed(can_manage)
    }

    pub(crate) fn ensure_can_create_project(can_create: bool) -> AppResult<()> {
        Self::ensure_allowed(can_create)
    }

    pub(crate) fn ensure_can_manage_project(can_manage: bool) -> AppResult<()> {
        Self::ensure_allowed(can_manage)
    }

    fn ensure_allowed(allowed: bool) -> AppResult<()> {
        if allowed { Ok(()) } else { Err(ErrorKind::Forbidden("forbidden".into()).into()) }
    }
}

/// Frontend navigation create/update policy for team and project drafts.
pub(crate) struct NavigationResourcePolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationTeamCreateDraft {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) visibility: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationTeamUpdateDraft {
    pub(crate) name: Option<String>,
    pub(crate) slug: Option<String>,
    pub(crate) visibility: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationProjectCreateDraft {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) color: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationProjectUpdateDraft {
    pub(crate) name: Option<String>,
    pub(crate) slug: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) description: Option<String>,
}

impl NavigationResourcePolicy {
    pub(crate) fn org_update_name(name: Option<String>) -> AppResult<String> {
        name.ok_or_else(|| ErrorKind::Validation("name is required".into()).into())
    }

    pub(crate) fn team_create_draft(
        name: String,
        slug: Option<String>,
        visibility: Option<String>,
        description: Option<String>,
    ) -> AppResult<NavigationTeamCreateDraft> {
        let draft = create_resource_draft(name, slug, "team name is required")?;
        Ok(NavigationTeamCreateDraft { name: draft.name, slug: draft.slug, visibility, description })
    }

    pub(crate) fn team_update_draft(
        name: Option<String>,
        slug: Option<String>,
        visibility: Option<String>,
        description: Option<String>,
    ) -> AppResult<NavigationTeamUpdateDraft> {
        Ok(NavigationTeamUpdateDraft {
            name: optional_non_empty(name, "team name")?,
            slug: optional_non_empty(slug, "team slug")?,
            visibility: optional_non_empty(visibility, "team visibility")?,
            description: optional_text(description),
        })
    }

    pub(crate) fn project_create_draft(
        name: String,
        slug: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> AppResult<NavigationProjectCreateDraft> {
        let draft = create_resource_draft(name, slug, "project name is required")?;
        Ok(NavigationProjectCreateDraft { name: draft.name, slug: draft.slug, color, description })
    }

    pub(crate) fn project_update_draft(
        name: Option<String>,
        slug: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> AppResult<NavigationProjectUpdateDraft> {
        Ok(NavigationProjectUpdateDraft {
            name: optional_non_empty(name, "project name")?,
            slug: optional_non_empty(slug, "project slug")?,
            color: optional_non_empty(color, "project color")?,
            description: optional_text(description),
        })
    }
}

struct CreateResourceDraft {
    name: String,
    slug: String,
}

fn create_resource_draft(
    name: String,
    slug: Option<String>,
    name_required_message: &str,
) -> AppResult<CreateResourceDraft> {
    let trimmed_name = required_text(&name, name_required_message)?;
    let slug = slug.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| ResourceSlugPolicy::derive(&name));
    Ok(CreateResourceDraft { name: trimmed_name, slug })
}

fn required_text(value: &str, message: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ErrorKind::Validation(message.into()).into());
    }
    Ok(trimmed.to_string())
}

fn optional_non_empty(value: Option<String>, field: &str) -> AppResult<Option<String>> {
    match value {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(ErrorKind::Validation(format!("{field} must not be empty")).into());
            }
            Ok(Some(trimmed.to_string()))
        }
        None => Ok(None),
    }
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
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
    fn resource_slug_policy_derives_default_slugs() {
        assert_eq!(ResourceSlugPolicy::derive("Engineering"), "engineering");
        assert_eq!(ResourceSlugPolicy::derive("My Team"), "my-team");
        assert_eq!(ResourceSlugPolicy::derive("A &&& B"), "a-b");
        assert_eq!(ResourceSlugPolicy::derive("!!foo__bar!!"), "foo-bar");
        assert_eq!(ResourceSlugPolicy::derive("---hi---"), "hi");
        assert_eq!(ResourceSlugPolicy::derive("  spaces  "), "spaces");
        assert_eq!(ResourceSlugPolicy::derive(""), "untitled");
        assert_eq!(ResourceSlugPolicy::derive("   "), "untitled");
    }

    #[test]
    fn resource_slug_policy_drops_unicode_gracefully() {
        assert_eq!(ResourceSlugPolicy::derive("日本語"), "untitled");
        assert_eq!(ResourceSlugPolicy::derive("hello 日本語 world"), "hello-world");
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
    fn project_repository_url_requires_https_with_host() {
        // Accepted: https with a real host (with or without a path / userinfo).
        assert!(ProjectRepositoryUrl::parse("https://github.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://gitlab.com/org/repo.git").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://host/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://user@host/repo").is_ok());

        // Rejected: empty, non-https schemes, scheme-less, and hostless https.
        assert!(ProjectRepositoryUrl::parse("").is_err());
        assert!(ProjectRepositoryUrl::parse("http://h/r").is_err());
        assert!(ProjectRepositoryUrl::parse("git@github.com:org/repo.git").is_err());
        assert!(ProjectRepositoryUrl::parse("ssh://git@host/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("file:///x").is_err());
        assert!(ProjectRepositoryUrl::parse("ftp://example.com/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("not-a-url").is_err());
        assert!(ProjectRepositoryUrl::parse("https://").is_err());
        assert!(ProjectRepositoryUrl::parse("https:///repo").is_err());
        // Length cap still applies (8-char prefix + 2048 host > 2048).
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
    fn resource_permission_policy_owns_permission_denials() {
        assert!(ResourcePermissionPolicy::ensure_can_manage_org(true).is_ok());
        assert!(matches!(
            ResourcePermissionPolicy::ensure_can_manage_org(false).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
        assert!(matches!(
            ResourcePermissionPolicy::ensure_can_manage_team(false).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
        assert!(matches!(
            ResourcePermissionPolicy::ensure_can_create_project(false).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
        assert!(matches!(
            ResourcePermissionPolicy::ensure_can_manage_project(false).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
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
    fn resource_member_policy_owns_email_lookup_error() {
        assert!(
            format!("{}", ResourceMemberPolicy::missing_org_user(" user@example.com "))
                .contains("org user user@example.com")
        );
    }

    #[test]
    fn resource_repository_policy_owns_identity_and_member_error_contracts() {
        let org_id = OrgId::new();
        let raw_org_id = Uuid::new_v4();
        let team_id = TeamId::new();
        let raw_team_id = Uuid::new_v4();
        let group_id = GroupId::new();
        let project_id = ProjectId::new();
        let workspace_id = WorkspaceId::new();
        let user_id = Uuid::new_v4();

        assert!(matches!(
            ResourceRepositoryPolicy::organization_not_found(org_id).kind,
            ErrorKind::NotFound(message) if message == format!("organization {org_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::organization_uuid_not_found(raw_org_id).kind,
            ErrorKind::NotFound(message) if message == format!("organization {raw_org_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::team_not_found(team_id).kind,
            ErrorKind::NotFound(message) if message == format!("team {team_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::team_uuid_not_found(raw_team_id).kind,
            ErrorKind::NotFound(message) if message == format!("team {raw_team_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::group_not_found(group_id).kind,
            ErrorKind::NotFound(message) if message == format!("group {group_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::project_not_found(project_id).kind,
            ErrorKind::NotFound(message) if message == format!("project {project_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::default_project_team_required().kind,
            ErrorKind::Validation(message) if message == "cannot create project: organization has no teams — create a team first"
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::workspace_not_found(workspace_id).kind,
            ErrorKind::NotFound(message) if message == format!("workspace {workspace_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::resource_profile_not_found(user_id).kind,
            ErrorKind::NotFound(message) if message == format!("resource_profile {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::favorite_not_found(user_id).kind,
            ErrorKind::NotFound(message) if message == format!("favorite {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::favorite_already_exists().kind,
            ErrorKind::Validation(message) if message == "favorite already exists"
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::team_or_user_not_found(team_id, user_id).kind,
            ErrorKind::NotFound(message) if message == format!("team {team_id} or user {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::project_or_user_not_found(project_id, user_id).kind,
            ErrorKind::NotFound(message) if message == format!("project {project_id} or user {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::team_member_not_found(user_id).kind,
            ErrorKind::NotFound(message) if message == format!("team member {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::project_member_not_found(user_id).kind,
            ErrorKind::NotFound(message) if message == format!("project member {user_id}")
        ));
        assert!(matches!(
            ResourceRepositoryPolicy::group_member_not_found(group_id, user_id).kind,
            ErrorKind::NotFound(message) if message == format!("member {user_id} in group {group_id}")
        ));
        assert!(matches!(ResourceRepositoryPolicy::group_member_already_exists().kind, ErrorKind::Conflict(_)));
    }

    #[test]
    fn resource_response_helpers_keep_legacy_keys() {
        assert_eq!(resource_data_response(vec![1])["data"], json!([1]));
        assert_eq!(resource_members_response(vec!["alice"])["members"], json!(["alice"]));
        assert_eq!(resource_member_response("alice")["member"], json!("alice"));
        assert_eq!(resource_delete_response()["ok"], true);
    }

    #[test]
    fn project_group_summary_keeps_legacy_alias_shape() {
        let group = ProjectGroupSummary::new(Uuid::nil(), "Backend".into(), Uuid::nil());
        let response = resource_project_groups_response(vec![group.clone()]);

        assert_eq!(response["data"], response["groups"]);
        assert_eq!(response["groups"][0]["projectId"], json!(Uuid::nil()));

        let created = resource_group_created_response("persisted", Some(group));
        assert_eq!(created["data"], "persisted");
        assert_eq!(created["group"]["name"], "Backend");
    }

    #[test]
    fn navigation_team_create_draft_trims_name_and_defaults_slug() {
        let draft = NavigationResourcePolicy::team_create_draft(
            " Engineering Team ".into(),
            None,
            None,
            Some(" ships ".into()),
        )
        .unwrap();

        assert_eq!(draft.name, "Engineering Team");
        assert_eq!(draft.slug, "engineering-team");
        assert_eq!(draft.description.as_deref(), Some(" ships "));
    }

    #[test]
    fn navigation_team_update_draft_trims_patch_fields_and_allows_empty_description() {
        let draft = NavigationResourcePolicy::team_update_draft(
            Some(" Platform ".into()),
            Some(" platform ".into()),
            Some(" private ".into()),
            Some("   ".into()),
        )
        .unwrap();

        assert_eq!(draft.name.as_deref(), Some("Platform"));
        assert_eq!(draft.slug.as_deref(), Some("platform"));
        assert_eq!(draft.visibility.as_deref(), Some("private"));
        assert_eq!(draft.description.as_deref(), Some(""));
    }

    #[test]
    fn navigation_project_create_draft_preserves_create_optional_contract() {
        let draft = NavigationResourcePolicy::project_create_draft(
            " Forge ".into(),
            Some(" custom-slug ".into()),
            Some(" #007AFF ".into()),
            Some(" Control plane ".into()),
        )
        .unwrap();

        assert_eq!(draft.name, "Forge");
        assert_eq!(draft.slug, " custom-slug ");
        assert_eq!(draft.color.as_deref(), Some(" #007AFF "));
        assert_eq!(draft.description.as_deref(), Some(" Control plane "));
    }

    #[test]
    fn navigation_resource_drafts_reject_required_empty_fields() {
        assert!(NavigationResourcePolicy::org_update_name(None).is_err());
        assert_eq!(NavigationResourcePolicy::org_update_name(Some("Acme".into())).unwrap(), "Acme");
        assert!(NavigationResourcePolicy::team_create_draft(" ".into(), None, None, None).is_err());
        assert!(NavigationResourcePolicy::project_create_draft(" ".into(), None, None, None).is_err());
        assert!(NavigationResourcePolicy::team_update_draft(Some(" ".into()), None, None, None).is_err());
        assert!(NavigationResourcePolicy::project_update_draft(None, None, Some(" ".into()), None).is_err());
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
