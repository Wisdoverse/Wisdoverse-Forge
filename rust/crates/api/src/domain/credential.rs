//! Credential domain rules.
//!
//! This module owns API key, SSH key, and Container CLI credential policies
//! that are independent of repositories, encryption, HTTP handlers, and
//! filesystem mount materialization.

use agentforge_core::{AppResult, CliToolKind, ErrorKind};
use url::Url;

const API_KEY_PREFIX: &str = "af_";
const VALID_API_KEY_SCOPES: &[&str] = &["read", "write", "admin"];
const VALID_SSH_KEY_PREFIXES: &[&str] =
    &["ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521"];
const VALID_GIT_PROVIDERS: &[&str] = &["github", "gitlab", "bitbucket", "custom"];
const VALID_GIT_CREDENTIAL_TYPES: &[&str] = &["token", "ssh", "oauth"];

/// Validated pagination request for credential lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialListPage {
    limit: i64,
    offset: i64,
}

impl CredentialListPage {
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

/// API key display name value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApiKeyName<'a> {
    value: &'a str,
}

impl<'a> ApiKeyName<'a> {
    pub(crate) fn parse(name: &'a str) -> AppResult<Self> {
        let value = name.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// API key plaintext format policy.
pub(crate) struct ApiKeyFormat;

impl ApiKeyFormat {
    pub(crate) const PREFIX: &'static str = API_KEY_PREFIX;

    pub(crate) fn validate(key: &str) -> AppResult<()> {
        if !key.starts_with(API_KEY_PREFIX) {
            return Err(ErrorKind::Validation("key must start with 'af_'".into()).into());
        }
        if key.len() != 67 {
            return Err(ErrorKind::Validation("key must be exactly 67 characters".into()).into());
        }
        if hex::decode(&key[API_KEY_PREFIX.len()..]).is_err() {
            return Err(ErrorKind::Validation("key must contain valid hex characters after prefix".into()).into());
        }
        Ok(())
    }
}

/// API key scope policy.
pub(crate) struct ApiKeyScopePolicy;

impl ApiKeyScopePolicy {
    pub(crate) fn validate(scopes: &[String]) -> AppResult<()> {
        for scope in scopes {
            if !VALID_API_KEY_SCOPES.contains(&scope.as_str()) {
                return Err(ErrorKind::Validation(format!(
                    "invalid scope '{}', valid: {:?}",
                    scope, VALID_API_KEY_SCOPES
                ))
                .into());
            }
        }
        Ok(())
    }
}

/// SSH key display name value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SshKeyName<'a> {
    value: &'a str,
}

impl<'a> SshKeyName<'a> {
    pub(crate) fn parse(name: &'a str) -> AppResult<Self> {
        let value = name.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Supported SSH key kinds persisted with each key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SshKeyKind {
    Ed25519,
    Rsa,
    Ecdsa,
}

impl SshKeyKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Rsa => "rsa",
            Self::Ecdsa => "ecdsa",
        }
    }
}

/// Validated SSH public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SshPublicKey<'a> {
    value: &'a str,
    kind: SshKeyKind,
}

impl<'a> SshPublicKey<'a> {
    pub(crate) fn parse(public_key: &'a str) -> AppResult<Self> {
        let value = public_key.trim();
        let Some(prefix) = VALID_SSH_KEY_PREFIXES.iter().find(|prefix| value.starts_with(**prefix)) else {
            return Err(ErrorKind::Validation(format!(
                "unsupported SSH key type, expected one of: {:?}",
                VALID_SSH_KEY_PREFIXES
            ))
            .into());
        };
        let kind = if prefix.starts_with("ecdsa") {
            SshKeyKind::Ecdsa
        } else if *prefix == "ssh-rsa" {
            SshKeyKind::Rsa
        } else {
            SshKeyKind::Ed25519
        };
        Ok(Self { value, kind })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }

    pub(crate) fn kind(self) -> SshKeyKind {
        self.kind
    }

    /// Compute a SHA-256 fingerprint of the public key's binary data.
    pub(crate) fn fingerprint(self) -> String {
        use base64::Engine;
        let engine = base64::engine::general_purpose::STANDARD;

        let bytes_to_hash = match self.value.split_whitespace().nth(1) {
            Some(b64_data) => match engine.decode(b64_data) {
                Ok(decoded) => decoded,
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to base64-decode SSH key data, hashing raw string");
                    self.value.as_bytes().to_vec()
                }
            },
            None => {
                tracing::warn!("SSH key missing base64 data field, hashing raw string");
                self.value.as_bytes().to_vec()
            }
        };

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&bytes_to_hash);
        let digest = hasher.finalize();
        format!("SHA256:{}", engine.encode(digest))
    }
}

