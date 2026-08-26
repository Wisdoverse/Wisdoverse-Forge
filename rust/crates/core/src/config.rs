//! Configuration loading from environment variables.
//!
//! Compatible with the existing `docker/.env` format. Uses the `config` crate
//! to deserialize environment variables into a typed struct.

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

fn default_port() -> u16 {
    4003
}

/// Deserialize a `Vec<String>` from either a comma-separated string (how the env
/// source delivers list values) or a native sequence (config files / tests).
/// Whitespace around each item is trimmed and empty items are dropped.
fn deserialize_comma_separated<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct CommaSeparated;
    impl<'de> serde::de::Visitor<'de> for CommaSeparated {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a comma-separated string or a sequence of strings")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    out.push(item);
                }
            }
            Ok(out)
        }
    }
    deserializer.deserialize_any(CommaSeparated)
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

fn default_true() -> bool {
    true
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

/// Known review check keys (must stay in sync with the domain policy and
/// the frontend checklist items).
pub const KNOWN_REVIEW_GATES: &[&str] = &["result_matches_brief", "artifacts_checked", "no_secrets", "reusable_saved"];

/// Structural boot check for `REVIEW_REQUIRED_GATES`: only known check keys
/// may be required, so a typo never silently disables the acceptance gate.
fn validate_review_gates(csv: &str) -> Result<(), config::ConfigError> {
    for key in csv.split(',').map(str::trim).filter(|entry| !entry.is_empty()) {
        if !KNOWN_REVIEW_GATES.contains(&key) {
            return Err(config::ConfigError::Message(format!(
                "REVIEW_REQUIRED_GATES contains unknown check key '{key}'",
            )));
        }
    }
    Ok(())
}

/// Structural boot check for `LLM_PRICING` so a typoed JSON blob fails
/// loudly at startup instead of silently disabling cost estimates.
fn validate_llm_pricing(json_text: &str) -> Result<(), config::ConfigError> {
    let value: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|err| config::ConfigError::Message(format!("LLM_PRICING must be valid JSON: {err}")))?;
    let object = value.as_object().ok_or_else(|| {
        config::ConfigError::Message("LLM_PRICING must be a JSON object of model -> { input, output }.".to_string())
    })?;
    for (model, rate) in object {
        let input = rate
            .get("input")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| config::ConfigError::Message(format!("LLM_PRICING[{model}].input must be a number")))?;
        let output = rate
            .get("output")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| config::ConfigError::Message(format!("LLM_PRICING[{model}].output must be a number")))?;
        if input < 0.0 || output < 0.0 {
            return Err(config::ConfigError::Message(format!("LLM_PRICING[{model}] rates must be >= 0")));
        }
    }
    Ok(())
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
/// Enterprise sign-in via a generic OpenID Connect provider (e.g. Casdoor,
/// Keycloak, Authentik, Entra ID). All fields optional so external
/// `AppConfig { .. }` literals stay unchanged; `AppConfig::from_env` fails
/// fast when SSO is half-configured (enabled without the required fields).
///
/// Env mapping uses the same `__` separator: `AUTH_SSO__ENABLED`,
/// `AUTH_SSO__OIDC_DISCOVERY_URL`, `AUTH_SSO__OIDC_CLIENT_ID`,
/// `AUTH_SSO__OIDC_CLIENT_SECRET`, `AUTH_SSO__OIDC_SCOPES`,
/// `AUTH_SSO__DISPLAY_NAME`, `AUTH_SSO__SPA_BASE_URL`.
#[derive(Debug, Default, Deserialize)]
pub struct SsoConfig {
    /// Master switch for the SSO sign-in button and the OIDC flow.
    #[serde(default)]
    pub enabled: bool,
    /// OIDC discovery document URL (e.g.
    /// `https://casdoor.example.com/.well-known/openid-configuration`).
    pub oidc_discovery_url: Option<String>,
    /// OIDC client id registered for this instance.
    pub oidc_client_id: Option<String>,
    /// OIDC client secret. Wrapped so derived `Debug` redacts it.
    pub oidc_client_secret: Option<SecretString>,
    /// Space-separated OIDC scopes (default: `openid profile email`).
    #[serde(default = "default_sso_scopes")]
    pub oidc_scopes: String,
    /// Button label in the login page (default: `Single sign-on`).
    pub display_name: Option<String>,
    /// Public base URL of the SPA (login page), e.g.
    /// `https://forge.example.com` — where the user lands after SSO.
    pub spa_base_url: Option<String>,
    /// Userinfo claim holding the user's group/role list (e.g. `groups`).
    /// When set together with `admin_groups`, sign-ins sync the org role:
    /// members found in an admin group become `admin`; members outside those
    /// groups become `member`. Owners are never overwritten.
    pub role_claim: Option<String>,
    /// Comma-separated group names that grant the org `admin` role.
    pub admin_groups: Option<String>,
    /// Org provisioning map: `orgSlug=group1;orgSlug2=group2`. When the
    /// provider groups contain the mapped group, the user is added to that
    /// org (as `member`, or `admin` when also in `admin_groups`). Requires
    /// `role_claim`.
    pub org_group_map: Option<String>,
    /// Team provisioning map: `teamName=group1;teamName2=group2`. When the
    /// provider groups contain the mapped group, the user is added to that
    /// team (by name, inside the org they inherit from `org_group_map` or
    /// their default org) as `member`, or `admin` when also in
    /// `admin_groups`. With `deprovision`, a missing group removes the team
    /// membership. Requires `role_claim`.
    pub team_group_map: Option<String>,
    /// Deprovisioning policy: when `true`, sign-in is denied when none of the
    /// mapped org groups apply; otherwise memberships for other missing groups
    /// are removed when safe. Owners and the last org membership are retained.
    #[serde(default)]
    pub deprovision: bool,
    /// Shared secret that protects the instant-off deprovisioning endpoint
    /// (`POST /api/v1/auth/deprovision`). Provider/IdP automation sends this
    /// header to revoke a user's non-owner memberships immediately instead of
    /// waiting for the next sign-in. Unset = the endpoint is disabled.
    pub deprovision_token: Option<SecretString>,
}

