//! Git credential service - validation, management, and Git platform CLI injection.

use agentforge_core::{AppResult, ErrorKind, TenantScope, crypto};
use agentforge_db::entities::GitCredential;
use url::Url;
use uuid::Uuid;

use crate::repositories::git_credential::GitCredentialRepository;

/// Valid git providers.
const VALID_PROVIDERS: &[&str] = &["github", "gitlab", "bitbucket", "custom"];

/// Valid credential types.
const VALID_CREDENTIAL_TYPES: &[&str] = &["token", "ssh", "oauth"];

/// Business logic layer for git credential operations.
pub struct GitCredentialService {
    repo: GitCredentialRepository,
}

/// Environment variables needed by Git platform CLIs inside an agent container.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GitCliCredentialInjection {
    pub env: Vec<(String, String)>,
}

impl GitCredentialService {
    pub fn new(repo: GitCredentialRepository) -> Self {
        Self { repo }
    }

    /// Create a new git credential after validation.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        provider: &str,
        credential_type: &str,
        remote_url: Option<&str>,
        token_encrypted: Option<&[u8]>,
        token_nonce: Option<&[u8]>,
    ) -> AppResult<GitCredential> {
        let name = validate_git_credential(name, provider, credential_type)?;

        self.repo.create(scope, name, provider, credential_type, remote_url, token_encrypted, token_nonce).await
    }

    /// Upsert a git credential by provider after validation.
    pub async fn upsert_for_provider(
        &self,
        scope: &TenantScope,
        name: &str,
        provider: &str,
        credential_type: &str,
        remote_url: Option<&str>,
        token_encrypted: Option<&[u8]>,
        token_nonce: Option<&[u8]>,
    ) -> AppResult<GitCredential> {
        let name = validate_git_credential(name, provider, credential_type)?;

        self.repo
            .upsert_for_provider(scope, name, provider, credential_type, remote_url, token_encrypted, token_nonce)
            .await
    }

    /// List git credentials (paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<GitCredential>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Resolve saved GitHub/GitLab tokens into env vars consumed by `gh` and `glab`.
    pub async fn resolve_cli_env(
        &self,
        scope: &TenantScope,
        encryption_key: Option<[u8; 32]>,
    ) -> AppResult<GitCliCredentialInjection> {
        let creds = self.repo.latest_cli_tokens(scope).await?;
        if creds.is_empty() {
            return Ok(GitCliCredentialInjection::default());
        }

        let key = encryption_key.ok_or_else(|| {
            ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - cannot decrypt stored git credentials".into())
        })?;
        let mut out = GitCliCredentialInjection::default();

        for cred in creds {
            let Some(token) = decrypt_git_token(&key, &cred)? else {
                continue;
            };
            match cred.provider.as_str() {
                "github" => append_github_cli_env(&mut out.env, cred.remote_url.as_deref(), token),
                "gitlab" => append_gitlab_cli_env(&mut out.env, cred.remote_url.as_deref(), token),
                _ => {}
            }
        }

        Ok(out)
    }

    /// Get a git credential by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<GitCredential> {
        self.repo.find_by_id(scope, id).await
    }

    /// Delete a git credential by ID.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

fn validate_git_credential<'a>(name: &'a str, provider: &str, credential_type: &str) -> AppResult<&'a str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
    }

    if !VALID_PROVIDERS.contains(&provider) {
        return Err(
            ErrorKind::Validation(format!("invalid provider '{}', valid: {:?}", provider, VALID_PROVIDERS)).into()
        );
    }

    if !VALID_CREDENTIAL_TYPES.contains(&credential_type) {
        return Err(ErrorKind::Validation(format!(
            "invalid credential_type '{}', valid: {:?}",
            credential_type, VALID_CREDENTIAL_TYPES
        ))
        .into());
    }

    Ok(name)
}

fn decrypt_git_token(key: &[u8; 32], cred: &GitCredential) -> AppResult<Option<String>> {
    let Some(ciphertext) = cred.token_encrypted.as_deref() else {
        return Ok(None);
    };
    let ciphertext = std::str::from_utf8(ciphertext).map_err(|err| {
        ErrorKind::Validation(format!("stored {} git credential ciphertext is not UTF-8: {err}", cred.provider))
    })?;
    let token = crypto::decrypt_base64(key, ciphertext).map_err(|err| {
        tracing::error!(
            error = %err,
            credential_id = %cred.id,
            provider = %cred.provider,
            "Failed to decrypt stored git credential token"
        );
        ErrorKind::Validation(format!(
            "stored {} git credential cannot be decrypted - reconnect it in Settings",
            cred.provider
        ))
    })?;
    let token = token.trim().to_string();
    Ok((!token.is_empty()).then_some(token))
}

fn append_github_cli_env(env: &mut Vec<(String, String)>, remote_url: Option<&str>, token: String) {
    let host = normalize_git_host(remote_url, "github.com");
    let is_github_dotcom = host == "github.com";
    if !is_github_dotcom {
        env.push(("GH_HOST".into(), host.clone()));
    }
    if host == "github.com" || host.ends_with(".ghe.com") {
        env.push(("GH_TOKEN".into(), token.clone()));
        env.push(("GITHUB_TOKEN".into(), token));
    } else {
        env.push(("GH_ENTERPRISE_TOKEN".into(), token.clone()));
        env.push(("GITHUB_ENTERPRISE_TOKEN".into(), token));
    }
    env.push(("GH_PROMPT_DISABLED".into(), "1".into()));
}