/// Container CLI credential policy.
pub(crate) struct ContainerCliCredentialPolicy;

impl ContainerCliCredentialPolicy {
    pub(crate) fn canonical_tool(raw: &str) -> AppResult<&'static str> {
        CliToolKind::parse_legacy(raw)
            .map(CliToolKind::as_str)
            .map_err(|err| ErrorKind::Validation(err.to_string()).into())
    }

    pub(crate) fn provider_for_tool(cli_tool: &str) -> &'static str {
        match CliToolKind::parse_legacy(cli_tool).ok() {
            Some(CliToolKind::Claude | CliToolKind::Opencode) => "anthropic",
            Some(CliToolKind::Codex) => "openai",
            Some(CliToolKind::Gemini) => "google",
            None => "unknown",
        }
    }

    pub(crate) fn api_key_env_for_tool(cli_tool: &str) -> Option<&'static str> {
        match CliToolKind::parse_legacy(cli_tool).ok()? {
            CliToolKind::Claude | CliToolKind::Opencode => Some("ANTHROPIC_API_KEY"),
            CliToolKind::Codex => Some("OPENAI_API_KEY"),
            CliToolKind::Gemini => Some("GEMINI_API_KEY"),
        }
    }

    pub(crate) fn provider_env(cli_tool: &str) -> Option<(&'static str, &'static str)> {
        Some((Self::provider_for_tool(cli_tool), Self::api_key_env_for_tool(cli_tool)?))
            .filter(|(provider, _)| *provider != "unknown")
    }
}

/// Host-side OAuth mount directory key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OauthMountContainerKey<'a> {
    value: &'a str,
}

impl<'a> OauthMountContainerKey<'a> {
    pub(crate) fn parse(value: &'a str) -> Result<Self, &'static str> {
        if value.is_empty() || value.contains('/') || value.contains('\\') || value == ".." || value == "." {
            return Err("invalid container_key");
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Validated Git credential write request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GitCredentialDraft<'a> {
    name: &'a str,
    provider: &'a str,
    credential_type: &'a str,
}

impl<'a> GitCredentialDraft<'a> {
    pub(crate) fn parse(name: &'a str, provider: &'a str, credential_type: &'a str) -> AppResult<Self> {
        let name = name.trim();
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }

        if !VALID_GIT_PROVIDERS.contains(&provider) {
            return Err(ErrorKind::Validation(format!(
                "invalid provider '{}', valid: {:?}",
                provider, VALID_GIT_PROVIDERS
            ))
            .into());
        }

        if !VALID_GIT_CREDENTIAL_TYPES.contains(&credential_type) {
            return Err(ErrorKind::Validation(format!(
                "invalid credential_type '{}', valid: {:?}",
                credential_type, VALID_GIT_CREDENTIAL_TYPES
            ))
            .into());
        }

        Ok(Self { name, provider, credential_type })
    }

    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn provider(self) -> &'a str {
        self.provider
    }

    pub(crate) fn credential_type(self) -> &'a str {
        self.credential_type
    }
}

/// Git remote host normalization policy for CLI credential injection.
pub(crate) struct GitRemoteHost;

impl GitRemoteHost {
    pub(crate) fn normalize(value: Option<&str>, default_host: &str) -> String {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return default_host.to_string();
        };
        if let Ok(url) = Url::parse(value)
            && let Some(host) = url.host_str()
        {
            return host.to_ascii_lowercase();
        }

        let value = value.trim_start_matches("ssh://").trim_start_matches("git+ssh://").trim_end_matches('/').trim();
        let without_user = value.rsplit_once('@').map_or(value, |(_, host)| host);
        without_user
            .split([':', '/'])
            .next()
            .filter(|host| !host.is_empty())
            .unwrap_or(default_host)
            .to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_page_clamps_limit_and_offset() {
        assert_eq!(CredentialListPage::new(0, -1).limit(), 1);
        assert_eq!(CredentialListPage::new(101, 50).limit(), 100);
        assert_eq!(CredentialListPage::new(20, -1).offset(), 0);
        assert_eq!(CredentialListPage::new(20, 50).offset(), 50);
    }

