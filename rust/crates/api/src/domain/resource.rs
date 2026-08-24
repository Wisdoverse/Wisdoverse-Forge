//! Tenant resource domain rules.
//!
//! This module owns organization, workspace, project, team, group, settings,
//! favorites, and membership policies that are independent of repositories and
//! HTTP route DTOs.

use agentforge_core::{AppError, AppResult, ErrorKind, GroupId, OrgId, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_llm::{ProviderTransport, provider_spec};
use serde::Serialize;
use serde_json::{Value, json};
use std::net::IpAddr;
use url::{Host, Url};
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

/// Response for redeeming a one-time team invite with the current account.
pub(crate) fn redeem_team_invite_response(org_id: Uuid, team_id: Uuid) -> Value {
    json!({ "ok": true, "orgId": org_id, "teamId": team_id })
}

/// Response for an invite that sent an email link (pending acceptance).
pub(crate) fn invite_pending_response(invite_url: &str) -> Value {
    json!({ "ok": true, "pending": true, "inviteUrl": invite_url })
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

/// Project repository URL policy (parse-don't-validate).
///
/// v1 clone only supports HTTPS token auth (GitHub/GitLab), so the URL must be
/// `https://` with a non-empty host. The parsed, validated host is RETAINED on
/// the value object so M6 can do credential-host-matching without re-parsing.
///
/// `parse` is best-effort defense-in-depth, NOT the primary SSRF control: the
/// clone container's restricted egress network (design spec §10, M4) is the real
/// control. What `parse` adds here:
///   - https-only + length cap (rejects `http`/`git`/`ssh`/`file`/scheme-less).
///   - No control chars / whitespace ANYWHERE in the URL — defeats CRLF / NUL /
///     space injection into a future `git clone <url>` argv (H2).
///   - REJECTS any embedded `userinfo@` in the authority (`https://user@host`,
///     `https://user:token@host`). This is the CRITICAL credential-leak gate:
///     the stored `repository_url` becomes the `CLONE_URL` the M5 worker passes
///     verbatim to `git clone "$CLONE_URL"`, so any `user:token@` embedded in it
///     would leak the token into git's argv, the container env (`docker
///     inspect`), `/proc/<pid>/cmdline`, and git's stderr — bypassing the entire
///     mount-based one-shot-helper credential design. Credentials must come ONLY
///     from the mounted secret, never the URL.
///   - A literal-IP / name deny-list for the host: loopback, link-local, cloud
///     metadata (`169.254.*`), RFC1918 private ranges, and `.local` (H1, best
///     effort — M4 egress filtering is the authoritative block).
///   - Non-empty, whitespace-free host label after isolating the authority and
///     stripping `:port` (rejects `https://:8080/r`) (H3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRepositoryUrl {
    host: String,
}

impl ProjectRepositoryUrl {
    const HTTPS_PREFIX: &'static str = "https://";

    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        if value.is_empty() {
            return Err(ErrorKind::Validation("repository URL must not be empty".into()).into());
        }
        if value.len() > 2048 {
            return Err(ErrorKind::Validation("repository URL must be 2048 characters or less".into()).into());
        }
        // Reject control chars and whitespace anywhere: a `\r`, `\n`, `\0`, or
        // space smuggled into the URL could split a future `git clone <url>`
        // command or inject a second argument (H2 — injection defense-in-depth).
        if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ErrorKind::Validation(
                "repository URL must not contain whitespace or control characters".into(),
            )
            .into());
        }
        let rest = value
            .strip_prefix(Self::HTTPS_PREFIX)
            .ok_or_else(|| AppError::from(ErrorKind::Validation("repository URL must start with https://".into())))?;
        // The authority is everything up to the first '/', '?', or '#'.
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // CRITICAL credential-leak gate: reject ANY embedded userinfo in the
        // authority. A `user@` or `user:token@` here would be stored verbatim and
        // handed to `git clone "$CLONE_URL"` by the M5 worker, leaking the token
        // into argv / `docker inspect` env / `/proc/<pid>/cmdline` / git stderr.
        // Credentials must come ONLY from the mounted secret, so no `@` is ever
        // permitted in the authority (this also makes the host extraction below
        // unambiguous — `host_port` is the whole authority, no userinfo to strip).
        if authority.contains('@') {
            return Err(ErrorKind::Validation(
                "repository URL must not contain embedded credentials; configure credentials in the Credentials panel"
                    .into(),
            )
            .into());
        }
        // Reject a hostless authority up front with a dedicated message: empty
        // (`https://`, `https:///repo` — the WHATWG parser would otherwise take a
        // triple-slash path segment as the host) or port-only (`https://:8080/r`,
        // whose host label before `:port` is empty). An IPv6 literal keeps its
        // brackets, so the url parser validates it.
        let host_label = if authority.starts_with('[') { authority } else { authority.split(':').next().unwrap_or("") };
        if host_label.is_empty() {
            return Err(ErrorKind::Validation("repository URL must include a host".into()).into());
        }
        // Classify the host with a spec-compliant URL parser rather than ad-hoc
        // string slicing. `url` percent-decodes the host, applies WHATWG IPv4
        // (inet_aton) normalization, and parses IPv6 literals — so encodings a
        // libc resolver would still route to an internal target cannot evade the
        // deny-list (percent-encoded `localhost`, decimal/hex/octal IPv4 such as
        // `2130706433` / `0x7f000001` / `0177.0.0.1`), and a genuine hostname is
        // never misclassified as a numeric literal. IP hosts are range-checked;
        // domains are rejected only for `localhost` / `.local`.
        let parsed = Url::parse(value)
            .map_err(|_| AppError::from(ErrorKind::Validation("repository URL is not a valid URL".into())))?;
        let blocked = match parsed.host() {
            None => true,
            Some(Host::Ipv4(v4)) => is_blocked_ip(IpAddr::V4(v4)),
            Some(Host::Ipv6(v6)) => is_blocked_ip(IpAddr::V6(v6)),
            Some(Host::Domain(domain)) => {
                let domain = domain.to_ascii_lowercase();
                domain.is_empty() || domain == "localhost" || domain.ends_with(".local")
            }
        };
        if blocked {
            return Err(ErrorKind::Validation(
                "repository host is not allowed (private, loopback, or metadata address)".into(),
            )
            .into());
        }
        // Store the parser's normalized (decoded, lowercased, IDNA) host for M6
        // credential-host-matching. `host_str` is present because `host()` was.
        Ok(Self { host: parsed.host_str().unwrap_or_default().to_string() })
    }

    /// The validated host authority label (no userinfo, no port), as written in
    /// the URL. M6 uses this for credential-host-matching (which must compare
    /// case-insensitively). Wired in M6; retained now so the host is not
    /// discarded and re-parsed later.
    #[allow(dead_code)] // consumed by M6 credential-host-matching
    pub(crate) fn host(&self) -> &str {
        &self.host
    }
}

