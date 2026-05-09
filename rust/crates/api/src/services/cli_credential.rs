//! Container CLI credential service — resolves what to inject into an agent container.
//!
//! Three-tier policy mirrors the legacy TS `agent.service.ts::injectUserApiKey`:
//! 1. Per-user API key (`user_llm_configs`, AES-GCM encrypted) → provider env var.
//! 2. Stored OAuth credentials (`user_cli_credentials`, AES-GCM encrypted JSON
//!    map of filename→contents from `claude /login` / `codex login` / etc.)
//!    → bind-mounted as a file at `/run/secrets/oauth-credentials/credentials`.
//!    Codex account_id is preserved verbatim because the proxy writer already
//!    extracts `chatgpt_account_id` from the JWT and bakes it into `auth.json`.
//! 3. System-wide fallback API key from `AppConfig.container_*_api_key`.
//!
//! Falls silently through each tier; ultimate "nothing matched" is the caller's
//! concern (container will run but the Container CLI will refuse to auth).

use std::path::{Path, PathBuf};

use agentforge_core::{AppError, AppResult, CliToolKind, ErrorKind, TenantScope, crypto};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use secrecy::{ExposeSecret, SecretString};
use tokio::fs;

use crate::repositories::cli_credential::{CliCredentialRepository, CliCredentialStatus};
use crate::repositories::user_llm_config::UserLlmConfigRepository;

/// Normalise + validate a user-supplied Container CLI tool string. Returns the
/// canonical slug or a 400-style validation error — surfaced by the controller.
pub fn validate_cli_tool(raw: &str) -> AppResult<&'static str> {
    CliToolKind::parse_legacy(raw)
        .map(|tool| tool.as_str())
        .map_err(|err| AppError::from(ErrorKind::Validation(err.to_string())))
}

/// Outcome of credential resolution for a single container spawn.
///
/// Aggregates every env var + bind-mount side-effect needed so the caller
/// (`routes::containers::start_agent`) can stitch them into the
/// `ContainerConfig` without knowing the tier ordering.
#[derive(Debug, Default, Clone)]
pub struct CredentialInjection {
    /// Env vars to add to the container (e.g. `ANTHROPIC_API_KEY=...`,
    /// `AGENTFORGE_CREDENTIAL_SOURCE=oauth-db-mount`).
    pub env: Vec<(String, String)>,
    /// Optional OAuth mount: host dir containing a single `credentials` file,
    /// bind-mounted read-only at `/run/secrets/oauth-credentials/`.
    pub oauth_mount_host_dir: Option<PathBuf>,
}

/// Container CLI tool → provider + env-var mapping. Mirrors the shared TS
/// constants in `shared/types/tools.ts` so both stacks agree on which env var a
/// Container CLI looks for when falling back to raw API keys.
fn tool_provider_env(cli_tool: &str) -> Option<(&'static str, &'static str)> {
    match CliToolKind::parse_legacy(cli_tool).ok()? {
        CliToolKind::Claude | CliToolKind::Opencode => Some(("anthropic", "ANTHROPIC_API_KEY")),
        CliToolKind::Gemini => Some(("google", "GEMINI_API_KEY")),
        CliToolKind::Codex => Some(("openai", "OPENAI_API_KEY")),
    }
}

pub struct CliCredentialService {
    cli_creds: CliCredentialRepository,
    user_llm: UserLlmConfigRepository,
    encryption_key: Option<[u8; 32]>,
    oauth_mount_root: PathBuf,
    system_anthropic: Option<SecretString>,
    system_google: Option<SecretString>,
    system_openai: Option<SecretString>,
}