fn default_sso_scopes() -> String {
    "openid profile email".to_string()
}

/// Required variables: `DATABASE_URL`, `JWT_SECRET`.
/// All others have sensible defaults.
///
/// Secret-bearing fields (`jwt_secret`, `bootstrap_admin_token`,
/// `llm_encryption_key`, the three `container_*_api_key` fields, and
/// `cli_auth_proxy_openai_client_secret`) are wrapped in [`SecretString`] so the derived `Debug` emits
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

    /// CN-7: the operator's declaration that this deployment runs MORE THAN ONE
    /// replica, so it must NOT silently fall back to the per-process in-memory
    /// OAuth/PKCE state store. That fallback splits the authorize-side `put` and
    /// the callback-side `take` across replicas, breaking the CLI auth flow with
    /// no boot-time signal. When `true`, [`AppConfig::from_env`] fails fast unless
    /// `REDIS_URL` is set. Default `false` keeps single-replica behaviour.
    #[serde(default = "default_false")]
    pub require_external_state: bool,

    /// ADR 0008 Phase 2 rollout gate. When `true` AND Redis is connected, agent
    /// liveness (`last_seen` / offline detection) is served from a Redis TTL key
    /// instead of a per-heartbeat PostgreSQL write; `participants`/`agents`
    /// remain the durable source of truth for lease-relevant `busy`/`available`
    /// status. When `false` (default) or Redis is unavailable, the worker uses
    /// the Phase 1 PostgreSQL path. Default `false` for a dark, flag-gated
    /// rollout: deploying the code changes nothing until this is flipped.
    #[serde(default = "default_false")]
    pub presence_redis_enabled: bool,

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

    /// NATS connection URL injected into CONTAINER-backed agents only. Falls
    /// back to `nats_agent_url` (then `nats_url`) when unset.
    ///
    /// Deployments that firewall the host's public NATS port away from the
    /// Docker bridge must set this to an address reachable from the agent
    /// network (e.g. `nats://agentforge-nats:4222`), while `nats_agent_url`
    /// keeps the public address that off-host Host CLI agents join against.
    pub nats_container_url: Option<String>,

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

    /// Enterprise single sign-on (OpenID Connect).
    #[serde(default)]
    pub auth_sso: SsoConfig,

    /// Stripe billing configuration. Flattened so deployments continue to use
    /// the existing flat env names: `STRIPE_SECRET_KEY`,
    /// `STRIPE_WEBHOOK_SECRET`, and `STRIPE_PUBLISHABLE_KEY`.
    #[serde(default, flatten)]
    pub stripe: StripeConfig,

    /// JWT signing secret (required). Wrapped in `SecretString` so the
    /// derived `Debug` redacts it; reach the bytes with `.expose_secret()`.
    pub jwt_secret: SecretString,

    /// One-time setup key required when a production deployment has no active
    /// platform administrator. Existing deployments with an administrator do
    /// not need it, so the field remains optional for upgrade compatibility.
    pub bootstrap_admin_token: Option<SecretString>,

    /// Explicit local-only opt-in for creating the first administrator without
    /// a setup token. Ignored outside `development` and `test` modes.
    #[serde(default)]
    pub allow_unprotected_admin_bootstrap: bool,

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

    /// Additional allowed image-reference prefixes for dev-environment containers
    /// (`DEV_ENV_ALLOWED_IMAGE_REGISTRIES`, comma-separated, e.g.
    /// `ghcr.io/myorg/,docker.io/`). Official Docker Hub library images and the
    /// managed `agentforge-agent` images are always allowed; this only widens the
    /// F018 allowlist. Empty by default (closed except the built-in safe set).
    /// Deserialized from a comma-separated string because the env source provides
    /// scalar strings, not sequences.
    #[serde(default, deserialize_with = "deserialize_comma_separated")]
    pub dev_env_allowed_image_registries: Vec<String>,

    /// F004: operator opt-in to force-reset stored legacy unsalted SHA-256
    /// password hashes at startup — replace each with the reset sentinel and
    /// stamp the session-invalidation floor. The startup routine runs it ONLY
    /// when a password-reset path (SMTP) is configured, so enabling it can never
    /// lock out a legacy user who would otherwise have no way back in. Off by
    /// default; the compat window already blocks legacy logins in production.
    #[serde(default = "default_false")]
    pub force_reset_legacy_sha256: bool,

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

    /// Optional JSON pricing per LLM model, in USD per 1M tokens, used for
    /// analytics cost estimates:
    /// `LLM_PRICING={"gpt-4o":{"input":2.5,"output":10.0}}`.
    /// Model keys match the model string recorded on assistant messages;
    /// missing models simply get no estimate.
    #[serde(default)]
    pub llm_pricing: Option<String>,

    /// Comma-separated review check keys that must ALL be completed (by any
    /// reviewer) before a human can mark a task completed: `REVIEW_REQUIRED_GATES`.
    /// Known keys: `result_matches_brief`, `artifacts_checked`, `no_secrets`,
    /// `reusable_saved`. Unknown keys fail startup.
    #[serde(default)]
    pub review_required_gates: Option<String>,

    /// Scheduled compliance export cadence in hours (0 = off). Exports are
    /// written to `COMPLIANCE_EXPORT_DIR` as per-org CSV files plus a
    /// `.last_run` marker so a restart does not immediately re-export.
    #[serde(default)]
    pub compliance_export_interval_hours: i64,

    /// Directory for scheduled compliance exports. Required when
    /// `COMPLIANCE_EXPORT_INTERVAL_HOURS > 0`.
    pub compliance_export_dir: Option<String>,

    /// Retention (days) for telemetry tables (`events`,
    /// `analytics_events`); 0 = keep forever. Purged on boot and every 6 h.
    #[serde(default)]
    pub analytics_retention_days: i64,

    /// Retention (days) for finished run attempts of terminal tasks; 0 = keep
    /// forever. Deleting a run nulls run-scoped event/message/attachment links
    /// (records preserved) and cascades context injections.
    #[serde(default)]
    pub run_retention_days: i64,

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

    /// Permit Host CLI enrollment over a plaintext `nats://` URL.
    ///
    /// By default (`false`) the enrollment service rejects any NATS URL that
    /// does not start with `tls://`. Set `ALLOW_PLAINTEXT_HOST_NATS=true` only
    /// in isolated development environments where a TLS NATS server is not
    /// available. Production MUST keep this unset or `false`.
    #[serde(default = "default_false")]
    pub allow_plaintext_host_nats: bool,

    /// Base URL the one-command join script downloads `agentforge-sidecar`
    /// binaries from when the operator machine does not have one installed.
    /// Defaults to this repository's GitHub latest-release downloads. Point it
    /// at an internal mirror for air-gapped deployments
    /// (`HOST_JOIN_BINARY_BASE_URL=https://mirror.example.com/agentforge`).
    pub host_join_binary_base_url: Option<String>,

    /// Deployment-side CLI agent-image auto-updater rollout gate. When `false`
    /// (default) the backend does not spawn the updater and nothing polls the
    /// registry. When `true` (and a Docker daemon is available) a background
    /// worker periodically pulls newer `agent-<tool>:latest` overlays so newly
    /// spawned agents use the current CLI. Running agents are never touched.
    #[serde(default = "default_false")]
    pub cli_image_auto_update_enabled: bool,

    /// How often (seconds) the CLI agent-image auto-updater polls the registry.
    /// Default 900 (15 min). CLI publishers ship at most a few times per week,
    /// so this is well clear of registry rate limits.
    #[serde(default = "default_cli_image_update_interval")]
    pub cli_image_auto_update_interval_secs: u64,

    /// Prune superseded (dangling) agent overlay images after each updater
    /// sweep. `false` (default) keeps the destructive image-removal path off.
    /// Only effective when `cli_image_auto_update_enabled` is also true (prune
    /// runs inside the updater loop). Image-level only — never touches running
    /// or stopped containers, and removes only our own dangling overlays that no
    /// container references.
    #[serde(default = "default_false")]
    pub cli_image_prune_enabled: bool,

    /// Auto-build the local `claude` agent image when a newer
    /// `@anthropic-ai/claude-code` version is published on npm. `false`
    /// (default) keeps the updater sweep detect-only for claude: the admin
    /// panel shows "update available" with a one-click Build button. When
    /// `true` (and `cli_image_auto_update_enabled` is on) the sweep builds the
    /// overlay image server-side with zero clicks. `claude` has no public
    /// registry image — its license requires a local build — so this is the
    /// only auto-update path for that tool.
    #[serde(default = "default_false")]
    pub cli_image_claude_auto_build: bool,

    /// npm registry base URL the claude version check and local build use.
    /// Defaults to `https://registry.npmjs.org` at the use-site. Operators
    /// behind a firewall (or in China) can point it at a mirror such as
    /// `https://registry.npmmirror.com`; the value is also passed to the
    /// generated Dockerfile as the `NPM_REGISTRY` build-arg so the in-image
    /// `npm install` uses the same mirror.
    pub cli_image_npm_registry: Option<String>,

    /// Enable the project-clone worker + reconciler (project-git-clone, M5).
    /// `true` (default) starts the worker when a Docker daemon is available; it
    /// dequeues `project_clone` jobs, runs the ephemeral clone container, and
    /// owns the attempt status machine. Set `false` to disable cloning entirely
    /// (e.g. an air-gapped deployment that never clones).
    #[serde(default = "default_true")]
    pub project_clone_worker_enabled: bool,

    /// Clone image ref the worker launches per clone. Defaults to
    /// `agentforge-clone:latest` at the use-site. Override with
    /// `PROJECT_CLONE_IMAGE` to pin a digest or a registry copy.
    pub project_clone_image: Option<String>,

    /// Backend-controlled secret root (mode 0700) the per-clone credential file
    /// is materialized under, OUTSIDE the projects/workspace tree agent
    /// containers bind. Defaults to `/tmp/agentforge/clone-secrets`. The runtime
    /// owns the file ownership/mode mechanics; this is just the root path.
    pub project_clone_secret_root: Option<String>,

    /// Hard wall-clock timeout per clone, seconds. Default 600 (10 min).
    #[serde(default = "default_clone_timeout_secs")]
    pub project_clone_timeout_secs: u64,

    /// GitHub App identifier used by the self-fix loop to mint installation
    /// tokens and open/merge PRs. Required together with the other three
    /// `github_app_*` fields, or all four must be absent — partial config
    /// fails startup so the loop cannot boot half-wired.
    #[serde(default)]
    pub github_app_id: Option<String>,

    /// GitHub App installation identifier (the per-account install of the
    /// App above) used to scope minted installation tokens.
    #[serde(default)]
    pub github_app_installation_id: Option<String>,

    /// GitHub App private key (PEM) used to sign the JWT that exchanges for an
    /// installation token. Wrapped in [`SecretString`] so the derived `Debug`
    /// redacts it; reach the bytes with `.expose_secret()`.
    #[serde(default)]
    pub github_app_private_key: Option<SecretString>,

    /// "owner/repo" the self-fix loop targets.
    #[serde(default)]
    pub github_app_repo: Option<String>,

    /// Enable the self-fix PR-bridge worker. `true` (default) starts the worker
    /// that dequeues `self_fix_pr` jobs and drives `SelfFixService::open_pr`.
    /// Set `false` to keep PR opening manual (e.g. while the GitHub App is
    /// unconfigured). Env: `SELF_FIX_PR_WORKER_ENABLED`.
    #[serde(default = "default_true")]
    pub self_fix_pr_worker_enabled: bool,

    /// Maximum number of merge attempts before `approve_and_merge` hard-refuses
    /// with `merge_attempts_exhausted` and flips `review_status` to
    /// `changes_requested`. Protects against runaway retry loops.
    /// Default: 5. Env: `SELF_FIX_MAX_MERGE_ATTEMPTS`.
    #[serde(default = "default_self_fix_max_merge_attempts")]
    pub self_fix_max_merge_attempts: i32,

    /// How long (seconds) a self-fix task may stay in `in_review` before the
    /// reaper backstop flips it to `changes_requested`. The self-fix loop then
    /// re-queues the task for another fix attempt. Default: 604800 (7 days).
    /// Env: `SELF_FIX_REVIEW_DEADLINE_SECS`.
    #[serde(default = "default_self_fix_review_deadline_secs")]
    pub self_fix_review_deadline_secs: u64,

    /// How long (seconds) a `blocked/waiting_agent` task may sit before the
    /// reaper ages it out with `status='canceled'` and
    /// `failure_code='waiting_agent_timeout'`. Default 3600 (1 hour).
    /// Env: `BLOCKED_TASK_TTL_SECS`.
    #[serde(default = "default_blocked_task_ttl_secs")]
    pub blocked_task_ttl_secs: u64,

    /// How long (seconds) a claimed `running` job_queue row may hold its lock
    /// before the stale-lock reaper returns it to `pending` for re-dispatch.
    /// Guards against workers that crash mid-job. Must exceed the longest
    /// legitimate job runtime. Releasing does not consume a retry attempt.
    /// Default 1800 (30 minutes). Env: `JOB_QUEUE_STALE_LOCK_TIMEOUT_SECS`.
    #[serde(default = "default_job_queue_stale_lock_timeout_secs")]
    pub job_queue_stale_lock_timeout_secs: u64,
}