/// SSRF guard for an operator-supplied outbound HTTPS endpoint (e.g. an LLM
/// provider base URL used for live model discovery). Reuses the clone-URL
/// validator wholesale — https-only, no embedded credentials, no control
/// characters, and a private/loopback/metadata host deny-list — so there is a
/// single hardened guard for all operator-supplied egress targets. Returns
/// `true` only when the URL is well-formed and the host is public.
pub(crate) fn is_outbound_https_host_allowed(base_url: &str) -> bool {
    ProjectRepositoryUrl::parse(base_url).is_ok()
}

/// SSRF policy for an operator-supplied provider `base_url`, enforced at the
/// persistence, *inference*, and *connection-test* surfaces so a private/
/// metadata host can neither be stored nor reached.
///
/// The guard targets **hosted-SaaS** providers, where the only legitimate
/// `base_url` override is a public coding-plan vendor and a private/loopback/
/// metadata override is therefore an SSRF attempt. This is determined per
/// provider, NOT per transport: the `OpenAiCompatible` transport is shared by
/// both hosted vendors that ship a public default (groq, deepseek, zhipu,
/// openrouter, …) and genuine bring-your-own endpoints, so exempting the whole
/// transport would bypass the guard for that large hosted set.
///
/// A provider is **bring-your-own-endpoint** (exempt) iff it is Ollama (local,
/// keyless) or its spec default base URL is absent (the generic
/// `openai_compatible` provider, which requires operator-supplied infra) or
/// itself non-public (a self-hosted litellm at `http://litellm:4000`). For BYO
/// providers a private host is a supported deployment, so the network-egress
/// allowlist (see F022) is the authoritative control, not this literal check.
///
/// Returns `true` (allowed) when the override is absent/blank (the factory then
/// uses the vetted compile-time spec default), the provider is bring-your-own,
/// or the override is a well-formed public HTTPS URL. Returns `false` for a
/// private/loopback/metadata/link-local host or any non-HTTPS / credential-
/// bearing URL on a hosted-SaaS provider. An unknown provider is treated
/// strictly (public HTTPS required).
pub(crate) fn provider_base_url_allowed(provider_key: &str, base_url: Option<&str>) -> bool {
    let Some(base) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let Some(spec) = provider_spec(provider_key) else {
        return is_outbound_https_host_allowed(base);
    };
    let bring_your_own_endpoint = match spec.transport {
        // Local, keyless runtime.
        ProviderTransport::Ollama => true,
        // Shared transport: exempt only when the default is absent (generic
        // openai_compatible) or non-public (self-hosted litellm). Hosted vendors
        // ship a public default and stay guarded.
        ProviderTransport::OpenAiCompatible => {
            spec.default_base_url.map(|default| !is_outbound_https_host_allowed(default)).unwrap_or(true)
        }
        // Name-brand hosted SaaS (default is the adapter's hardcoded public host).
        ProviderTransport::Anthropic | ProviderTransport::OpenAi | ProviderTransport::Gemini => false,
    };
    bring_your_own_endpoint || is_outbound_https_host_allowed(base)
}

