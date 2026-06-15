//! Git credential service - validation, management, and Git platform CLI injection.

use agentforge_core::{AppResult, TenantScope, crypto};
use agentforge_db::entities::GitCredential;
use agentforge_platform::SecretBytes;
use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::credential::{
    CredentialListPage, GitCredentialDraft, GitCredentialEncryptionPolicy, GitCredentialToken, GitRemoteHost,
};
pub(crate) use crate::domain::credential::{
    credential_delete_response, git_credential_response, git_credentials_response,
};
use crate::repositories::credential::git::GitCredentialRepository;

/// Business logic layer for git credential operations.
pub struct GitCredentialService {
    repo: GitCredentialRepository,
    encryption_key: Option<[u8; 32]>,
}

/// Service input for creating a git credential from HTTP/API fields.
pub(crate) struct CreateGitCredentialInput {
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) credential_type: String,
    pub(crate) remote_url: Option<String>,
    pub(crate) token: Option<String>,
}

/// Service input for the legacy provider-scoped upsert endpoint.
pub(crate) struct UpsertGitCredentialInput {
    pub(crate) provider: String,
    pub(crate) token: String,
    pub(crate) host: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) credential_type: Option<String>,
}

/// Environment variables needed by Git platform CLIs inside an agent container.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GitCliCredentialInjection {
    pub env: Vec<(String, String)>,
}

/// One host-matched git credential resolved for a project clone (M5/M6).
///
/// Carries the `credential_id` (so the worker records WHICH credential it used
/// on the attempt — never the secret) and the decrypted credential bytes in a
/// [`SecretBytes`] wrapper that NEVER serializes and NEVER `Debug`-prints the
/// token (it `zeroize`-scrubs on drop). The bytes are already in the M3/M4
/// clone-entrypoint secret-file form (`x-access-token:<token>` for GitHub,
/// `oauth2:<token>` for GitLab), so the worker writes them to the mounted secret
/// file verbatim and never has to reason about the colon-form contract again.
///
/// Deliberately NOT `Clone`/`Serialize`/`Debug`-revealing: the secret should
/// have exactly one owner whose drop scrubs it, and the worker materializes it
/// only at the instant it launches the container (see M5 worker).
pub struct ResolvedCredential {
    /// The selected `git_credentials.id` — recorded on the attempt row. Never
    /// the secret.
    pub credential_id: Uuid,
    /// The decrypted credential bytes in the clone-entrypoint secret-file form
    /// (`x-access-token:<token>` / `oauth2:<token>`). Never logged or serialized.
    pub secret: SecretBytes,
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Print only the credential id; the secret is intentionally redacted.
        f.debug_struct("ResolvedCredential")
            .field("credential_id", &self.credential_id)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl GitCredentialService {
    pub fn new(repo: GitCredentialRepository) -> Self {
        Self { repo, encryption_key: None }
    }

    pub fn from_pool(pool: PgPool, encryption_key: Option<[u8; 32]>) -> Self {
        Self::with_encryption_key(GitCredentialRepository::new(pool), encryption_key)
    }

    pub fn with_encryption_key(repo: GitCredentialRepository, encryption_key: Option<[u8; 32]>) -> Self {
        Self { repo, encryption_key }
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
        let credential = GitCredentialDraft::parse(name, provider, credential_type)?;

        self.repo
            .create(
                scope,
                credential.name(),
                credential.provider(),
                credential.credential_type(),
                remote_url,
                token_encrypted,
                token_nonce,
            )
            .await
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
        let credential = GitCredentialDraft::parse(name, provider, credential_type)?;

        self.repo
            .upsert_for_provider(
                scope,
                credential.name(),
                credential.provider(),
                credential.credential_type(),
                remote_url,
                token_encrypted,
                token_nonce,
            )
            .await
    }

    /// Create a git credential from request fields, encrypting the token when present.
    pub(crate) async fn create_with_token(
        &self,
        scope: &TenantScope,
        input: CreateGitCredentialInput,
    ) -> AppResult<GitCredential> {
        let token_encrypted = self.encrypt_git_token(input.token.as_deref())?;
        self.upsert_for_provider(
            scope,
            &input.name,
            &input.provider,
            &input.credential_type,
            trimmed_opt(input.remote_url.as_deref()),
            token_encrypted.as_deref(),
            None,
        )
        .await
    }

    /// Upsert a provider-scoped git credential using legacy frontend defaults.
    pub(crate) async fn upsert_provider_with_token(
        &self,
        scope: &TenantScope,
        input: UpsertGitCredentialInput,
    ) -> AppResult<GitCredential> {
        let provider = input.provider.trim().to_ascii_lowercase();
        let remote_url = trimmed_opt(input.host.as_deref());
        let credential_type = trimmed_opt(input.credential_type.as_deref()).unwrap_or("token");
        let default_name = remote_url.map_or_else(|| provider.clone(), |host| format!("{provider} ({host})"));
        let name = trimmed_opt(input.name.as_deref()).unwrap_or(default_name.as_str());
        let token_encrypted = self.encrypt_git_token(Some(input.token.as_str()))?;

        self.upsert_for_provider(scope, name, &provider, credential_type, remote_url, token_encrypted.as_deref(), None)
            .await
    }

    /// List git credentials (paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<GitCredential>> {
        let page = CredentialListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
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

        let key = encryption_key.ok_or_else(GitCredentialEncryptionPolicy::missing_decrypt_key)?;
        let mut out = GitCliCredentialInjection::default();

        for cred in creds {
            let Some(token) = decrypt_git_token(&key, &cred)? else {
                continue;
            };
            // The CLI-env path necessarily materializes the token as an env-var
            // String value; `(*token).clone()` copies out of the Zeroizing wrapper
            // at this boundary while the decrypted source still scrubs on drop.
            match cred.provider.as_str() {
                "github" => append_github_cli_env(&mut out.env, cred.remote_url.as_deref(), (*token).clone()),
                "gitlab" => append_gitlab_cli_env(&mut out.env, cred.remote_url.as_deref(), (*token).clone()),
                _ => {}
            }
        }

        Ok(out)
    }