fn default_self_fix_max_merge_attempts() -> i32 {
    5
}

fn default_self_fix_review_deadline_secs() -> u64 {
    604800 // 7 days
}

fn default_blocked_task_ttl_secs() -> u64 {
    3600
}

fn default_job_queue_stale_lock_timeout_secs() -> u64 {
    1800
}

fn default_clone_timeout_secs() -> u64 {
    600
}

fn default_cli_image_update_interval() -> u64 {
    900
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
        if cfg.bootstrap_admin_token.as_ref().is_some_and(|token| token.expose_secret().len() < 32) {
            return Err(config::ConfigError::Message(
                "BOOTSTRAP_ADMIN_TOKEN must be at least 32 characters".to_string(),
            ));
        }

        if cfg.cli_auth_proxy_revoke_threshold < 1 {
            return Err(config::ConfigError::Message("CLI_AUTH_PROXY_REVOKE_THRESHOLD must be at least 1".to_string()));
        }

        if cfg.auth_sso.enabled {
            let sso = &cfg.auth_sso;
            let mut missing = Vec::new();
            if sso.oidc_discovery_url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                missing.push("AUTH_SSO__OIDC_DISCOVERY_URL");
            }
            if sso.oidc_client_id.as_deref().map(str::trim).unwrap_or("").is_empty() {
                missing.push("AUTH_SSO__OIDC_CLIENT_ID");
            }
            let secret_present =
                sso.oidc_client_secret.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false);
            if !secret_present {
                missing.push("AUTH_SSO__OIDC_CLIENT_SECRET");
            }
            if sso.spa_base_url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                missing.push("AUTH_SSO__SPA_BASE_URL");
            }
            let role_claim = sso.role_claim.as_deref().map(str::trim).filter(|v| !v.is_empty());
            let admin_groups = sso.admin_groups.as_deref().map(str::trim).filter(|v| !v.is_empty());
            match (role_claim, admin_groups) {
                (None, Some(_)) => missing.push("AUTH_SSO__ROLE_CLAIM"),
                (Some(_), None) => missing.push("AUTH_SSO__ADMIN_GROUPS"),
                _ => {}
            }
            if sso.org_group_map.as_deref().map(str::trim).unwrap_or("").is_empty() {
                if sso.deprovision {
                    missing.push("AUTH_SSO__ORG_GROUP_MAP");
                }
            } else if role_claim.is_none() {
                missing.push("AUTH_SSO__ROLE_CLAIM");
            }
            if !sso.team_group_map.as_deref().map(str::trim).unwrap_or("").is_empty() && role_claim.is_none() {
                missing.push("AUTH_SSO__ROLE_CLAIM");
            }
            if !missing.is_empty() {
                return Err(config::ConfigError::Message(format!(
                    "AUTH_SSO__ENABLED requires: {}",
                    missing.join(", ")
                )));
            }
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

        // GitHub App (self-fix loop) — all four fields required together when
        // any is set. Fail-fast: a half-wired App can't mint installation
        // tokens, so the loop must refuse to boot rather than silently fail
        // every PR attempt at runtime.
        let github_app_fields = [
            cfg.github_app_id.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            cfg.github_app_installation_id.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            cfg.github_app_private_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false),
            cfg.github_app_repo.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        ];
        let github_app_set = github_app_fields.iter().filter(|v| **v).count();
        if github_app_set != 0 && github_app_set != github_app_fields.len() {
            return Err(config::ConfigError::Message(
                "GITHUB_APP_ID, GITHUB_APP_INSTALLATION_ID, GITHUB_APP_PRIVATE_KEY, and GITHUB_APP_REPO \
                 must be configured together (self-fix loop)"
                    .to_string(),
            ));
        }

        // LLM_ENCRYPTION_KEY is mandatory in production. Without it, tier-1 (user
        // LLM API keys) and tier-2 (stored OAuth/CLI credentials) resolution
        // fails closed and every agent silently falls back to the shared system
        // key — i.e. all tenants execute under one identity — and credential
        // upload/test/save break at runtime with confusing 400s and no boot-time
        // signal. Fail-fast at startup instead (F020), mirroring the NATS/GitHub
        // groupings above.
        let llm_key_missing =
            cfg.llm_encryption_key.as_ref().map(|v| v.expose_secret().trim().is_empty()).unwrap_or(true);
        if cfg.is_production() && llm_key_missing {
            return Err(config::ConfigError::Message(
                "LLM_ENCRYPTION_KEY is required in production (credential encryption fails closed without it)"
                    .to_string(),
            ));
        }

        // CN-7: multi-replica deployments must externalise the OAuth/PKCE state
        // store to Redis. The in-memory `StateStore::Memory` fallback is
        // process-local, so with >1 replica the authorize `put` and the callback
        // `take` can land on different replicas and the CLI auth flow fails with
        // no boot-time signal. When the operator declares external state required,
        // fail fast unless Redis is configured rather than silently degrading.
        if cfg.require_external_state && cfg.redis_url.is_none() {
            return Err(config::ConfigError::Message(
                "REQUIRE_EXTERNAL_STATE=true requires REDIS_URL: multi-replica deployments need Redis for the \
                 shared OAuth/PKCE state store; the in-memory fallback is single-replica only"
                    .to_string(),
            ));
        }

        if let Some(pricing) = cfg.llm_pricing.as_deref() {
            validate_llm_pricing(pricing)?;
        }

        if let Some(gates) = cfg.review_required_gates.as_deref() {
            validate_review_gates(gates)?;
        }

        if cfg.compliance_export_interval_hours > 0 && cfg.compliance_export_dir.is_none() {
            return Err(config::ConfigError::Message(
                "COMPLIANCE_EXPORT_INTERVAL_HOURS requires COMPLIANCE_EXPORT_DIR".to_string(),
            ));
        }

        if cfg.analytics_retention_days < 0 || cfg.run_retention_days < 0 {
            return Err(config::ConfigError::Message(
                "ANALYTICS_RETENTION_DAYS / RUN_RETENTION_DAYS must be 0 (off) or a positive number of days"
                    .to_string(),
            ));
        }

        Ok(cfg)
    }

    /// Returns `true` when running in production.
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }
}

