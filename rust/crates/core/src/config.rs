//! Configuration loading from environment variables.
//!
//! Compatible with the existing `docker/.env` format. Uses the `config` crate
//! to deserialize environment variables into a typed struct.

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

fn default_port() -> u16 {
    4003
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_jwt_expiry() -> u64 {
    900 // 15 minutes
}

fn default_env() -> String {
    "development".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_codex_default_model() -> String {
    "gpt-5.5".to_string()
}

fn default_cli_auth_proxy_revoke_threshold() -> i32 {
    2
}

fn default_false() -> bool {
    false
}

fn default_storage_provider() -> String {
    "local".to_string()
}

fn default_storage_local_path() -> String {
    "~/.agentforge/data/uploads".to_string()
}

fn default_storage_max_file_size() -> i64 {
    10 * 1024 * 1024
}

fn default_storage_max_files_per_session() -> i64 {
    20
}

fn default_storage_signed_url_expiry() -> u64 {
    3600
}

fn default_minio_bucket() -> String {
    "agentforge".to_string()
}

/// NATS auth callout configuration (issue #38 phase 2).
///
/// All fields are `Option<…>` so the struct can `#[derive(Default)]` —
/// external `AppConfig { .. }` literals construct `NatsCalloutConfig::default()`
/// and remain unchanged when future phases add fields. `AppConfig::from_env`
/// rejects partial configurations (some set, some not) so production cannot
/// boot into a half-wired state.
///
/// Secret-bearing fields use [`SecretString`] so the derived `Debug` emits
/// `[REDACTED alloc::string::String]` instead of the raw bytes.
///
/// The `sys_password` field grants the API a SYS-account connection used
/// exclusively to publish `$SYS.REQ.SERVER.<server_name>.KICK` revocations —
/// the API's `backend` user lives in the AGENTFORGE account and has no
/// access to the `$SYS.>` subject tree. Required together with the other
/// fields so operators cannot silently boot into a half-wired revocation
/// path (missing KICK credentials collapses revocation latency to the
/// 15-minute JWT TTL ceiling with no visible signal).
#[derive(Debug, Default, Deserialize)]
pub struct NatsCalloutConfig {
    /// Password for the `auth_service` user in the AUTH NATS account. The
    /// callout service subscribes `$SYS.REQ.USER.AUTH` using these credentials.
    #[serde(default)]
    pub auth_service_password: Option<SecretString>,

    /// Ed25519 nkey seed used to sign outer `AuthorizationResponse` JWTs
    /// returned to the NATS server. The public half must be listed as
    /// `authorization.auth_callout.issuer` in `docker/nats.conf`.
    #[serde(default)]
    pub issuer_seed: Option<SecretString>,

    /// Ed25519 nkey seed used to sign inner User JWT claims embedded in the
    /// AuthorizationResponse's `nats.jwt` field. The public half must be
    /// listed in the AGENTFORGE account's `signing_keys`.
    #[serde(default)]
    pub account_signing_key_seed: Option<SecretString>,

    /// Curve25519 XKey seed used to decrypt incoming callout requests and
    /// encrypt responses back to the server's per-request ephemeral xkey.
    /// The public half must be listed as `authorization.auth_callout.xkey` in
    /// `docker/nats.conf`. When absent, the callout runs in plaintext mode —
    /// acceptable only for local dev.
    #[serde(default)]
    pub xkey_seed: Option<SecretString>,

    /// NATS server name as configured in `nats.conf` via `server_name:`.
    /// Used to address the `$SYS.REQ.SERVER.<name>.KICK` subject for
    /// targeted connection revocation on `stop_agent`.
    #[serde(default)]
    pub server_name: Option<String>,

    /// Password for the SYS-account `sys` user. The callout worker uses it
    /// to open a second, lazy NATS connection that can publish
    /// `$SYS.REQ.SERVER.<server_name>.KICK` — the API's `backend` user lives
    /// in the AGENTFORGE account and cannot reach `$SYS.>`. When absent, the
    /// `revoke()` path falls back to DB clear + 15-minute JWT TTL ceiling.
    #[serde(default)]
    pub sys_password: Option<SecretString>,
}

// Note on the former `account_signing_key_public` field (removed with issue
// #55): in NATS server-config / non-operator mode neither `accounts.<NAME>
// .signing_keys` nor an account public nkey participates in authorization —
// `nats-server` rejects the former at startup (the original #55 bug) and
// ignores the latter at runtime (`aud` on the inner User JWT is matched
// against account NAMES, not keys). The account placement is now the
// hardcoded `"AGENTFORGE"` literal in `bins/server/src/main.rs`, matching
// the `accounts { AGENTFORGE { … } }` label in `docker/nats.conf`. If a
// future migration moves to operator + JWT-resolver mode, reintroduce the
// field and wire it through `sign_user_jwt::audience_account_name`.

impl NatsCalloutConfig {
    /// Return the names of all fields currently `None`. Used by
    /// `AppConfig::from_env` to produce a precise error when NATS is
    /// configured but some callout secret is missing.
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.auth_service_password.is_none() {
            missing.push("NATS_CALLOUT__AUTH_SERVICE_PASSWORD");
        }
        if self.issuer_seed.is_none() {
            missing.push("NATS_CALLOUT__ISSUER_SEED");
        }
        if self.account_signing_key_seed.is_none() {
            missing.push("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_SEED");
        }
        if self.xkey_seed.is_none() {
            missing.push("NATS_CALLOUT__XKEY_SEED");
        }
        if self.server_name.is_none() {
            missing.push("NATS_CALLOUT__SERVER_NAME");
        }
        if self.sys_password.is_none() {
            missing.push("NATS_CALLOUT__SYS_PASSWORD");
        }
        missing
    }
}

/// Stripe billing configuration.
///
/// Secrets are optional so self-hosted deployments can run without billing.
/// When any Stripe secret is configured, both server-side secrets must be
/// present to avoid accepting checkout/cancellation requests without a webhook
/// path that can reconcile Stripe's source of truth back into PostgreSQL.
#[derive(Debug, Default, Deserialize)]
pub struct StripeConfig {
    pub stripe_secret_key: Option<SecretString>,
    pub stripe_webhook_secret: Option<SecretString>,
    pub stripe_publishable_key: Option<String>,
}

impl StripeConfig {
    pub fn is_configured(&self) -> bool {
        self.stripe_secret_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false)
            && self.stripe_webhook_secret.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false)
    }
}