    /// Resolve EXACTLY ONE org-scoped git credential whose host matches the clone
    /// repository URL's host (M5/M6 host-matched credential selection).
    ///
    /// This is NOT "latest token per provider": it picks the single credential
    /// that targets the SAME host as the repo being cloned, so a project on
    /// `gitlab.example.com` never picks up a `gitlab.com` token (and vice versa).
    /// Matching, in priority order:
    ///
    /// 1. an exact host match on the credential's `remote_url` (normalized);
    /// 2. for a credential with no usable `remote_url`, a match on the provider's
    ///    canonical SaaS host (`github`→`github.com`, `gitlab`→`gitlab.com`) — so
    ///    a bare provider token still serves a `github.com` / `gitlab.com` clone.
    ///
    /// The candidate list is ordered newest-first, so among equally-matching
    /// credentials the most recently updated wins.
    ///
    /// Returns `Ok(None)` when no credential matches the host — the worker then
    /// clones ANONYMOUSLY (public-repo path). A private repo with no matching
    /// credential simply fails the clone with git's auth error, which the worker
    /// classifies + redacts; we never silently fall back to a wrong-host token.
    ///
    /// The decrypted bytes are returned ONLY here, in a [`SecretBytes`] wrapper,
    /// and only the SINGLE selected credential is decrypted (the others are never
    /// touched), minimizing the plaintext blast radius. Org-scoped: a credential
    /// from another organization can never be selected.
    pub async fn resolve_for_host(&self, scope: &TenantScope, host: &str) -> AppResult<Option<ResolvedCredential>> {
        let target = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if target.is_empty() {
            return Ok(None);
        }

        let candidates = self.repo.org_token_candidates(scope.org_id().as_uuid()).await?;

        // Two passes so an EXPLICIT remote_url host match always beats a
        // provider-canonical fallback, regardless of row order.
        let pick = candidates
            .iter()
            .find(|cred| credential_remote_host(cred).as_deref() == Some(target.as_str()))
            .or_else(|| {
                candidates.iter().find(|cred| {
                    credential_remote_host(cred).is_none()
                        && provider_canonical_host(&cred.provider).as_deref() == Some(target.as_str())
                })
            });

        let Some(cred) = pick else {
            return Ok(None);
        };

        // Decrypt ONLY the selected credential — the moment we are about to hand
        // it to the container. The key is required; a configured-but-keyless
        // deployment surfaces the standard decrypt-key error rather than silently
        // cloning anonymously for a private repo.
        let key = self.encryption_key.ok_or_else(GitCredentialEncryptionPolicy::missing_decrypt_key)?;
        let Some(token) = decrypt_git_token(&key, cred)? else {
            // The selected credential had no usable token after decrypt (blank);
            // treat as "no credential" rather than mounting an empty secret (the
            // entrypoint REFUSES a present-but-empty secret).
            return Ok(None);
        };

        // Build the colon-form secret bytes in a Zeroizing buffer, then MOVE them
        // into SecretBytes (a move, not a heap copy). `mem::take` empties the
        // intermediate Zeroizing so no non-zeroized plaintext copy survives.
        let mut bytes = clone_secret_bytes(&cred.provider, &token);
        Ok(Some(ResolvedCredential { credential_id: cred.id, secret: SecretBytes::new(std::mem::take(&mut *bytes)) }))
    }