impl CliCredentialService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cli_creds: CliCredentialRepository,
        user_llm: UserLlmConfigRepository,
        encryption_key: Option<[u8; 32]>,
        oauth_mount_root: PathBuf,
        system_anthropic: Option<SecretString>,
        system_google: Option<SecretString>,
        system_openai: Option<SecretString>,
    ) -> Self {
        Self { cli_creds, user_llm, encryption_key, oauth_mount_root, system_anthropic, system_google, system_openai }
    }

    /// Encrypt + upsert a file map (e.g. `{ "auth.json": "...", "credentials": "..." }`).
    /// Rejects calls when no encryption key is configured — we refuse to store
    /// plaintext credentials, which would be worse than refusing the upload.
    ///
    /// `files` must be a JSON object whose values are all strings (the file
    /// contents). `serde_json::to_string` itself happily serialises numbers /
    /// arrays / nulls, so the shape check is explicit here.
    pub async fn upload(&self, scope: &TenantScope, cli_tool: &str, files: &serde_json::Value) -> AppResult<()> {
        let key = self.encryption_key.as_ref().ok_or_else(|| {
            ErrorKind::Validation(
                "LLM_ENCRYPTION_KEY is not configured — refusing to store plaintext credentials".to_string(),
            )
        })?;
        let tool = validate_cli_tool(cli_tool)?;
        let map = files.as_object().ok_or_else(|| {
            ErrorKind::Validation("`files` must be a JSON object mapping filename → contents".to_string())
        })?;
        if map.is_empty() {
            return Err(ErrorKind::Validation("`files` must not be empty".to_string()).into());
        }
        for (name, value) in map {
            if !value.is_string() {
                return Err(ErrorKind::Validation(format!(
                    "`files.{name}` must be a string; got {}",
                    match value {
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::Bool(_) => "bool",
                        serde_json::Value::Null => "null",
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::Object(_) => "object",
                        serde_json::Value::String(_) => unreachable!(),
                    }
                ))
                .into());
            }
        }
        let plaintext = serde_json::to_string(files)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize files: {err}")))?;
        let ciphertext = crypto::encrypt_base64(key, &plaintext)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("failed to encrypt credentials: {err}")))?;
        self.cli_creds.upsert_encrypted(scope, tool, &ciphertext).await
    }

    /// List stored Container CLI connections for a user. Never returns ciphertext.
    pub async fn list_statuses(&self, scope: &TenantScope) -> AppResult<Vec<CliCredentialStatus>> {
        self.cli_creds.list_for_user(scope).await
    }

    /// Encrypt + upsert credentials directly by `user_id`, without a
    /// `TenantScope`. Used by the `CredentialStreamWorker` queue consumer which
    /// resolves `user_id` from the `agents` table but has no request scope.
    ///
    /// Encryption is mandatory — returns an error when no `LLM_ENCRYPTION_KEY`
    /// is configured (same contract as `upload`).
    pub async fn upsert_encrypted_by_user_id(
        &self,
        user_id: uuid::Uuid,
        cli_tool: &str,
        plaintext_json: &str,
    ) -> AppResult<()> {
        let key = self.encryption_key.as_ref().ok_or_else(|| {
            ErrorKind::Validation(
                "LLM_ENCRYPTION_KEY is not configured — refusing to store plaintext credentials".to_string(),
            )
        })?;
        let tool = validate_cli_tool(cli_tool)?;
        let ciphertext = crypto::encrypt_base64(key, plaintext_json)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("failed to encrypt credentials: {err}")))?;
        self.cli_creds.upsert_encrypted_by_user_id(user_id, tool, &ciphertext).await
    }

    /// Remove the stored blob. Idempotent — no error if nothing was stored.
    pub async fn remove(&self, scope: &TenantScope, cli_tool: &str) -> AppResult<()> {
        let tool = validate_cli_tool(cli_tool)?;
        self.cli_creds.delete(scope, tool).await
    }

    /// Resolve the credential set for an agent container. Never returns an
    /// error for "no credentials" — that's a normal outcome (e.g. unauthed
    /// smoke agent). `Err` is only returned for infra failures (DB, FS).
    pub async fn resolve(
        &self,
        scope: &TenantScope,
        cli_tool: &str,
        container_key: &str,
    ) -> AppResult<CredentialInjection> {
        let Some((provider, env_var)) = tool_provider_env(cli_tool) else {
            // Unknown Container CLI — pre-worker-bridge hello-world tools land here.
            return Ok(CredentialInjection::default());
        };

        // Tier 1: user API key. A row that exists but fails to decrypt is a
        // hard error — falling through to tier 2/3 would silently demote the
        // user to a different identity (shared system key, another user's
        // OAuth blob if rotation ever collided, etc.). The row only got
        // written after a successful encrypt, so decrypt failure almost
        // always means `LLM_ENCRYPTION_KEY` was rotated without re-encrypting
        // — the operator needs to know, not the user.
        if let Some(key) = self.encryption_key
            && let Some(encrypted) = self.user_llm.find_default_api_key(scope, provider).await?
        {
            let plaintext = crypto::decrypt_base64(&key, &encrypted).map_err(|err| {
                tracing::error!(error = %err, user_id = %scope.user_id().as_uuid(), %provider, "Failed to decrypt user LLM API key — refusing to fall back to another tier");
                ErrorKind::Internal(anyhow::anyhow!(
                    "stored user LLM API key failed to decrypt (likely LLM_ENCRYPTION_KEY rotation); re-upload via /api/v1/user-llm-configs"
                ))
            })?;
            let mut out = CredentialInjection::default();
            out.env.push((env_var.to_string(), plaintext));
            out.env.push(("AGENTFORGE_CREDENTIAL_SOURCE".into(), "user".into()));
            return Ok(out);
        }

        // Tier 2: stored OAuth credentials (claude login / codex login / ...).
        // Same hard-error contract as tier 1 — a decrypt failure on an
        // existing row means the key rotated; reconnect is required.
        if let Some(key) = self.encryption_key
            && let Some(encrypted) = self.cli_creds.find_encrypted_active(scope, cli_tool).await?
        {
            let plaintext = crypto::decrypt_base64(&key, &encrypted).map_err(|err| {
                tracing::error!(error = %err, user_id = %scope.user_id().as_uuid(), cli_tool, "Failed to decrypt Container CLI credentials — user must reconnect");
                ErrorKind::Validation(format!(
                    "stored {cli_tool} credentials cannot be decrypted — reconnect via /api/v1/cli-auth-proxy or /api/v1/cli-credentials"
                ))
            })?;
            match self.write_oauth_mount(container_key, plaintext.as_bytes()).await {
                Ok(host_dir) => {
                    let mut out = CredentialInjection { oauth_mount_host_dir: Some(host_dir), ..Default::default() };
                    out.env.push(("AGENTFORGE_CREDENTIAL_SOURCE".into(), "oauth-db-mount".into()));
                    out.env.push(("AGENTFORGE_OAUTH_MOUNT".into(), "/run/secrets/oauth-credentials".into()));
                    return Ok(out);
                }
                Err(err) => {
                    // File-mount failure falls back to env var delivery so
                    // very small blobs still work. Large blobs can hit
                    // E2BIG (Docker env size ceiling) — matches the legacy
                    // warning path in `injectUserApiKey`.
                    tracing::warn!(error = %err, user_id = %scope.user_id().as_uuid(), cli_tool, bytes = plaintext.len(), "File mount failed — falling back to env var for OAuth credentials");
                    let mut out = CredentialInjection::default();
                    let blob = BASE64.encode(plaintext.as_bytes());
                    out.env.push(("AGENTFORGE_OAUTH_CREDENTIALS".into(), blob));
                    out.env.push(("AGENTFORGE_CREDENTIAL_SOURCE".into(), "oauth-db".into()));
                    return Ok(out);
                }
            }
        }

        // Tier 3: system-wide fallback. `CONTAINER_*_API_KEY=` in `.env`
        // deserialises as `Some("")` rather than `None`; treat empty strings as
        // "not configured" so we don't inject a blank env var + spurious
        // `AGENTFORGE_CREDENTIAL_SOURCE=system` label.
        let system_key = match CliToolKind::parse_legacy(cli_tool).ok() {
            Some(CliToolKind::Claude | CliToolKind::Opencode) => self.system_anthropic.as_ref(),
            Some(CliToolKind::Gemini) => self.system_google.as_ref(),
            Some(CliToolKind::Codex) => self.system_openai.as_ref(),
            None => None,
        }
        .map(|s| s.expose_secret())
        .filter(|s| !s.is_empty());
        if let Some(sys) = system_key {
            let mut out = CredentialInjection::default();
            out.env.push((env_var.to_string(), sys.to_string()));
            out.env.push(("AGENTFORGE_CREDENTIAL_SOURCE".into(), "system".into()));
            return Ok(out);
        }

        Ok(CredentialInjection::default())
    }

    /// Write the decrypted OAuth blob into a per-container dir that can be
    /// bind-mounted read-only. Legacy TS encoded the JSON file map base64 and
    /// wrote it to `credentials`; the entrypoint (`agent-entrypoint.sh`)
    /// expects exactly that shape, so we preserve it.
    async fn write_oauth_mount(&self, container_key: &str, plaintext: &[u8]) -> std::io::Result<PathBuf> {
        // Path-traversal guard on the caller-provided key so we can't escape
        // the mount root. Any `/`, `\`, or leading dot would be a bug in the
        // caller (container names are agentforge-generated UUIDs).
        if container_key.is_empty()
            || container_key.contains('/')
            || container_key.contains('\\')
            || container_key == ".."
            || container_key == "."
        {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid container_key"));
        }
        // Host-side isolation is enforced at `oauth_mount_root` (mode 0700)
        // so other host users can't even `cd` into the tree to reach any
        // container's credentials. Inside the (backend-only) root, per-
        // container dirs + files are left world-readable because the agent
        // user inside the spawned container (uid 1011 per
        // `Dockerfile.agent-base`) does NOT share an effective uid with the
        // backend container's `agentforge` user (uid 100). Docker bind
        // mounts preserve numeric uids — without a chown we can't do (no
        // CAP_CHOWN as the unprivileged backend), the agent-side reader
        // would be locked out of anything stricter than 0644.
        //
        // Best-effort chmod (`.ok()`): on a pre-existing root we may not
        // own, the chmod fails and we log + continue. Operators deploying
        // to shared hosts should point `OAUTH_MOUNT_DIR` at a path the
        // backend creates fresh.
        fs::create_dir_all(&self.oauth_mount_root).await?;
        set_mode(&self.oauth_mount_root, 0o700).await.ok();

        let mount_dir = self.oauth_mount_root.join(container_key);
        fs::create_dir_all(&mount_dir).await?;
        set_mode(&mount_dir, 0o755).await.ok();

        let blob = BASE64.encode(plaintext);
        let cred_path = mount_dir.join("credentials");
        fs::write(&cred_path, blob.as_bytes()).await?;
        set_mode(&cred_path, 0o644).await.ok();
        Ok(mount_dir)
    }

    /// Remove a previously-materialised OAuth mount directory. Idempotent —
    /// no error if the dir doesn't exist. Called by `stop_agent` so secrets
    /// don't linger on disk after the container is torn down.
    pub async fn cleanup_oauth_mount(&self, container_key: &str) -> std::io::Result<()> {
        if container_key.is_empty()
            || container_key.contains('/')
            || container_key.contains('\\')
            || container_key == ".."
            || container_key == "."
        {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid container_key"));
        }
        let mount_dir = self.oauth_mount_root.join(container_key);
        match fs::remove_dir_all(&mount_dir).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(unix)]
async fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path).await?;
    let mut perms = meta.permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms).await
}

#[cfg(not(unix))]
async fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_cli_tool_has_no_provider() {
        assert!(tool_provider_env("vim").is_none());
        assert!(tool_provider_env("").is_none());
    }

    #[tokio::test]
    async fn cleanup_oauth_mount_is_idempotent() {
        use crate::repositories::cli_credential::CliCredentialRepository;
        use crate::repositories::user_llm_config::UserLlmConfigRepository;
        let tmp = std::env::temp_dir().join(format!("agentforge-cleanup-test-{}", uuid::Uuid::new_v4()));
        // Build a service with NO DB pool — cleanup doesn't touch the DB.
        // We only need `oauth_mount_root` wired correctly.
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/unused").unwrap();
        let svc = CliCredentialService::new(
            CliCredentialRepository::new(pool.clone()),
            UserLlmConfigRepository::new(pool),
            None,
            tmp.clone(),
            None,
            None,
            None,
        );
        // Never-written mount → idempotent success.
        svc.cleanup_oauth_mount("container-xyz").await.unwrap();
        // Path traversal rejected.
        assert!(svc.cleanup_oauth_mount("..").await.is_err());
        assert!(svc.cleanup_oauth_mount("a/b").await.is_err());
        assert!(svc.cleanup_oauth_mount("").await.is_err());
    }

    #[test]
    fn validate_cli_tool_rejects_unknown() {
        assert!(validate_cli_tool("vim").is_err());
        assert!(validate_cli_tool("").is_err());
        assert!(validate_cli_tool(" claude ").is_ok(), "trims + lowercases");
        assert!(validate_cli_tool("CLAUDE").is_ok());
    }

    #[test]
    fn cli_tool_mapping_matches_shared_constants() {
        assert_eq!(tool_provider_env("claude"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(tool_provider_env("opencode"), Some(("anthropic", "ANTHROPIC_API_KEY")));
        assert_eq!(tool_provider_env("gemini"), Some(("google", "GEMINI_API_KEY")));
        assert_eq!(tool_provider_env("codex"), Some(("openai", "OPENAI_API_KEY")));
    }
}