/// `provider_base_url_allowed` as a fail-closed guard returning a typed
/// `Validation` error, for call sites that propagate `AppResult`.
pub(crate) fn ensure_provider_base_url_allowed(provider_key: &str, base_url: Option<&str>) -> AppResult<()> {
    if provider_base_url_allowed(provider_key, base_url) {
        Ok(())
    } else {
        Err(ErrorKind::Validation(
            "provider base URL host is not allowed (private, loopback, or metadata address)".into(),
        )
        .into())
    }
}

/// Reject loopback / private / link-local / ULA / metadata / unspecified /
/// multicast IPs in either family. The 169.254.0.0/16 metadata endpoint is
/// covered by IPv4 `is_link_local`. Hosts are parsed by `url::Host` (WHATWG),
/// so non-canonical IPv4 forms (decimal/hex/octal) are normalized before reaching
/// here; this remains best-effort SSRF defense-in-depth (it does NOT resolve DNS —
/// a hostname resolving to a private IP is the network-egress layer's job).
fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return true;
            }
            // IPv4-mapped (`::ffff:a.b.c.d`): classify the embedded IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(v4));
            }
            let seg0 = v6.segments()[0];
            // ULA `fc00::/7` (fc00–fdff) and link-local `fe80::/10` (fe80–febf).
            (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
        }
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

/// One-time team invite policy: token format, hashing, expiry, and redeem
/// rules. The invite only grants the invitee's OWN email address — a
/// different account can never consume someone else's invite.
pub(crate) struct TeamInvitePolicy;

impl TeamInvitePolicy {
    pub(crate) const TTL_HOURS: i64 = 72;