    /// Get a git credential by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<GitCredential> {
        self.repo.find_by_id(scope, id).await
    }

    /// Delete a git credential by ID.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    fn encrypt_git_token(&self, token: Option<&str>) -> AppResult<Option<Vec<u8>>> {
        let Some(token) = trimmed_opt(token) else {
            return Ok(None);
        };
        let Some(token) = GitCredentialToken::parse(token) else {
            return Ok(None);
        };
        let key = self.encryption_key.as_ref().ok_or_else(GitCredentialEncryptionPolicy::missing_storage_key)?;
        let encrypted =
            crypto::encrypt_base64(key, token.value()).map_err(GitCredentialEncryptionPolicy::encrypt_failed)?;
        Ok(Some(encrypted.into_bytes()))
    }
}

fn trimmed_opt(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Normalized host of a credential's `remote_url`, or `None` when it has no
/// usable `remote_url`. Reuses [`GitRemoteHost::normalize`] but with an empty
/// default so a missing/blank `remote_url` yields `None` (not a guessed host) —
/// that case falls through to the provider-canonical match instead.
fn credential_remote_host(cred: &GitCredential) -> Option<String> {
    let raw = trimmed_opt(cred.remote_url.as_deref())?;
    let host = GitRemoteHost::normalize(Some(raw), "");
    if host.is_empty() { None } else { Some(host) }
}

/// Canonical SaaS host for a provider slug, for credentials with no `remote_url`.
/// Only the two v1-supported providers resolve; anything else is `None` (so a
/// `custom`/`bitbucket` token never matches by provider alone — it must carry a
/// `remote_url`).
fn provider_canonical_host(provider: &str) -> Option<String> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "github" => Some("github.com".to_string()),
        "gitlab" => Some("gitlab.com".to_string()),
        _ => None,
    }
}

/// Build the clone-entrypoint secret-file bytes for a token, in the colon-form
/// the M3 helper contract requires (see `docker/scripts/clone-entrypoint.sh`):
///
/// - GitHub: `x-access-token:<token>` (PAT-over-HTTPS / app-installation form);
/// - GitLab: `oauth2:<token>` (GitLab OAuth2/PAT-over-HTTPS form);
/// - anything else: `x-access-token:<token>` as a safe default username.
///
/// Always a colon-form indicator, never a bare token — so a token that itself
/// contains a `:` can never be mis-split into user/pass by the helper.
///
/// The bytes are assembled into a [`Zeroizing<Vec<u8>>`] by extending a buffer
/// (NOT a `format!`-built plain `String`, which would leave a non-zeroized
/// plaintext copy of the token on the heap). The final [`SecretBytes`] the caller
/// wraps it in is itself `Zeroizing`, so both the intermediate buffer and the
/// final secret scrub on drop.
fn clone_secret_bytes(provider: &str, token: &str) -> Zeroizing<Vec<u8>> {
    let username: &[u8] = match provider.trim().to_ascii_lowercase().as_str() {
        "gitlab" => b"oauth2:",
        // github + every other provider default to the x-access-token username.
        _ => b"x-access-token:",
    };
    let mut bytes = Zeroizing::new(Vec::with_capacity(username.len() + token.len()));
    bytes.extend_from_slice(username);
    bytes.extend_from_slice(token.as_bytes());
    bytes
}

/// Decrypt a stored git credential token into a [`Zeroizing<String>`] so every
/// plaintext heap copy is scrubbed on drop.
///
/// The plaintext that `crypto::decrypt_base64` returns is wrapped in `Zeroizing`
/// IMMEDIATELY (closing the upstream copy), and the trimmed result is also a
/// `Zeroizing<String>`, so the token never lingers as a non-zeroized `String`.
/// Returns `None` for a blank/empty token (treated as "no usable credential").
fn decrypt_git_token(key: &[u8; 32], cred: &GitCredential) -> AppResult<Option<Zeroizing<String>>> {
    let Some(ciphertext) = cred.token_encrypted.as_deref() else {
        return Ok(None);
    };
    let ciphertext = std::str::from_utf8(ciphertext)
        .map_err(|err| GitCredentialEncryptionPolicy::ciphertext_not_utf8(&cred.provider, err))?;
    // Wrap the decrypted plaintext the instant it exists so the upstream copy is
    // zeroized on drop (not left lingering as a plain String on the heap).
    let token = Zeroizing::new(crypto::decrypt_base64(key, ciphertext).map_err(|err| {
        tracing::error!(
            error = %err,
            credential_id = %cred.id,
            provider = %cred.provider,
            "Failed to decrypt stored git credential token"
        );
        GitCredentialEncryptionPolicy::decrypt_failed(&cred.provider)
    })?);
    // The trimmed value is also Zeroizing; a blank token yields None.
    Ok(GitCredentialToken::parse(&token).map(|token| Zeroizing::new(token.value().to_string())))
}