    #[test]
    fn api_key_name_trims_and_bounds() {
        assert_eq!(ApiKeyName::parse("  valid  ").unwrap().value(), "valid");
        assert!(ApiKeyName::parse("   ").is_err());
        assert!(ApiKeyName::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn api_key_format_validates_prefix_length_and_hex() {
        assert!(ApiKeyFormat::validate(&format!("af_{}", "a".repeat(64))).is_ok());
        assert!(ApiKeyFormat::validate(&format!("bad_{}", "a".repeat(64))).is_err());
        assert!(ApiKeyFormat::validate("af_short").is_err());
        assert!(ApiKeyFormat::validate(&format!("af_{}", "g".repeat(64))).is_err());
    }

    #[test]
    fn api_key_scope_policy_is_case_sensitive() {
        assert!(ApiKeyScopePolicy::validate(&["read".into(), "write".into(), "admin".into()]).is_ok());
        assert!(ApiKeyScopePolicy::validate(&[]).is_ok());
        assert!(ApiKeyScopePolicy::validate(&["READ".into()]).is_err());
        assert!(ApiKeyScopePolicy::validate(&["delete".into()]).is_err());
    }

    #[test]
    fn ssh_public_key_recognizes_supported_kinds() {
        assert_eq!(
            SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host").unwrap().kind(),
            SshKeyKind::Ed25519
        );
        assert_eq!(
            SshPublicKey::parse("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQ user@host").unwrap().kind(),
            SshKeyKind::Rsa
        );
        assert_eq!(
            SshPublicKey::parse("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHA user@host").unwrap().kind(),
            SshKeyKind::Ecdsa
        );
        assert!(SshPublicKey::parse("ssh-dss AAAAB3NzaC1kc3M user@host").is_err());
        assert!(SshPublicKey::parse("").is_err());
    }

    #[test]
    fn ssh_fingerprint_is_stable_and_key_specific() {
        let fp1 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA user1@host").unwrap().fingerprint();
        let fp2 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA user1@host").unwrap().fingerprint();
        let fp3 = SshPublicKey::parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5BBBB user2@host").unwrap().fingerprint();
        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3);
        assert!(fp1.starts_with("SHA256:"));
    }

    #[test]
    fn cli_credential_policy_canonicalizes_and_maps_tools() {
        assert_eq!(ContainerCliCredentialPolicy::canonical_tool(" CLAUDE ").unwrap(), "claude");
        assert!(ContainerCliCredentialPolicy::canonical_tool("vim").is_err());
        assert_eq!(ContainerCliCredentialPolicy::provider_env("claude"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("codex"), Some(("openai", "OPENAI_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("gemini"), Some(("google", "GEMINI_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("opencode"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(ContainerCliCredentialPolicy::provider_env("vim"), None);
    }

    #[test]
    fn oauth_mount_container_key_rejects_path_escape() {
        assert_eq!(OauthMountContainerKey::parse("container-xyz").unwrap().value(), "container-xyz");
        assert!(OauthMountContainerKey::parse("").is_err());
        assert!(OauthMountContainerKey::parse("..").is_err());
        assert!(OauthMountContainerKey::parse(".").is_err());
        assert!(OauthMountContainerKey::parse("a/b").is_err());
        assert!(OauthMountContainerKey::parse("a\\b").is_err());
    }

    #[test]
    fn git_credential_draft_trims_name_and_validates_enums() {
        let draft = GitCredentialDraft::parse("  GitHub  ", "github", "token").unwrap();
        assert_eq!(draft.name(), "GitHub");
        assert_eq!(draft.provider(), "github");
        assert_eq!(draft.credential_type(), "token");

        assert!(GitCredentialDraft::parse("", "github", "token").is_err());
        assert!(GitCredentialDraft::parse(&"a".repeat(256), "github", "token").is_err());
        assert!(GitCredentialDraft::parse("GitHub", "azure", "token").is_err());
        assert!(GitCredentialDraft::parse("GitHub", "github", "password").is_err());
    }

    #[test]
    fn git_credential_draft_accepts_existing_provider_and_type_sets() {
        for provider in ["github", "gitlab", "bitbucket", "custom"] {
            assert!(GitCredentialDraft::parse("name", provider, "token").is_ok());
        }
        for credential_type in ["token", "ssh", "oauth"] {
            assert!(GitCredentialDraft::parse("name", "github", credential_type).is_ok());
        }
    }

    #[test]
    fn git_remote_host_accepts_common_git_url_shapes() {
        assert_eq!(GitRemoteHost::normalize(None, "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("github.com/Wisdoverse/repo"), "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("git@github.com:Wisdoverse/repo.git"), "github.com"), "github.com");
        assert_eq!(GitRemoteHost::normalize(Some("https://GitHub.com/Wisdoverse/repo"), "github.com"), "github.com");
        assert_eq!(
            GitRemoteHost::normalize(Some("git+ssh://git@gitlab.internal.example/group/repo"), "gitlab.com"),
            "gitlab.internal.example"
        );
    }
}