    pub(crate) fn generate_token() -> String {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use rand::Rng;
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    pub(crate) fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(token.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub(crate) fn invalid_or_expired() -> ErrorKind {
        ErrorKind::Validation(
            "This invite has expired or was already used. Ask the team lead to invite you again.".into(),
        )
    }

    pub(crate) fn email_mismatch() -> ErrorKind {
        ErrorKind::Validation(
            "This invite is for a different email address. Sign in with the invited email to join.".into(),
        )
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

    /// The create transaction could not allocate a unique `workspace_dir_name`
    /// for the project after the bounded suffix retries (the workspace has an
    /// extraordinary number of same-named projects). A `Conflict` so the client
    /// is told to pick a different name rather than a 500.
    pub(crate) fn workspace_dir_allocation_exhausted() -> AppError {
        ErrorKind::Conflict(
            "could not allocate a unique workspace directory for the project; choose a different name".into(),
        )
        .into()
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

/// Validated legacy-navigation project-create input.
///
/// Note: a caller-supplied `slug` is intentionally NOT carried here. The on-disk
/// identity (`workspace_dir_name`, mirrored to the `slug` column) is derived by
/// the filesystem-safe policy inside the create transaction, so a raw caller
/// slug can never become a directory name. Only the validated name + optional
/// presentation fields survive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationProjectCreateDraft {
    pub(crate) name: String,
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
        // `slug` is still ACCEPTED for backward request compatibility but is
        // discarded: `create_resource_draft` validates the name, and the on-disk
        // identity is derived in the create transaction, not from a caller slug.
        let draft = create_resource_draft(name, slug, "project name is required")?;
        Ok(NavigationProjectCreateDraft { name: draft.name, color, description })
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
        // Accepted: https with a real host (with or without a path), no userinfo.
        assert!(ProjectRepositoryUrl::parse("https://github.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://gitlab.com/org/repo.git").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://host/repo").is_ok());
        // Accepted: an explicit port on a public host.
        assert!(ProjectRepositoryUrl::parse("https://example.com:8443/org/repo").is_ok());

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
    fn project_repository_url_rejects_embedded_credentials() {
        // CRITICAL: any embedded userinfo must be rejected so a token can never be
        // stored in repository_url and leaked into a future `git clone "$CLONE_URL"`
        // argv / env / /proc / stderr. A plain `user@host` (no password) is also
        // rejected — credentials belong in the mounted secret, never the URL.
        assert!(ProjectRepositoryUrl::parse("https://user@host/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("https://user:tok@github.com/o/r").is_err());
        assert!(ProjectRepositoryUrl::parse("https://x-access-token:ghp_abc123@github.com/o/r.git").is_err());
        assert!(ProjectRepositoryUrl::parse("https://oauth2:glpat-xyz@gitlab.com/o/r.git").is_err());
        // The userinfo gate keys off `@` in the authority only — an `@` later in
        // the path (after the first '/') is NOT userinfo and must NOT be rejected.
        assert!(ProjectRepositoryUrl::parse("https://github.com/o/r@v1.0").is_ok());

        // Regression: the plain, credential-free host still parses and the host is
        // extracted correctly now that no userinfo-strip is involved.
        assert_eq!(ProjectRepositoryUrl::parse("https://github.com/o/r").unwrap().host(), "github.com");
    }

    #[test]
    fn project_repository_url_carries_parsed_host() {
        // S5 / parse-don't-validate: the validated host is retained. (No userinfo
        // form here — those are rejected; see rejects_embedded_credentials.)
        assert_eq!(ProjectRepositoryUrl::parse("https://github.com/o/r").unwrap().host(), "github.com");
        assert_eq!(ProjectRepositoryUrl::parse("https://gitlab.com/o/r").unwrap().host(), "gitlab.com");
        assert_eq!(ProjectRepositoryUrl::parse("https://example.com:8443/o/r").unwrap().host(), "example.com");
    }

    #[test]
    fn project_repository_url_rejects_injection_chars() {
        // H2: whitespace / control chars anywhere would let a smuggled `\n`/space
        // split a future `git clone <url>` invocation.
        assert!(ProjectRepositoryUrl::parse("https://github.com/o/r\n--upload-pack=evil").is_err());
        assert!(ProjectRepositoryUrl::parse("https://github.com/o/r evil").is_err());
        assert!(ProjectRepositoryUrl::parse("https://github.com/o/r\t").is_err());
        assert!(ProjectRepositoryUrl::parse("https://github.com/o/r\0").is_err());
        assert!(ProjectRepositoryUrl::parse("https://git hub.com/o/r").is_err());
    }

    #[test]
    fn project_repository_url_rejects_empty_or_port_only_authority() {
        // H3: a port-only authority has no real host. It must be rejected by the
        // dedicated empty-host branch (its own message), NOT the SSRF deny-list —
        // `project_clone_security::port_only_authority_is_rejected_by_empty_host_branch`
        // asserts the same contract at the service layer.
        for hostless in ["https://:8080/r", "https://", "https:///repo"] {
            let err = ProjectRepositoryUrl::parse(hostless).unwrap_err();
            let ErrorKind::Validation(message) = &err.kind else {
                panic!("{hostless} must be a Validation error, got {err:?}");
            };
            assert!(message.contains("must include a host"), "{hostless} -> {message:?}");
            assert!(!message.contains("private, loopback, or metadata"), "{hostless} -> {message:?}");
        }
        // Any `@` in the authority is now rejected as embedded credentials BEFORE
        // the empty-host check — so these all fail on the userinfo gate regardless
        // of whether userinfo/host happen to be empty.
        assert!(ProjectRepositoryUrl::parse("https://@host/r").is_err()); // empty userinfo still has `@`
        assert!(ProjectRepositoryUrl::parse("https://user@/r").is_err()); // userinfo, empty host
        assert!(ProjectRepositoryUrl::parse("https://@/r").is_err()); // both empty
    }

    #[test]
    fn project_repository_url_blocks_ssrf_literal_hosts() {
        // H1: loopback, link-local + metadata, RFC1918, and `.local` literals.
        // Best-effort defense-in-depth (M4 egress is the real control).
        let blocked = [
            "https://localhost/r",
            "https://127.0.0.1/r",
            "https://127.1.2.3/r",
            "https://[::1]/r",
            "https://169.254.169.254/latest/meta-data/", // cloud metadata
            "https://10.0.0.5/r",
            "https://192.168.1.1/r",
            "https://172.16.0.1/r",
            "https://172.31.255.255/r",
            "https://internal.local/r",
            // IPv6 loopback / unspecified / ULA / link-local + IPv4-mapped IPv6.
            "https://[::]/r",                     // unspecified
            "https://[fd00::1]/r",                // ULA (fc00::/7)
            "https://[fc00::abcd]/r",             // ULA
            "https://[fe80::1]/r",                // link-local
            "https://[::ffff:10.0.0.1]/r",        // IPv4-mapped RFC1918
            "https://[::ffff:169.254.169.254]/r", // IPv4-mapped metadata
            "https://[fe80::1%25eth0]/r",         // scoped (zone-id) link-local
            "https://0.0.0.0/r",                  // unspecified IPv4
            // inet_aton-style numeric evasions that resolve to 127.0.0.1.
            "https://2130706433/r", // decimal
            "https://0x7f000001/r", // hex
            "https://0177.0.0.1/r", // octal first octet
            "https://127.1/r",      // partial dotted (→127.0.0.1)
            // Percent-encoded hosts that decode to a private/loopback/metadata
            // target (the URL parser decodes before classification).
            "https://%6c%6f%63%61%6c%68%6f%73%74/r", // -> localhost
            "https://%31%36%39.254.169.254/r",       // -> 169.254.169.254
        ];
        for url in blocked {
            assert!(ProjectRepositoryUrl::parse(url).is_err(), "{url} should be blocked");
        }
        // Public-looking 172.x addresses OUTSIDE 172.16-31 are not blocked here.
        assert!(ProjectRepositoryUrl::parse("https://172.15.0.1/r").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://172.32.0.1/r").is_ok());
        // Public IPv6 literals and normal hostnames remain allowed — including
        // hostnames whose labels happen to be all hex digits with a hex TLD
        // (e.g. `.de`), which must NOT be mistaken for a numeric IP literal.
        assert!(ProjectRepositoryUrl::parse("https://[2606:4700::1111]/r").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://api.groq.com/r").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://deadbeef.example.com/r").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://abc123.de/r").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://cafe.be/r").is_ok());
    }

    #[test]
    fn provider_base_url_ssrf_guard_blocks_hosted_saas_private_hosts() {
        // A missing or blank operator override is allowed: the factory falls back
        // to the vetted compile-time spec default (api.anthropic.com, …).
        assert!(provider_base_url_allowed("anthropic", None));
        assert!(provider_base_url_allowed("anthropic", Some("   ")));

        // An operator-supplied public HTTPS host is allowed.
        assert!(provider_base_url_allowed("anthropic", Some("https://api.anthropic.com")));

        // Hosted-SaaS transports: an operator-supplied private / loopback /
        // metadata host (or non-HTTPS) is an SSRF attempt and is blocked.
        for blocked in [
            "https://127.0.0.1/v1",
            "https://localhost/v1",
            "https://169.254.169.254/latest/meta-data/",
            "https://10.0.0.5/v1",
            "https://192.168.1.10/v1",
            "https://[fd00::1]/v1",          // IPv6 ULA
            "https://[::ffff:127.0.0.1]/v1", // IPv4-mapped loopback
            "https://2130706433/v1",         // inet_aton decimal -> 127.0.0.1
            "http://api.anthropic.com",
        ] {
            assert!(!provider_base_url_allowed("anthropic", Some(blocked)), "{blocked} must be blocked");
        }

        // Bring-your-own-endpoint providers are exempt: a self-hosted Ollama or
        // OpenAI-compatible (litellm/vLLM) endpoint on a private host is a
        // supported deployment; the network-egress allowlist is the control there.
        assert!(provider_base_url_allowed("ollama", Some("http://127.0.0.1:11434")));
        assert!(provider_base_url_allowed("litellm", Some("http://litellm:4000")));
        assert!(provider_base_url_allowed("openai_compatible", Some("http://vllm:8000/v1")));

        // Hosted vendors that SHARE the OpenAI-compatible transport but ship a
        // PUBLIC default endpoint stay guarded — a private override is an SSRF
        // attempt, not a self-hosted deployment.
        for hosted in ["groq", "deepseek", "zhipu", "openrouter"] {
            assert!(
                !provider_base_url_allowed(hosted, Some("https://169.254.169.254/v1")),
                "{hosted} private override must be blocked"
            );
            assert!(
                provider_base_url_allowed(hosted, Some("https://api.example.com/v1")),
                "{hosted} public override must be allowed"
            );
        }

        // Unknown provider is treated strictly (public HTTPS required).
        assert!(!provider_base_url_allowed("not_a_real_provider", Some("http://10.0.0.5/v1")));

        // The ensure_* wrapper surfaces a typed Validation error for blocked hosts
        // and Ok for allowed ones.
        assert!(matches!(
            ensure_provider_base_url_allowed("anthropic", Some("https://127.0.0.1/v1")).unwrap_err().kind,
            ErrorKind::Validation(_)
        ));
        assert!(ensure_provider_base_url_allowed("anthropic", Some("https://api.anthropic.com")).is_ok());
        assert!(ensure_provider_base_url_allowed("anthropic", None).is_ok());
        assert!(ensure_provider_base_url_allowed("openai_compatible", Some("http://vllm:8000/v1")).is_ok());
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
        // A caller-supplied slug is accepted by the request but intentionally
        // discarded — the dir name is derived in the create transaction.
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