fn append_github_cli_env(env: &mut Vec<(String, String)>, remote_url: Option<&str>, token: String) {
    let host = GitRemoteHost::normalize(remote_url, "github.com");
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
    env.push(("GITLAB_HOST".into(), GitRemoteHost::normalize(remote_url, "gitlab.com")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

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

    fn credential_with_ciphertext(ciphertext: String) -> GitCredential {
        GitCredential {
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
        }
    }

    /// Borrow the decrypted token as `&str` for assertions (the result is a
    /// `Zeroizing<String>`; deref to compare).
    fn decrypted(key: &[u8; 32], cred: &GitCredential) -> Option<String> {
        decrypt_git_token(key, cred).unwrap().map(|t| (*t).clone())
    }

    #[test]
    fn decrypt_git_token_roundtrips_saved_ciphertext() {
        let key = [9u8; 32];
        let ciphertext = crypto::encrypt_base64(&key, "ghp-secret").unwrap();
        let cred = credential_with_ciphertext(ciphertext);

        assert_eq!(decrypted(&key, &cred).as_deref(), Some("ghp-secret"));
    }

    #[test]
    fn decrypt_git_token_trims_and_ignores_blank_tokens() {
        let key = [9u8; 32];
        let padded = credential_with_ciphertext(crypto::encrypt_base64(&key, "  ghp-secret  ").unwrap());
        let blank = credential_with_ciphertext(crypto::encrypt_base64(&key, "   ").unwrap());

        assert_eq!(decrypted(&key, &padded).as_deref(), Some("ghp-secret"));
        assert_eq!(decrypted(&key, &blank), None);
    }

    fn credential_with(provider: &str, remote_url: Option<&str>) -> GitCredential {
        GitCredential {
            id: Uuid::now_v7(),
            organization_id: agentforge_core::OrgId::new(),
            user_id: agentforge_core::UserId::new(),
            name: provider.into(),
            provider: provider.into(),
            credential_type: "token".into(),
            token_encrypted: Some(b"x".to_vec()),
            token_nonce: None,
            remote_url: remote_url.map(str::to_string),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn credential_remote_host_normalizes_known_url_shapes() {
        // https URL, ssh URL, and host:port all reduce to the bare lowercase host.
        assert_eq!(
            credential_remote_host(&credential_with("github", Some("https://github.com/o/r.git"))).as_deref(),
            Some("github.com")
        );
        assert_eq!(
            credential_remote_host(&credential_with("gitlab", Some("git@gitlab.example.com:group/repo.git")))
                .as_deref(),
            Some("gitlab.example.com")
        );
        assert_eq!(
            credential_remote_host(&credential_with("github", Some("GitHub.Enterprise.Example"))).as_deref(),
            Some("github.enterprise.example")
        );
        // A blank/missing remote_url yields None (falls through to provider match).
        assert_eq!(credential_remote_host(&credential_with("github", None)), None);
        assert_eq!(credential_remote_host(&credential_with("github", Some("   "))), None);
    }

    #[test]
    fn provider_canonical_host_only_resolves_supported_saas() {
        assert_eq!(provider_canonical_host("github").as_deref(), Some("github.com"));
        assert_eq!(provider_canonical_host("GitLab").as_deref(), Some("gitlab.com"));
        // Unsupported providers never match by provider alone (must carry a url).
        assert_eq!(provider_canonical_host("bitbucket"), None);
        assert_eq!(provider_canonical_host("custom"), None);
    }

    #[test]
    fn clone_secret_bytes_uses_provider_colon_form() {
        // GitHub uses x-access-token; GitLab uses oauth2; both are colon-form so a
        // token containing ':' can never be mis-split by the entrypoint helper.
        // (The bytes live in a Zeroizing buffer; deref to compare.)
        assert_eq!(*clone_secret_bytes("github", "ghp_abc:def"), b"x-access-token:ghp_abc:def".to_vec());
        assert_eq!(*clone_secret_bytes("gitlab", "glpat-xyz"), b"oauth2:glpat-xyz".to_vec());
        // Any other provider defaults to the x-access-token username.
        assert_eq!(*clone_secret_bytes("custom", "tok"), b"x-access-token:tok".to_vec());
    }

    #[test]
    fn resolved_credential_debug_never_leaks_the_secret() {
        let resolved = ResolvedCredential {
            credential_id: Uuid::now_v7(),
            secret: SecretBytes::from(b"x-access-token:ghp_supersecret".to_vec()),
        };
        let debug = format!("{resolved:?}");
        assert!(!debug.contains("ghp_supersecret"), "secret leaked through Debug: {debug}");
        assert!(debug.contains("[REDACTED]"));
    }
}