fn append_gitlab_cli_env(env: &mut Vec<(String, String)>, remote_url: Option<&str>, token: String) {
    env.push(("GITLAB_TOKEN".into(), token));
    env.push(("GITLAB_HOST".into(), normalize_git_host(remote_url, "gitlab.com")));
}

fn normalize_git_host(value: Option<&str>, default_host: &str) -> String {
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
    without_user.split([':', '/']).next().filter(|host| !host.is_empty()).unwrap_or(default_host).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn valid_providers() {
        assert!(VALID_PROVIDERS.contains(&"github"));
        assert!(VALID_PROVIDERS.contains(&"gitlab"));
        assert!(VALID_PROVIDERS.contains(&"bitbucket"));
        assert!(VALID_PROVIDERS.contains(&"custom"));
        assert!(!VALID_PROVIDERS.contains(&"azure"));
    }

    #[test]
    fn valid_credential_types() {
        assert!(VALID_CREDENTIAL_TYPES.contains(&"token"));
        assert!(VALID_CREDENTIAL_TYPES.contains(&"ssh"));
        assert!(VALID_CREDENTIAL_TYPES.contains(&"oauth"));
        assert!(!VALID_CREDENTIAL_TYPES.contains(&"password"));
    }

    #[test]
    fn github_dotcom_env_uses_standard_token_aliases() {
        let mut env = Vec::new();
        append_github_cli_env(&mut env, Some("https://github.com/Wisdoverse/wisdoverse-forge"), "ghp-secret".into());

        assert!(env.contains(&("GH_TOKEN".into(), "ghp-secret".into())));
        assert!(env.contains(&("GITHUB_TOKEN".into(), "ghp-secret".into())));
        assert!(env.contains(&("GH_PROMPT_DISABLED".into(), "1".into())));
        assert!(!env.iter().any(|(k, _)| k == "GH_ENTERPRISE_TOKEN"));
        assert!(!env.iter().any(|(k, _)| k == "GH_HOST"));
    }

    #[test]
    fn github_enterprise_env_sets_host_and_enterprise_token() {
        let mut env = Vec::new();
        append_github_cli_env(&mut env, Some("https://github.enterprise.example/acme/repo"), "ghes-secret".into());

        assert!(env.contains(&("GH_HOST".into(), "github.enterprise.example".into())));
        assert!(env.contains(&("GH_ENTERPRISE_TOKEN".into(), "ghes-secret".into())));
        assert!(env.contains(&("GITHUB_ENTERPRISE_TOKEN".into(), "ghes-secret".into())));
        assert!(!env.iter().any(|(k, _)| k == "GH_TOKEN"));
    }

    #[test]
    fn github_enterprise_cloud_env_sets_host_and_standard_token() {
        let mut env = Vec::new();
        append_github_cli_env(&mut env, Some("acme.ghe.com"), "ghe-secret".into());

        assert!(env.contains(&("GH_HOST".into(), "acme.ghe.com".into())));
        assert!(env.contains(&("GH_TOKEN".into(), "ghe-secret".into())));
        assert!(env.contains(&("GITHUB_TOKEN".into(), "ghe-secret".into())));
        assert!(!env.iter().any(|(k, _)| k == "GH_ENTERPRISE_TOKEN"));
    }

    #[test]
    fn gitlab_env_normalizes_host_for_entrypoint() {
        let mut env = Vec::new();
        append_gitlab_cli_env(
            &mut env,
            Some("git@gitlab.internal.example:group/repo.git"),
            "gitlab-token-placeholder".into(),
        );

        assert!(env.contains(&("GITLAB_TOKEN".into(), "gitlab-token-placeholder".into())));
        assert!(env.contains(&("GITLAB_HOST".into(), "gitlab.internal.example".into())));
    }

    #[test]
    fn normalize_git_host_accepts_common_git_url_shapes() {
        assert_eq!(normalize_git_host(None, "github.com"), "github.com");
        assert_eq!(normalize_git_host(Some("github.com/Wisdoverse/repo"), "github.com"), "github.com");
        assert_eq!(normalize_git_host(Some("git@github.com:Wisdoverse/repo.git"), "github.com"), "github.com");
        assert_eq!(normalize_git_host(Some("https://GitHub.com/Wisdoverse/repo"), "github.com"), "github.com");
    }

    #[test]
    fn decrypt_git_token_roundtrips_saved_ciphertext() {
        let key = [9u8; 32];
        let ciphertext = crypto::encrypt_base64(&key, "ghp-secret").unwrap();
        let cred = GitCredential {
            id: Uuid::now_v7(),
            organization_id: agentforge_core::OrgId::new(),
            user_id: agentforge_core::UserId::new(),
            name: "GitHub".into(),
            provider: "github".into(),
            credential_type: "token".into(),
            token_encrypted: Some(ciphertext.into_bytes()),
            token_nonce: None,
            remote_url: Some("github.com".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(decrypt_git_token(&key, &cred).unwrap().as_deref(), Some("ghp-secret"));
    }
}