/// CN-7 STARTUP readiness check, complementing the config-time URL-present guard
/// in [`AppConfig::from_env`]. When a deployment declares it needs external
/// (shared) state, Redis must be USABLE for the state store — not merely have a
/// `REDIS_URL`, not merely be connected, but actually accept the `SET`/`GETDEL`
/// the CLI auth proxy performs — or the proxy selects the Redis store and then
/// 500s on every read/write. Call this from the server binary AFTER the Redis
/// client is created, passing the result of `RedisClient::probe_read_write()`, so
/// a malformed, unreachable, OR read-only / ACL-restricted `REDIS_URL` fails fast
/// at boot instead of at runtime.
pub fn ensure_external_state_redis_ready(
    require_external_state: bool,
    redis_usable: bool,
) -> Result<(), config::ConfigError> {
    if require_external_state && !redis_usable {
        return Err(config::ConfigError::Message(
            "REQUIRE_EXTERNAL_STATE=true but Redis is not usable for the shared state store (check that \
             REDIS_URL is valid, reachable, and accepts writes — not read-only or ACL-restricted); refusing \
             to boot a multi-replica deployment without a usable shared state store"
                .to_string(),
        ));
    }
    Ok(())
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
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                // Clear any ambient optional vars to make assertions deterministic.
                ("BOOTSTRAP_ADMIN_TOKEN", None),
                ("ALLOW_UNPROTECTED_ADMIN_BOOTSTRAP", None),
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
                assert!(cfg.bootstrap_admin_token.is_none());
                assert!(!cfg.allow_unprotected_admin_bootstrap);
                assert_eq!(cfg.port, 4003);
                assert_eq!(cfg.host, "0.0.0.0");
                assert_eq!(cfg.jwt_expiry_seconds, 900);
                assert_eq!(cfg.environment, "development");
                assert_eq!(cfg.log_level, "info");
                assert!(cfg.redis_url.is_none());
                assert!(!cfg.presence_redis_enabled);
                assert!(cfg.nats_url.is_none());
                assert!(cfg.nats_agent_url.is_none());
                assert!(cfg.nats_container_url.is_none());
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
            presence_redis_enabled: false,
            require_external_state: false,
            nats_url: None,
            nats_agent_url: None,
            nats_container_url: None,
            nats_callout: NatsCalloutConfig::default(),
            stripe: StripeConfig::default(),
            auth_sso: SsoConfig::default(),
            jwt_secret: test_jwt_secret(),
            bootstrap_admin_token: None,
            allow_unprotected_admin_bootstrap: false,
            jwt_expiry_seconds: 900,
            environment: "production".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            dev_env_allowed_image_registries: Vec::new(),
            force_reset_legacy_sha256: false,
            llm_encryption_key: None,
            container_anthropic_api_key: None,
            container_google_api_key: None,
            container_openai_api_key: None,
            codex_default_model: "gpt-5.5".to_string(),
            llm_pricing: None,
            review_required_gates: None,
            compliance_export_interval_hours: 0,
            compliance_export_dir: None,
            analytics_retention_days: 0,
            run_retention_days: 0,
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
            allow_plaintext_host_nats: false,
            host_join_binary_base_url: None,
            cli_image_auto_update_enabled: false,
            cli_image_auto_update_interval_secs: 900,
            cli_image_prune_enabled: false,
            cli_image_claude_auto_build: false,
            cli_image_npm_registry: None,
            project_clone_worker_enabled: false,
            project_clone_image: None,
            project_clone_secret_root: None,
            project_clone_timeout_secs: 600,
            github_app_id: None,
            github_app_installation_id: None,
            github_app_private_key: None,
            github_app_repo: None,
            self_fix_pr_worker_enabled: true,
            self_fix_max_merge_attempts: 5,
            self_fix_review_deadline_secs: 604800,
            blocked_task_ttl_secs: 3600,
            job_queue_stale_lock_timeout_secs: 1800,
        };
        assert!(cfg.is_production());
    }

    #[test]
    fn smtp_partial_configuration_is_rejected() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
    fn github_app_fields_must_be_all_or_none() {
        // 1. Only GITHUB_APP_ID set -> partial config rejected at boot.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("GITHUB_APP_ID", Some("123456")),
                ("GITHUB_APP_INSTALLATION_ID", None),
                ("GITHUB_APP_PRIVATE_KEY", None),
                ("GITHUB_APP_REPO", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("GITHUB_APP_ID"), "error was: {err}");
                assert!(err.contains("configured together"), "error was: {err}");
            },
        );

        // 2. All four set -> loads, fields populated.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("GITHUB_APP_ID", Some("123456")),
                ("GITHUB_APP_INSTALLATION_ID", Some("789012")),
                ("GITHUB_APP_PRIVATE_KEY", Some("-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----")),
                ("GITHUB_APP_REPO", Some("acme/widgets")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("full GitHub App config should load");
                assert_eq!(cfg.github_app_id.as_deref(), Some("123456"));
                assert_eq!(cfg.github_app_installation_id.as_deref(), Some("789012"));
                assert_eq!(
                    cfg.github_app_private_key.as_ref().map(|v| v.expose_secret().to_string()),
                    Some("-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----".to_string())
                );
                assert_eq!(cfg.github_app_repo.as_deref(), Some("acme/widgets"));
            },
        );

        // 3. None set -> feature simply disabled, config loads.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("GITHUB_APP_ID", None),
                ("GITHUB_APP_INSTALLATION_ID", None),
                ("GITHUB_APP_PRIVATE_KEY", None),
                ("GITHUB_APP_REPO", None),
            ],
            || {
                let cfg = AppConfig::from_env().expect("absent GitHub App config should load");
                assert!(cfg.github_app_id.is_none());
                assert!(cfg.github_app_installation_id.is_none());
                assert!(cfg.github_app_private_key.is_none());
                assert!(cfg.github_app_repo.is_none());
            },
        );
    }

    #[test]
    fn jwt_secret_too_short_rejected() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("too-short")),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("at least 32 characters"), "error was: {err}");
            },
        );
    }

    #[test]
    fn bootstrap_admin_token_too_short_rejected() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("BOOTSTRAP_ADMIN_TOKEN", Some("too-short")),
            ],
            || {
                let err = AppConfig::from_env().expect_err("short setup key must fail closed").to_string();
                assert!(err.contains("BOOTSTRAP_ADMIN_TOKEN"), "error was: {err}");
                assert!(err.contains("at least 32 characters"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_accepts_custom_cli_auth_proxy_revoke_threshold() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
    fn from_env_accepts_llm_pricing() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("LLM_PRICING", Some(r#"{"gpt-4o":{"input":2.5,"output":10.0}}"#)),
            ],
            || {
                let cfg = AppConfig::from_env().expect("pricing should load");
                assert!(cfg.llm_pricing.as_deref().unwrap().contains("gpt-4o"));
            },
        );
    }

    #[test]
    fn from_env_accepts_review_gates() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("REVIEW_REQUIRED_GATES", Some("no_secrets,result_matches_brief")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("review gates should load");
                assert!(cfg.review_required_gates.as_deref().unwrap().contains("no_secrets"));
            },
        );
    }

    #[test]
    fn from_env_rejects_unknown_review_gates() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("REVIEW_REQUIRED_GATES", Some("mystery_gate")),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("REVIEW_REQUIRED_GATES"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_accepts_compliance_export_config() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("COMPLIANCE_EXPORT_INTERVAL_HOURS", Some("24")),
                ("COMPLIANCE_EXPORT_DIR", Some("/var/lib/agentforge/compliance")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("compliance config should load");
                assert_eq!(cfg.compliance_export_interval_hours, 24);
            },
        );
    }

    #[test]
    fn from_env_rejects_compliance_interval_without_dir() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("COMPLIANCE_EXPORT_INTERVAL_HOURS", Some("24")),
            ],
            || {
                let err = AppConfig::from_env().expect_err("pairing must be explicit");
                let message = err.to_string();
                assert!(message.contains("COMPLIANCE_EXPORT_DIR"), "error was: {message}");
            },
        );
    }

    #[test]
    fn from_env_rejects_malformed_llm_pricing() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("LLM_PRICING", Some(r#"{"gpt-4o":{"input":-1,"output":10}}"#)),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("LLM_PRICING"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_rejects_unknown_storage_provider() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
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
                ("REQUIRE_EXTERNAL_STATE", None),
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
    fn from_env_rejects_require_external_state_without_redis() {
        // CN-7: a multi-replica deployment that declares it needs external state
        // but configures no Redis must fail fast, not silently use the
        // single-replica in-memory store.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("REQUIRE_EXTERNAL_STATE", Some("true")),
                ("REDIS_URL", None),
            ],
            || {
                let result = AppConfig::from_env();
                assert!(result.is_err(), "REQUIRE_EXTERNAL_STATE=true with no REDIS_URL must be rejected");
                let err = result.unwrap_err().to_string();
                assert!(err.contains("REQUIRE_EXTERNAL_STATE"), "error was: {err}");
                assert!(err.contains("REDIS_URL"), "error was: {err}");
            },
        );
    }

    #[test]
    fn from_env_accepts_require_external_state_with_redis() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("REQUIRE_EXTERNAL_STATE", Some("true")),
                ("REDIS_URL", Some("redis://localhost:6379")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("REQUIRE_EXTERNAL_STATE with REDIS_URL must be accepted");
                assert!(cfg.require_external_state);
                assert_eq!(cfg.redis_url.as_deref(), Some("redis://localhost:6379"));
            },
        );
    }

    #[test]
    fn from_env_allows_in_memory_state_by_default() {
        // Default (no REQUIRE_EXTERNAL_STATE) keeps single-replica behaviour:
        // no Redis is fine, the guard does not trigger.
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("REDIS_URL", None),
            ],
            || {
                let cfg = AppConfig::from_env().expect("default (single-replica) config must be accepted");
                assert!(!cfg.require_external_state, "must default to false");
            },
        );
    }

    #[test]
    fn ensure_external_state_redis_ready_rejects_required_but_unusable() {
        // CN-7 startup guard: REQUIRE_EXTERNAL_STATE=true with a Redis that is not
        // usable for the state store (unreachable, or read-only / ACL-restricted →
        // the read/write probe returns false) must fail fast at startup.
        let err = ensure_external_state_redis_ready(true, false).unwrap_err().to_string();
        assert!(err.contains("REQUIRE_EXTERNAL_STATE"), "error was: {err}");
        assert!(err.contains("Redis is not usable"), "error was: {err}");
    }

    #[test]
    fn ensure_external_state_redis_ready_accepts_required_and_connected() {
        assert!(ensure_external_state_redis_ready(true, true).is_ok());
    }

    #[test]
    fn ensure_external_state_redis_ready_ignored_when_not_required() {
        // Single-replica: external state not required → a disconnected Redis is fine.
        assert!(ensure_external_state_redis_ready(false, false).is_ok());
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
            presence_redis_enabled: false,
            require_external_state: false,
            nats_url: None,
            nats_agent_url: None,
            nats_container_url: None,
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
            auth_sso: SsoConfig::default(),
            jwt_secret: SecretString::from("jwt-supersecret-value-min-32-chars!!".to_string()),
            bootstrap_admin_token: Some(SecretString::from("bootstrap-supersecret-value-min-32-chars".to_string())),
            allow_unprotected_admin_bootstrap: false,
            jwt_expiry_seconds: 900,
            environment: "development".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            dev_env_allowed_image_registries: Vec::new(),
            force_reset_legacy_sha256: false,
            llm_encryption_key: Some(SecretString::from("enc-key-supersecret".to_string())),
            container_anthropic_api_key: Some(SecretString::from("sk-ant-supersecret".to_string())),
            container_google_api_key: Some(SecretString::from("goog-supersecret".to_string())),
            container_openai_api_key: Some(SecretString::from("sk-openai-supersecret".to_string())),
            codex_default_model: "gpt-5.5".to_string(),
            llm_pricing: None,
            review_required_gates: None,
            compliance_export_interval_hours: 0,
            compliance_export_dir: None,
            analytics_retention_days: 0,
            run_retention_days: 0,
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
            allow_plaintext_host_nats: false,
            host_join_binary_base_url: None,
            cli_image_auto_update_enabled: false,
            cli_image_auto_update_interval_secs: 900,
            cli_image_prune_enabled: false,
            cli_image_claude_auto_build: false,
            cli_image_npm_registry: None,
            project_clone_worker_enabled: false,
            project_clone_image: None,
            project_clone_secret_root: None,
            project_clone_timeout_secs: 600,
            github_app_id: None,
            github_app_installation_id: None,
            github_app_private_key: None,
            github_app_repo: None,
            self_fix_pr_worker_enabled: true,
            self_fix_max_merge_attempts: 5,
            self_fix_review_deadline_secs: 604800,
            blocked_task_ttl_secs: 3600,
            job_queue_stale_lock_timeout_secs: 1800,
        };
        let dbg = format!("{cfg:?}");
        for needle in [
            "jwt-supersecret-value-min-32-chars!!",
            "bootstrap-supersecret-value-min-32-chars",
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

    #[test]
    fn dev_env_allowed_image_registries_parses_comma_separated_env() {
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("DEV_ENV_ALLOWED_IMAGE_REGISTRIES", Some("ghcr.io/myorg/, docker.io/ ,")),
            ],
            || {
                let cfg = AppConfig::from_env().expect("config with a comma list must load");
                // Trimmed, empty items dropped.
                assert_eq!(cfg.dev_env_allowed_image_registries, vec!["ghcr.io/myorg/", "docker.io/"]);
            },
        );

        // Unset -> empty (closed except built-in safe set).
        temp_env::with_vars(
            [
                ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
                ("REQUIRE_EXTERNAL_STATE", None),
                ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
                ("NATS_URL", None),
                ("DEV_ENV_ALLOWED_IMAGE_REGISTRIES", None),
            ],
            || {
                let cfg = AppConfig::from_env().expect("config without the var must load");
                assert!(cfg.dev_env_allowed_image_registries.is_empty());
            },
        );
    }

    #[test]
    fn production_requires_llm_encryption_key() {
        let base = [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("REQUIRE_EXTERNAL_STATE", None),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("NATS_URL", None),
        ];

        // Production with no LLM_ENCRYPTION_KEY -> rejected at boot (F020).
        temp_env::with_vars(
            base.iter()
                .copied()
                .chain([("ENVIRONMENT", Some("production")), ("LLM_ENCRYPTION_KEY", None)])
                .collect::<Vec<_>>(),
            || {
                let err = AppConfig::from_env().expect_err("production must require LLM_ENCRYPTION_KEY").to_string();
                assert!(err.contains("LLM_ENCRYPTION_KEY"), "error was: {err}");
            },
        );

        // Production with an empty LLM_ENCRYPTION_KEY -> also rejected.
        temp_env::with_vars(
            base.iter()
                .copied()
                .chain([("ENVIRONMENT", Some("production")), ("LLM_ENCRYPTION_KEY", Some("   "))])
                .collect::<Vec<_>>(),
            || {
                assert!(AppConfig::from_env().is_err(), "an empty key must be rejected in production");
            },
        );

        // Production with a real key -> loads.
        temp_env::with_vars(
            base.iter()
                .copied()
                .chain([("ENVIRONMENT", Some("production")), ("LLM_ENCRYPTION_KEY", Some("a-real-encryption-key"))])
                .collect::<Vec<_>>(),
            || {
                assert!(AppConfig::from_env().is_ok(), "production with a key must load");
            },
        );

        // Development with no key -> loads (the requirement is production-gated).
        temp_env::with_vars(
            base.iter()
                .copied()
                .chain([("ENVIRONMENT", Some("development")), ("LLM_ENCRYPTION_KEY", None)])
                .collect::<Vec<_>>(),
            || {
                assert!(AppConfig::from_env().is_ok(), "development must not require LLM_ENCRYPTION_KEY");
            },
        );
    }
}