/// Application configuration loaded from environment variables.
///
/// Required variables: `DATABASE_URL`, `JWT_SECRET`.
/// All others have sensible defaults.
///
/// Secret-bearing fields (`jwt_secret`, `llm_encryption_key`, the three
/// `container_*_api_key` fields, and `cli_auth_proxy_openai_client_secret`)
/// are wrapped in [`SecretString`] so the derived `Debug` emits
/// `[REDACTED alloc::string::String]` instead of the secret material.
/// Any code that needs the underlying bytes must call `.expose_secret()`
/// at the use site so the leak surface is searchable.
///
/// `Clone` is intentionally NOT derived — the config is passed around as
/// `Arc<AppConfig>` (see `AppState`), so a cheap clone would duplicate
/// secret material for no reason.
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    /// HTTP listen port (default: 4003, matching `shared/defaults.ts` SERVER_PORT).
    #[serde(default = "default_port")]
    pub port: u16,

    /// HTTP listen host (default: 0.0.0.0).
    #[serde(default = "default_host")]
    pub host: String,

    /// PostgreSQL connection string (required).
    pub database_url: String,

    /// Redis connection URL (optional — graceful degradation when absent).
    pub redis_url: Option<String>,

    /// NATS connection URL for the backend (optional). Under the account
    /// split introduced in issue #38 this URL carries the backend user's
    /// credentials (`nats://backend:<password>@nats:4222`); production
    /// deployments MUST set it to a URL whose user is authorised for the
    /// full subject namespace.
    pub nats_url: Option<String>,

    /// NATS connection URL injected into spawned agent containers. Defaults
    /// to `nats_url` for backwards compatibility, but production deployments
    /// MUST set this separately to a URL whose user is restricted to
    /// per-agent publish/subscribe subjects (issue #38). Reusing the
    /// backend URL here hands sidecars the backend's credentials — which is
    /// exactly the pre-#38 posture.
    pub nats_agent_url: Option<String>,

    /// NATS auth callout configuration (issue #38 phase 2). Grouped into a
    /// sub-struct so additional callout fields in future phases do NOT
    /// require every external `AppConfig { ... }` literal to change —
    /// callers construct `NatsCalloutConfig::default()` once and forget.
    ///
    /// Env mapping: `NATS_CALLOUT__<FIELD>` → `nats_callout.<field>`
    /// (e.g. `NATS_CALLOUT__ISSUER_SEED` → `nats_callout.issuer_seed`).
    /// The `__` separator is set in `config::Environment::default()`.
    #[serde(default)]
    pub nats_callout: NatsCalloutConfig,

    /// Stripe billing configuration. Flattened so deployments continue to use
    /// the existing flat env names: `STRIPE_SECRET_KEY`,
    /// `STRIPE_WEBHOOK_SECRET`, and `STRIPE_PUBLISHABLE_KEY`.
    #[serde(default, flatten)]
    pub stripe: StripeConfig,

    /// JWT signing secret (required). Wrapped in `SecretString` so the
    /// derived `Debug` redacts it; reach the bytes with `.expose_secret()`.
    pub jwt_secret: SecretString,

    /// JWT token expiry in seconds (default: 900 = 15 min).
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_seconds: u64,

    /// Runtime environment — matches NODE_ENV for compatibility.
    #[serde(default = "default_env")]
    pub environment: String,

    /// Log level filter (default: "info").
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Allowed CORS origin for production (e.g. "https://app.agentforge.dev").
    /// Required in production to prevent open CORS. In development this is ignored.
    pub cors_origin: Option<String>,

    /// Directory holding the built SPA (Vite `dist/`). When set and the directory
    /// exists, the server serves static assets and falls back to `index.html`
    /// for unmatched GET routes so the SPA router owns client-side paths.
    pub static_dir: Option<String>,

    /// Base URL that spawned agent containers use to reach this backend
    /// (`CONTAINER_SERVER_URL`, e.g. `http://agentforge:4003`). Injected into
    /// every agent container as `AGENTFORGE_SERVER_URL`. Optional — when unset
    /// the legacy HTTP dual-write stays disabled and the sidecar still runs.
    pub container_server_url: Option<String>,

    /// Base URL of a local Ollama instance (`OLLAMA_BASE_URL`, e.g.
    /// `http://localhost:11434`). Required when using the "ollama" provider in
    /// `LlmProviderFactory::build`. When unset, build returns `NotConfigured`.
    pub ollama_base_url: Option<String>,

    /// 64-hex-char AES-256 key used by the legacy TS `encryptAesGcm` / Rust
    /// `core::crypto::decrypt_base64` pair. Required to decrypt stored OAuth
    /// credentials (`user_cli_credentials`) and per-user LLM API keys
    /// (`user_llm_configs`). When unset the credential injection paths silently
    /// fall back to the system-level API keys.
    pub llm_encryption_key: Option<SecretString>,

    /// System-wide Anthropic API key injected into `claude` / `opencode` agent
    /// containers as the tier-3 fallback (matches legacy
    /// `CONTAINER_ANTHROPIC_API_KEY`). Used when the user has no per-user key
    /// and no stored OAuth credentials.
    pub container_anthropic_api_key: Option<SecretString>,

    /// System-wide Google API key injected into `gemini` agent containers as
    /// the tier-3 fallback (`CONTAINER_GOOGLE_API_KEY`).
    pub container_google_api_key: Option<SecretString>,

    /// System-wide OpenAI API key injected into `codex` agent containers as
    /// the tier-3 fallback (`CONTAINER_OPENAI_API_KEY`).
    pub container_openai_api_key: Option<SecretString>,

    /// Default model passed to Codex container-CLI agents when the agent row
    /// does not carry an explicit model. Injected into sidecars as
    /// `AGENTFORGE_CLI_MODEL` so production does not inherit user-local
    /// `.codex/config.toml` defaults. Override with `CODEX_DEFAULT_MODEL`;
    /// set it to an empty string to let the Codex CLI choose its own default.
    #[serde(default = "default_codex_default_model")]
    pub codex_default_model: String,

    /// Host-side directory where per-container OAuth credential bind-mounts are
    /// materialised (mirrors legacy `paths.dataDir + /oauth-mounts`). Defaults
    /// to `/tmp/agentforge/oauth-mounts` when unset. Each agent spawn writes a
    /// `credentials` file under `<dir>/<container_name>/` mode 0600 and mounts
    /// the directory read-only at `/run/secrets/oauth-credentials`.
    pub oauth_mount_dir: Option<String>,

    /// Attachment storage provider. `local` stores objects under
    /// `storage_local_path`; `minio` uses the S3-compatible MinIO settings.
    #[serde(default = "default_storage_provider")]
    pub storage_provider: String,

    /// Local attachment object root when `storage_provider=local`.
    #[serde(default = "default_storage_local_path")]
    pub storage_local_path: String,

    /// Maximum accepted attachment payload size in bytes.
    #[serde(default = "default_storage_max_file_size")]
    pub storage_max_file_size: i64,

    /// Per-agent attachment count guard. Enforced by the upload service when an
    /// attachment is associated with an agent.
    #[serde(default = "default_storage_max_files_per_session")]
    pub storage_max_files_per_session: i64,

    /// Signed URL expiry for object-storage providers that support presign.
    /// The current Rust API proxies downloads, but the value is carried in
    /// config to preserve the deployment contract.
    #[serde(default = "default_storage_signed_url_expiry")]
    pub storage_signed_url_expiry: u64,

    /// MinIO/S3 endpoint. Accepts either `host:port` or a full URL.
    pub minio_endpoint: Option<String>,
    pub minio_access_key: Option<SecretString>,
    pub minio_secret_key: Option<SecretString>,
    #[serde(default = "default_minio_bucket")]
    pub minio_bucket: String,
    #[serde(default = "default_false")]
    pub minio_use_ssl: bool,
    pub minio_region: Option<String>,

    /// Issue #41 rollout gate: when `false`, the backend skips spawning the
    /// credential sync worker and the sidecar skips spawning its watcher.
    /// Default `false` for staged rollout; flip to `true` after smoke tests.
    #[serde(default = "default_false")]
    pub credential_sync_enabled: bool,

    /// Admin override for the Codex/OpenAI CLI auth proxy provider. Set
    /// `CLI_AUTH_PROXY_OPENAI_CLIENT_ID` (required for override to apply) plus
    /// optional `_CLIENT_SECRET` / `_AUTH_ENDPOINT` / `_TOKEN_ENDPOINT` to
    /// swap the hard-coded public Codex client for your own OAuth app.
    /// When set alongside `APP_URL`, the proxy flips to server-callback mode
    /// (`GET /api/v1/cli-auth-proxy/openai/callback`) so the IdP can redirect
    /// straight back to us instead of requiring manual paste.
    pub cli_auth_proxy_openai_client_id: Option<String>,
    pub cli_auth_proxy_openai_client_secret: Option<SecretString>,
    pub cli_auth_proxy_openai_auth_endpoint: Option<String>,
    pub cli_auth_proxy_openai_token_endpoint: Option<String>,

    /// Publicly reachable URL of this backend, used to build server-callback
    /// redirect URIs (`${app_url}/api/v1/cli-auth-proxy/openai/callback`).
    /// Optional — manual callback mode works without it.
    pub app_url: Option<String>,

    /// Consecutive `invalid_grant` refresh failures required before the
    /// background CLI auth proxy worker revokes stored CLI credentials.
    /// Default keeps the legacy behavior (2) while allowing staging or
    /// production rollouts to tune noise tolerance without recompiling.
    #[serde(default = "default_cli_auth_proxy_revoke_threshold")]
    pub cli_auth_proxy_revoke_threshold: i32,

    /// SMTP settings for transactional auth email. Password reset depends on
    /// these being fully configured; partial config fails startup instead of
    /// silently accepting reset requests that can never deliver email.
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<SecretString>,
    pub smtp_from: Option<String>,
    #[serde(default = "default_false")]
    pub smtp_secure: bool,
}

impl AppConfig {
    /// Load configuration from environment variables.
    ///
    /// Uses `__` as separator for nested keys (e.g. `DATABASE__URL`),
    /// though the current schema is flat.
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let cfg: Self = config::Config::builder()
            .add_source(config::Environment::default().separator("__").ignore_empty(true))
            .build()?
            .try_deserialize()?;

        if cfg.jwt_secret.expose_secret().len() < 32 {
            return Err(config::ConfigError::Message("JWT_SECRET must be at least 32 characters".to_string()));
        }

        if cfg.cli_auth_proxy_revoke_threshold < 1 {
            return Err(config::ConfigError::Message("CLI_AUTH_PROXY_REVOKE_THRESHOLD must be at least 1".to_string()));
        }

        let stripe_secret_present =
            cfg.stripe.stripe_secret_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false);
        let stripe_webhook_present =
            cfg.stripe.stripe_webhook_secret.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false);
        if stripe_secret_present != stripe_webhook_present {
            return Err(config::ConfigError::Message(
                "STRIPE_SECRET_KEY and STRIPE_WEBHOOK_SECRET must be configured together".to_string(),
            ));
        }

        match cfg.storage_provider.as_str() {
            "local" | "minio" => {}
            other => {
                return Err(config::ConfigError::Message(format!(
                    "STORAGE_PROVIDER must be 'local' or 'minio', got '{other}'"
                )));
            }
        }
        if cfg.storage_max_file_size <= 0 {
            return Err(config::ConfigError::Message("STORAGE_MAX_FILE_SIZE must be positive".to_string()));
        }
        if cfg.storage_max_files_per_session < 1 {
            return Err(config::ConfigError::Message("STORAGE_MAX_FILES_PER_SESSION must be at least 1".to_string()));
        }
        if cfg.storage_provider == "minio" {
            let missing = [
                ("MINIO_ENDPOINT", cfg.minio_endpoint.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false)),
                (
                    "MINIO_ACCESS_KEY",
                    cfg.minio_access_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false),
                ),
                (
                    "MINIO_SECRET_KEY",
                    cfg.minio_secret_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false),
                ),
            ]
            .into_iter()
            .filter_map(|(name, present)| (!present).then_some(name))
            .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(config::ConfigError::Message(format!(
                    "STORAGE_PROVIDER=minio requires {}",
                    missing.join(", ")
                )));
            }
        }

        // NATS auth callout (issue #38 phase 2) — all callout secrets required
        // together when NATS is configured. Fail-fast: refuse to boot into a
        // half-wired state where the callout service can't mint JWTs but
        // agents still try to connect.
        if cfg.nats_url.is_some() {
            let missing = cfg.nats_callout.missing_fields();
            if !missing.is_empty() {
                return Err(config::ConfigError::Message(format!(
                    "NATS auth callout requires {} when NATS_URL is configured (issue #38 phase 2)",
                    missing.join(", ")
                )));
            }
        }

        let smtp_partial = [
            cfg.smtp_host.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            cfg.smtp_user.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            cfg.smtp_password.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false),
            cfg.smtp_from.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        ];
        let smtp_set_count = smtp_partial.iter().filter(|value| **value).count();
        if smtp_set_count != 0 && smtp_set_count != smtp_partial.len() {
            return Err(config::ConfigError::Message(
                "SMTP_HOST, SMTP_USER, SMTP_PASSWORD, and SMTP_FROM must be configured together".to_string(),
            ));
        }

        Ok(cfg)
    }

    /// Returns `true` when running in production.
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt_secret() -> SecretString {
        SecretString::from("agentforge-jwt-placeholder-for-tests".to_string())
    }

    #[test]
    fn from_env_with_required_vars() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                // Clear any ambient optional vars to make assertions deterministic.
                ("REDIS_URL", None),
                ("NATS_URL", None),
                ("NATS_AGENT_URL", None),
                ("NATS_CALLOUT__AUTH_SERVICE_PASSWORD", None),
                ("NATS_CALLOUT__ISSUER_SEED", None),
                ("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_SEED", None),
                ("NATS_CALLOUT__XKEY_SEED", None),
                ("NATS_CALLOUT__SERVER_NAME", None),
                ("NATS_CALLOUT__SYS_PASSWORD", None),
                ("STRIPE_SECRET_KEY", None),
                ("STRIPE_WEBHOOK_SECRET", None),
                ("STRIPE_PUBLISHABLE_KEY", None),
                ("CONTAINER_SERVER_URL", None),
                ("OLLAMA_BASE_URL", None),
                ("LLM_ENCRYPTION_KEY", None),
                ("CONTAINER_ANTHROPIC_API_KEY", None),
                ("CONTAINER_GOOGLE_API_KEY", None),
                ("CONTAINER_OPENAI_API_KEY", None),
                ("CODEX_DEFAULT_MODEL", None),
                ("OAUTH_MOUNT_DIR", None),
                ("STORAGE_PROVIDER", None),
                ("STORAGE_LOCAL_PATH", None),
                ("STORAGE_MAX_FILE_SIZE", None),
                ("STORAGE_MAX_FILES_PER_SESSION", None),
                ("STORAGE_SIGNED_URL_EXPIRY", None),
                ("MINIO_ENDPOINT", None),
                ("MINIO_ACCESS_KEY", None),
                ("MINIO_SECRET_KEY", None),
                ("MINIO_BUCKET", None),
                ("MINIO_USE_SSL", None),
                ("MINIO_REGION", None),
                ("CLI_AUTH_PROXY_OPENAI_CLIENT_ID", None),
                ("CLI_AUTH_PROXY_OPENAI_CLIENT_SECRET", None),
                ("CLI_AUTH_PROXY_OPENAI_AUTH_ENDPOINT", None),
                ("CLI_AUTH_PROXY_OPENAI_TOKEN_ENDPOINT", None),
                ("APP_URL", None),
                ("CLI_AUTH_PROXY_REVOKE_THRESHOLD", None),
                ("SMTP_HOST", None),
                ("SMTP_PORT", None),
                ("SMTP_USER", None),
                ("SMTP_PASSWORD", None),
                ("SMTP_FROM", None),
                ("SMTP_SECURE", None),
                ("ENVIRONMENT", None),
                ("LOG_LEVEL", None),
                ("PORT", None),
                ("HOST", None),
            ],
            || {
                let cfg = AppConfig::from_env();
                assert!(cfg.is_ok(), "config load failed: {:?}", cfg.err());

                let cfg = cfg.unwrap();
                assert_eq!(cfg.database_url, "postgres://localhost/agentforge_test");
                assert_eq!(cfg.jwt_secret.expose_secret(), "test-secret-key-min-32-chars-long!!");
                assert_eq!(cfg.port, 4003);
                assert_eq!(cfg.host, "0.0.0.0");
                assert_eq!(cfg.jwt_expiry_seconds, 900);
                assert_eq!(cfg.environment, "development");
                assert_eq!(cfg.log_level, "info");
                assert!(cfg.redis_url.is_none());
                assert!(cfg.nats_url.is_none());
                assert!(cfg.nats_agent_url.is_none());
                assert!(cfg.nats_callout.auth_service_password.is_none());
                assert!(cfg.nats_callout.issuer_seed.is_none());
                assert!(cfg.nats_callout.account_signing_key_seed.is_none());
                assert!(cfg.nats_callout.xkey_seed.is_none());
                assert!(cfg.nats_callout.server_name.is_none());
                assert!(cfg.nats_callout.sys_password.is_none());
                assert!(!cfg.stripe.is_configured());
                assert!(cfg.stripe.stripe_publishable_key.is_none());
                assert!(cfg.container_server_url.is_none());
                assert!(cfg.ollama_base_url.is_none());
                assert!(cfg.llm_encryption_key.is_none());
                assert!(cfg.container_anthropic_api_key.is_none());
                assert!(cfg.container_google_api_key.is_none());
                assert!(cfg.container_openai_api_key.is_none());
                assert_eq!(cfg.codex_default_model, "gpt-5.5");
                assert!(cfg.oauth_mount_dir.is_none());
                assert_eq!(cfg.storage_provider, "local");
                assert_eq!(cfg.storage_local_path, "~/.agentforge/data/uploads");
                assert_eq!(cfg.storage_max_file_size, 10 * 1024 * 1024);
                assert_eq!(cfg.storage_max_files_per_session, 20);
                assert_eq!(cfg.storage_signed_url_expiry, 3600);
                assert!(cfg.minio_endpoint.is_none());
                assert!(cfg.minio_access_key.is_none());
                assert!(cfg.minio_secret_key.is_none());
                assert_eq!(cfg.minio_bucket, "agentforge");
                assert!(!cfg.minio_use_ssl);
                assert!(cfg.minio_region.is_none());
                assert!(cfg.cli_auth_proxy_openai_client_id.is_none());
                assert!(cfg.app_url.is_none());
                assert_eq!(cfg.cli_auth_proxy_revoke_threshold, 2);
                assert!(cfg.smtp_host.is_none());
                assert!(cfg.smtp_port.is_none());
                assert!(cfg.smtp_user.is_none());
                assert!(cfg.smtp_password.is_none());
                assert!(cfg.smtp_from.is_none());
                assert!(!cfg.smtp_secure);
                assert!(!cfg.credential_sync_enabled);
                assert!(!cfg.is_production());
            },
        );
    }

    #[test]
    fn is_production_returns_true() {
        let cfg = AppConfig {
            port: 4003,
            host: "0.0.0.0".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            redis_url: None,
            nats_url: None,
            nats_agent_url: None,
            nats_callout: NatsCalloutConfig::default(),
            stripe: StripeConfig::default(),
            jwt_secret: test_jwt_secret(),
            jwt_expiry_seconds: 900,
            environment: "production".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            llm_encryption_key: None,
            container_anthropic_api_key: None,
            container_google_api_key: None,
            container_openai_api_key: None,
            codex_default_model: "gpt-5.5".to_string(),
            oauth_mount_dir: None,
            storage_provider: "local".to_string(),
            storage_local_path: "~/.agentforge/data/uploads".to_string(),
            storage_max_file_size: 10 * 1024 * 1024,
            storage_max_files_per_session: 20,
            storage_signed_url_expiry: 3600,
            minio_endpoint: None,
            minio_access_key: None,
            minio_secret_key: None,
            minio_bucket: "agentforge".to_string(),
            minio_use_ssl: false,
            minio_region: None,
            credential_sync_enabled: false,
            cli_auth_proxy_openai_client_id: None,
            cli_auth_proxy_openai_client_secret: None,
            cli_auth_proxy_openai_auth_endpoint: None,
            cli_auth_proxy_openai_token_endpoint: None,
            app_url: None,
            cli_auth_proxy_revoke_threshold: 2,
            smtp_host: None,
            smtp_port: None,
            smtp_user: None,
            smtp_password: None,
            smtp_from: None,
            smtp_secure: false,
        };
        assert!(cfg.is_production());
    }

    #[test]
    fn smtp_partial_configuration_is_rejected() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("SMTP_HOST", Some("smtp.example.com")),
                ("SMTP_USER", Some("noreply@example.com")),
                ("SMTP_PASSWORD", None),
                ("SMTP_FROM", Some("Wisdoverse Forge <noreply@example.com>")),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("SMTP_HOST"), "error was: {err}");
                assert!(err.contains("configured together"), "error was: {err}");
            },
        );
    }

    #[test]
    fn empty_optional_environment_values_are_ignored() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("SMTP_HOST", Some("")),
                ("SMTP_PORT", Some("")),
                ("SMTP_USER", Some("")),
                ("SMTP_PASSWORD", Some("")),
                ("SMTP_FROM", Some("")),
                ("SMTP_SECURE", Some("")),
            ],
            || {
                let cfg = AppConfig::from_env();
                assert!(cfg.is_ok(), "config load failed: {:?}", cfg.err());

                let cfg = cfg.unwrap();
                assert!(cfg.smtp_host.is_none());
                assert!(cfg.smtp_port.is_none());
                assert!(cfg.smtp_user.is_none());
                assert!(cfg.smtp_password.is_none());
                assert!(cfg.smtp_from.is_none());
                assert!(!cfg.smtp_secure);
            },
        );
    }

    #[test]
    fn stripe_partial_configuration_is_rejected() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("STRIPE_SECRET_KEY", Some("sk_test_configured")),
                ("STRIPE_WEBHOOK_SECRET", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("STRIPE_SECRET_KEY"), "error was: {err}");
                assert!(err.contains("STRIPE_WEBHOOK_SECRET"), "error was: {err}");
            },
        );
    }

    #[test]
    fn stripe_configuration_loads_when_secrets_are_paired() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("STRIPE_SECRET_KEY", Some("sk_test_configured")),
                ("STRIPE_WEBHOOK_SECRET", Some("whsec_configured")),
                ("STRIPE_PUBLISHABLE_KEY", Some("pk_test_configured")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("paired Stripe config should load");
                assert!(cfg.stripe.is_configured());
                assert_eq!(cfg.stripe.stripe_publishable_key.as_deref(), Some("pk_test_configured"));
            },
        );
    }

    #[test]
    fn nats_callout_secrets_required_when_nats_url_set() {
        // Partial configuration: NATS_URL set but callout secrets missing.
        // This is the exact foot-gun the validation guards against — a
        // half-wired state where the callout can't mint JWTs but agents try
        // to connect. Must fail-fast at boot.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", Some("nats://backend:pw@nats:4222")),
                // Intentionally NOT setting the six callout env vars.
                ("NATS_CALLOUT__AUTH_SERVICE_PASSWORD", None),
                ("NATS_CALLOUT__ISSUER_SEED", None),
                ("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_SEED", None),
                ("NATS_CALLOUT__XKEY_SEED", None),
                ("NATS_CALLOUT__SERVER_NAME", None),
                ("NATS_CALLOUT__SYS_PASSWORD", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("auth callout requires"), "error was: {err}");
                assert!(err.contains("NATS_CALLOUT__ISSUER_SEED"), "error was: {err}");
                assert!(err.contains("NATS_CALLOUT__SERVER_NAME"), "error was: {err}");
                assert!(err.contains("NATS_CALLOUT__SYS_PASSWORD"), "error was: {err}");
                // Regression guard for issue #55 follow-up: the account
                // public nkey env var is no longer part of the callout
                // contract — the account name placement claim is a
                // hardcoded string inside `main.rs`. A future re-addition
                // of this env var should be intentional (operator mode).
                assert!(
                    !err.contains("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_PUBLIC"),
                    "account pubkey env var must NOT be required after #55 — error was: {err}"
                );
            },
        );
    }

    #[test]
    fn nats_callout_secrets_ignored_when_nats_url_absent() {
        // Dev-mode sanity: when NATS_URL is not set, the callout secrets
        // may also be absent without rejection.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("NATS_AGENT_URL", None),
                ("NATS_CALLOUT__AUTH_SERVICE_PASSWORD", None),
                ("NATS_CALLOUT__ISSUER_SEED", None),
                ("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_SEED", None),
                ("NATS_CALLOUT__XKEY_SEED", None),
                ("NATS_CALLOUT__SERVER_NAME", None),
                ("NATS_CALLOUT__SYS_PASSWORD", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_ok(), "should succeed without NATS_URL; got: {:?}", result.err());
            },
        );
    }

    #[test]
    fn jwt_secret_too_short_rejected() {
        temp_env::with_vars(
            [("DATABASE_URL", Some("postgres://localhost/agentforge_test")), ("JWT_SECRET", Some("too-short"))],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("at least 32 characters"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_accepts_custom_cli_auth_proxy_revoke_threshold() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("SMTP_HOST", None),
                ("SMTP_USER", None),
                ("SMTP_PASSWORD", None),
                ("SMTP_FROM", None),
                ("CLI_AUTH_PROXY_REVOKE_THRESHOLD", Some("4")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("custom threshold should load");
                assert_eq!(cfg.cli_auth_proxy_revoke_threshold, 4);
            },
        );
    }

    #[test]
    fn from_env_rejects_zero_cli_auth_proxy_revoke_threshold() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("SMTP_HOST", None),
                ("SMTP_USER", None),
                ("SMTP_PASSWORD", None),
                ("SMTP_FROM", None),
                ("CLI_AUTH_PROXY_REVOKE_THRESHOLD", Some("0")),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("CLI_AUTH_PROXY_REVOKE_THRESHOLD"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_rejects_unknown_storage_provider() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("STORAGE_PROVIDER", Some("s3")),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("STORAGE_PROVIDER"), "error was: {err}");
                assert!(err.contains("local"), "error was: {err}");
                assert!(err.contains("minio"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_requires_minio_credentials_when_provider_is_minio() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("STORAGE_PROVIDER", Some("minio")),
                ("MINIO_ENDPOINT", None),
                ("MINIO_ACCESS_KEY", None),
                ("MINIO_SECRET_KEY", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("STORAGE_PROVIDER=minio requires"), "error was: {err}");
                assert!(err.contains("MINIO_ENDPOINT"), "error was: {err}");
                assert!(err.contains("MINIO_ACCESS_KEY"), "error was: {err}");
                assert!(err.contains("MINIO_SECRET_KEY"), "error was: {err}");
            },
        );
    }

    #[test]
    fn debug_output_redacts_secret_fields() {
        // Guards against a future `tracing::info!(?config)` exfiltrating tokens.
        // `SecretString::Debug` emits `SecretBox<…>([REDACTED])` — we assert the
        // known secret literal never appears, regardless of the exact wrapper.
        let cfg = AppConfig {
            port: 4003,
            host: "0.0.0.0".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            redis_url: None,
            nats_url: None,
            nats_agent_url: None,
            nats_callout: NatsCalloutConfig {
                auth_service_password: Some(SecretString::from("nats-auth-svc-supersecret".to_string())),
                issuer_seed: Some(SecretString::from(
                    "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-callout-issuer-seed".to_string(),
                )),
                account_signing_key_seed: Some(SecretString::from(
                    "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-account-sk-seed".to_string(),
                )),
                xkey_seed: Some(SecretString::from("SXAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-xkey-seed".to_string())),
                server_name: Some("agentforge-test".to_string()),
                sys_password: Some(SecretString::from("nats-sys-supersecret".to_string())),
            },
            stripe: StripeConfig {
                stripe_secret_key: Some(SecretString::from("sk-stripe-supersecret".to_string())),
                stripe_webhook_secret: Some(SecretString::from("whsec-stripe-supersecret".to_string())),
                stripe_publishable_key: Some("pk_test_publishable".to_string()),
            },
            jwt_secret: SecretString::from("jwt-supersecret-value-min-32-chars!!".to_string()),
            jwt_expiry_seconds: 900,
            environment: "development".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            llm_encryption_key: Some(SecretString::from("enc-key-supersecret".to_string())),
            container_anthropic_api_key: Some(SecretString::from("sk-ant-supersecret".to_string())),
            container_google_api_key: Some(SecretString::from("goog-supersecret".to_string())),
            container_openai_api_key: Some(SecretString::from("sk-openai-supersecret".to_string())),
            codex_default_model: "gpt-5.5".to_string(),
            oauth_mount_dir: None,
            storage_provider: "local".to_string(),
            storage_local_path: "~/.agentforge/data/uploads".to_string(),
            storage_max_file_size: 10 * 1024 * 1024,
            storage_max_files_per_session: 20,
            storage_signed_url_expiry: 3600,
            minio_endpoint: None,
            minio_access_key: None,
            minio_secret_key: None,
            minio_bucket: "agentforge".to_string(),
            minio_use_ssl: false,
            minio_region: None,
            credential_sync_enabled: false,
            cli_auth_proxy_openai_client_id: None,
            cli_auth_proxy_openai_client_secret: Some(SecretString::from("client-supersecret".to_string())),
            cli_auth_proxy_openai_auth_endpoint: None,
            cli_auth_proxy_openai_token_endpoint: None,
            app_url: None,
            cli_auth_proxy_revoke_threshold: 2,
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_port: Some(587),
            smtp_user: Some("noreply@example.com".to_string()),
            smtp_password: Some(SecretString::from("smtp-supersecret".to_string())),
            smtp_from: Some("Wisdoverse Forge <noreply@example.com>".to_string()),
            smtp_secure: true,
        };
        let dbg = format!("{cfg:?}");
        for needle in [
            "jwt-supersecret-value-min-32-chars!!",
            "enc-key-supersecret",
            "sk-ant-supersecret",
            "goog-supersecret",
            "sk-openai-supersecret",
            "client-supersecret",
            "nats-auth-svc-supersecret",
            "callout-issuer-seed",
            "account-sk-seed",
            "xkey-seed",
            "nats-sys-supersecret",
            "smtp-supersecret",
            "sk-stripe-supersecret",
            "whsec-stripe-supersecret",
        ] {
            assert!(!dbg.contains(needle), "Debug leaked secret {needle:?}: {dbg}");
        }
        // And the expose path still returns the literal — no silent zeroing.
        assert_eq!(cfg.jwt_secret.expose_secret(), "jwt-supersecret-value-min-32-chars!!");
    }

    #[test]
    fn defaults_are_sensible() {
        assert_eq!(default_port(), 4003);
        assert_eq!(default_host(), "0.0.0.0");
        assert_eq!(default_jwt_expiry(), 900);
        assert_eq!(default_env(), "development");
        assert_eq!(default_log_level(), "info");
        assert_eq!(default_cli_auth_proxy_revoke_threshold(), 2);
        assert_eq!(default_storage_provider(), "local");
        assert_eq!(default_storage_local_path(), "~/.agentforge/data/uploads");
        assert_eq!(default_storage_max_file_size(), 10 * 1024 * 1024);
        assert_eq!(default_storage_max_files_per_session(), 20);
        assert_eq!(default_storage_signed_url_expiry(), 3600);
        assert_eq!(default_minio_bucket(), "agentforge");
    }
}
