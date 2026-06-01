//! Container CLI auth proxy — PKCE-wrapped OAuth for Container CLIs that can
//! run the login flow inside an agent container but can't receive a browser
//! callback themselves.
//!
//! Ports the legacy TS `CliAuthProxyService`
//! (`server/src/modules/cli-auth-proxy/cli-auth-proxy.service.ts`) with the
//! Codex provider preset. Other Container CLIs (claude/gemini/opencode) don't
//! currently have an OAuth PKCE path in legacy — users copy-paste their file map
//! via the `cli-credentials` endpoints instead.
//!
//! Flow (manual callback mode, matching Codex):
//! 1. Client calls `authorize(user_id, "openai")` → PKCE code_verifier +
//!    state are stored (Redis if available, else process-local map with 5-min
//!    TTL), we return the provider authorization URL.
//! 2. User opens the URL, completes OAuth, lands on the provider's redirect
//!    URI (`http://localhost:1455/auth/callback`). Codex CLI displays the
//!    `code#state` — the user pastes it back into our UI.
//! 3. Client calls `complete_manual("openai", pasted_input)`. We parse the
//!    input (full URL / `code#state` / query string), look up the stored
//!    state, exchange the code at the token endpoint, pull the
//!    `chatgpt_account_id` out of the access-token JWT, and upsert an
//!    `auth.json` row in `user_cli_credentials` via `CliCredentialService`.
//!
//! State entries are single-use (deleted on retrieval) to prevent replay.

mod refresh_classifier;
pub use crate::domain::cli_auth_proxy::{
    CallbackMode, ProviderInfo, ProviderStatus, RefreshSummary, RevokedCliCredential,
};
pub(crate) use crate::domain::cli_auth_proxy::{
    CliAuthCredentialPayloadRead, CliAuthProxyPolicy, CliAuthTokenFileInput, TokenResponse,
    cli_auth_auth_json_from_str, cli_auth_authorize_response, cli_auth_authorize_url, cli_auth_callback_idp_error_html,
    cli_auth_callback_missing_params_html, cli_auth_callback_service_error_html, cli_auth_callback_success_html,
    cli_auth_connected_response, cli_auth_credential_decrypt_failed_reason, cli_auth_credential_payload_from_plain,
    cli_auth_credential_payload_invalid_reason, cli_auth_disconnected_response, cli_auth_encryption_key_missing_reason,
    cli_auth_providers_response, cli_auth_state_entry_from_payload, cli_auth_state_entry_payload,
    cli_auth_statuses_response, cli_auth_token_file_payload, cli_auth_token_files_from_plain,
    extract_chatgpt_account_id, parse_callback_input,
};
pub use refresh_classifier::{RefreshErrorKind, classify_refresh_failure};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use agentforge_core::{AppConfig, AppResult, CliToolKind, TenantScope, crypto};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use redis::AsyncCommands;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::RwLock;

/// serde adapter for `SecretString` — `secrecy` ships `Deserialize` but
/// deliberately not `Serialize` so accidental JSON/log round-trips stay opt-in.
/// `StateEntry` needs both sides (Redis round-trip), so we opt in here.
mod secret_string_serde {
    use secrecy::{ExposeSecret, SecretString};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(s: &SecretString, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(s.expose_secret())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<SecretString, D::Error> {
        String::deserialize(de).map(SecretString::from)
    }
}

use crate::repositories::credential::cli::{CliCredentialRepository, EncryptedWithRevocation};
use agentforge_infra::RedisClient;

const STATE_TTL_SECS: u64 = 300;

/// Provider config — baked in for Codex, overridable via `AppConfig` for admin
/// custom OAuth apps.
#[derive(Debug, Clone)]
pub struct CliAuthProxyProvider {
    pub name: String,
    pub display_name: String,
    pub cli_tool: String,
    pub client_id: String,
    /// Confidential-client secret. `SecretString` so `Debug` on the provider
    /// doesn't leak the value into `tracing::debug!(?provider)`; reach the
    /// bytes via `.expose_secret()` when POSTing the token form.
    pub client_secret: Option<SecretString>,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub redirect_uri: String,
    pub scope: String,
    /// `manual` (user pastes callback URL) vs `server` (backend hosts the
    /// redirect). Only `manual` is wired for now.
    pub callback_mode: CallbackMode,
}

/// Build the deployment's CLI auth provider registry from static defaults plus
/// operator overrides.
pub fn resolve_providers(config: &AppConfig) -> Vec<CliAuthProxyProvider> {
    let mut openai = CliAuthProxyProvider {
        name: "openai".to_string(),
        display_name: "OpenAI (Codex)".to_string(),
        cli_tool: "codex".to_string(),
        client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_string(),
        client_secret: None,
        auth_endpoint: "https://auth.openai.com/oauth/authorize".to_string(),
        token_endpoint: "https://auth.openai.com/oauth/token".to_string(),
        redirect_uri: "http://localhost:1455/auth/callback".to_string(),
        scope: "openid profile email offline_access".to_string(),
        callback_mode: CallbackMode::Manual,
    };
    if let Some(cid) = config.cli_auth_proxy_openai_client_id.as_deref().filter(|s| !s.is_empty()) {
        openai.client_id = cid.to_string();
        openai.client_secret = config
            .cli_auth_proxy_openai_client_secret
            .as_ref()
            .map(|s| s.expose_secret().to_string())
            .filter(|s| !s.is_empty())
            .map(SecretString::from);
        if let Some(ep) = config.cli_auth_proxy_openai_auth_endpoint.as_deref().filter(|s| !s.is_empty()) {
            openai.auth_endpoint = ep.to_string();
        }
        if let Some(ep) = config.cli_auth_proxy_openai_token_endpoint.as_deref().filter(|s| !s.is_empty()) {
            openai.token_endpoint = ep.to_string();
        }
        if let Some(app_url) = config.app_url.as_deref().filter(|s| !s.is_empty()) {
            openai.redirect_uri = format!("{}/api/v1/cli-auth-proxy/openai/callback", app_url.trim_end_matches('/'));
            openai.callback_mode = CallbackMode::Server;
        }
    }
    vec![openai]
}

/// Outcome of one refresh attempt — dispatched by `refresh_stale`.
#[derive(Debug)]
enum RefreshOutcome {
    Refreshed,
    /// Refresh token rejected — caller bumps per-row fail counter and
    /// revokes at threshold.
    InvalidGrant,
    /// OAuth app rejected — caller logs at error! level and emits a
    /// metric but NEVER touches the user row.
    InvalidClient,
    /// Transient network / 5xx / unknown code — caller counts as failed
    /// and retries on the next sweep.
    OtherFailure(String),
}

struct CredentialConnectionStatus {
    connected: bool,
    last_refresh: Option<String>,
    revoked_at: Option<String>,
    revoke_reason: Option<String>,
    refresh_fail_count: i32,
}

fn credential_connection_status(
    scope: &TenantScope,
    cli_tool: &str,
    row: Option<EncryptedWithRevocation>,
    encryption_key: Option<&[u8; 32]>,
) -> CredentialConnectionStatus {
    let Some((enc, revoked_at, reason, count)) = row else {
        return CredentialConnectionStatus {
            connected: false,
            last_refresh: None,
            revoked_at: None,
            revoke_reason: None,
            refresh_fail_count: 0,
        };
    };

    let mut last_refresh = None;
    let mut unusable_reason = None;

    if let Some(key) = encryption_key {
        match crypto::decrypt_base64(key, &enc) {
            Ok(plain) => match cli_auth_credential_payload_from_plain(&plain) {
                CliAuthCredentialPayloadRead::Usable { last_refresh: refresh } => {
                    last_refresh = refresh;
                }
                CliAuthCredentialPayloadRead::InvalidPayload { error } => {
                    tracing::warn!(
                        error = %error,
                        user_id = %scope.user_id().as_uuid(),
                        cli_tool,
                        "stored Container CLI credentials decrypted but are not a file-map JSON object"
                    );
                    unusable_reason = Some(cli_auth_credential_payload_invalid_reason().to_string());
                }
            },
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    user_id = %scope.user_id().as_uuid(),
                    cli_tool,
                    "stored Container CLI credentials cannot be decrypted for status"
                );
                unusable_reason = Some(cli_auth_credential_decrypt_failed_reason().to_string());
            }
        }
    } else {
        unusable_reason = Some(cli_auth_encryption_key_missing_reason().to_string());
    }

    let revoked_at = revoked_at.map(|d| d.to_rfc3339());
    let connected = revoked_at.is_none() && unusable_reason.is_none();
    CredentialConnectionStatus {
        connected,
        last_refresh,
        revoked_at,
        revoke_reason: reason.or(unusable_reason),
        refresh_fail_count: count,
    }
}

/// PKCE + provider hint stored against the OAuth `state` value while the user
/// completes the browser flow.
///
/// `code_verifier` is a PKCE secret — its `SecretString` wrapper redacts
/// `Debug` and forces explicit `.expose_secret()` at use sites. The
/// `secret_string_serde` adapter opts the field into both sides of serde so
/// the Redis round-trip still works.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
struct StateEntry {
    #[serde(with = "secret_string_serde")]
    code_verifier: SecretString,
    provider: String,
    user_id: uuid::Uuid,
}

/// In-memory PKCE state store — used when Redis is not configured. Must live
/// on `AppState` (shared across requests) so the `authorize` put and the
/// later `complete_manual` / `server_callback` take land on the same store
/// instance. A per-request `CliAuthProxyService::new` would construct
/// disjoint stores and every non-Redis callback would fail with
/// "invalid or expired OAuth state". See `AppState.cli_auth_memory_store`.
/// A background sweep is unnecessary — `take` both reads and removes;
/// stale entries linger at most until their TTL expires on read.
#[derive(Default)]
pub struct MemoryStateStore {
    inner: AsyncMutex<HashMap<String, (StateEntry, SystemTime)>>,
}

impl MemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    async fn put(&self, state: &str, entry: StateEntry, ttl: Duration) {
        let mut g = self.inner.lock().await;
        g.insert(state.to_string(), (entry, SystemTime::now() + ttl));
    }

    async fn take(&self, state: &str) -> Option<StateEntry> {
        let mut g = self.inner.lock().await;
        let (entry, expires) = g.remove(state)?;
        if expires < SystemTime::now() {
            return None;
        }
        Some(entry)
    }
}

/// Pick the state store mode at construction time. Redis mode propagates
/// errors as `ErrorKind::Internal`; memory mode cannot fail.
///
/// Operator intent is captured by the variant itself: a caller building
/// `Redis(...)` has declared "I configured Redis and expect it to work".
/// A subsequent outage (connection dropped, SET_EX errors) surfaces as a 500
/// response rather than silently landing state on a single replica's memory.
pub enum StateStore {
    Redis(Arc<RwLock<RedisClient>>),
    Memory(Arc<MemoryStateStore>),
}

impl StateStore {
    async fn put(&self, state: &str, entry: StateEntry) -> AppResult<()> {
        match self {
            Self::Redis(client) => {
                let key = redis_state_key(state);
                let value = cli_auth_state_entry_payload(&entry)?;
                let mut guard = client.write().await;
                let conn = guard.connection_mut().ok_or_else(CliAuthProxyPolicy::redis_connection_unavailable)?;
                let _: () = conn.set_ex(&key, &value, STATE_TTL_SECS).await.map_err(|err| {
                    tracing::warn!(error = %err, "OAuth state Redis SET_EX failed — propagating as Internal");
                    CliAuthProxyPolicy::redis_set_failed(err)
                })?;
                Ok(())
            }
            Self::Memory(mem) => {
                mem.put(state, entry, Duration::from_secs(STATE_TTL_SECS)).await;
                Ok(())
            }
        }
    }

    async fn take(&self, state: &str) -> AppResult<Option<StateEntry>> {
        match self {
            Self::Redis(client) => {
                let key = redis_state_key(state);
                let mut guard = client.write().await;
                let conn = guard.connection_mut().ok_or_else(CliAuthProxyPolicy::redis_connection_unavailable)?;
                let raw: Option<String> = redis::cmd("GETDEL").arg(&key).query_async(conn).await.map_err(|err| {
                    tracing::warn!(error = %err, "OAuth state Redis GETDEL failed — propagating as Internal");
                    CliAuthProxyPolicy::redis_getdel_failed(err)
                })?;
                match raw {
                    Some(value) => Ok(Some(cli_auth_state_entry_from_payload::<StateEntry>(&value)?)),
                    None => Ok(None),
                }
            }
            Self::Memory(mem) => Ok(mem.take(state).await),
        }
    }
}

pub struct CliAuthProxyService {
    providers: HashMap<String, CliAuthProxyProvider>,
    cli_creds: CliCredentialRepository,
    encryption_key: Option<[u8; 32]>,
    store: StateStore,
    http: reqwest::Client,
    revoke_threshold: i32,
}

impl CliAuthProxyService {
    pub fn from_pool_and_app_config(
        pool: PgPool,
        config: &AppConfig,
        encryption_key: Option<[u8; 32]>,
        redis: Arc<RwLock<RedisClient>>,
        memory_store: Arc<MemoryStateStore>,
    ) -> Self {
        Self::from_app_config(config, CliCredentialRepository::new(pool), encryption_key, redis, memory_store)
    }

    /// Build the deployment-scoped service wiring used by HTTP routes and
    /// workers. Runtime concerns stay here so handlers don't know how the
    /// provider registry, state-store backend, or revoke threshold are chosen.
    pub fn from_app_config(
        config: &AppConfig,
        cli_creds: CliCredentialRepository,
        encryption_key: Option<[u8; 32]>,
        redis: Arc<RwLock<RedisClient>>,
        memory_store: Arc<MemoryStateStore>,
    ) -> Self {
        let store =
            if config.redis_url.is_some() { StateStore::Redis(redis) } else { StateStore::Memory(memory_store) };
        Self::new(resolve_providers(config), cli_creds, encryption_key, store, config.cli_auth_proxy_revoke_threshold)
    }

    /// Constructor used by per-request handlers. The caller picks the
    /// `StateStore` variant based on whether Redis is configured — in
    /// multi-replica deployments the `Redis` variant MUST be used so that
    /// authorize-put and callback-take land on the same shared backend.
    pub fn new(
        providers: Vec<CliAuthProxyProvider>,
        cli_creds: CliCredentialRepository,
        encryption_key: Option<[u8; 32]>,
        store: StateStore,
        revoke_threshold: i32,
    ) -> Self {
        let providers = providers.into_iter().map(|p| (p.name.clone(), p)).collect();
        Self {
            providers,
            cli_creds,
            encryption_key,
            store,
            http: reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("reqwest builder"),
            revoke_threshold,
        }
    }

    /// List providers available on this deployment. UI uses this to decide
    /// whether to render the "Connect codex" button.
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|p| ProviderInfo {
                name: p.name.clone(),
                display_name: p.display_name.clone(),
                cli_tool: p.cli_tool.clone(),
                callback_mode: p.callback_mode,
            })
            .collect()
    }

    /// Step 1 of the flow. Generate PKCE pair + state, store verifier against
    /// state (5-min TTL), return the provider authorization URL with the
    /// S256-hashed challenge.
    pub async fn authorize(&self, scope: &TenantScope, provider_name: &str) -> AppResult<String> {
        let provider = self.require_provider(provider_name)?;

        let (verifier, challenge) = generate_pkce();
        let state = generate_state();

        self.store_state(
            &state,
            StateEntry {
                code_verifier: SecretString::from(verifier),
                provider: provider_name.to_string(),
                user_id: scope.user_id().as_uuid(),
            },
        )
        .await?;

        let params = vec![
            ("response_type", "code"),
            ("client_id", provider.client_id.as_str()),
            ("redirect_uri", provider.redirect_uri.as_str()),
            ("scope", provider.scope.as_str()),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];
        cli_auth_authorize_url(&provider.auth_endpoint, &params)
    }

    /// Per-user connection status. `connected=true` only when a non-revoked row
    /// exists and the current encryption key can still decrypt it. A row that
    /// merely exists but cannot be decrypted would fail container credential
    /// injection, so the UI must show a reconnect path instead of "Connected".
    pub async fn status(&self, scope: &TenantScope) -> AppResult<Vec<ProviderStatus>> {
        let mut out = Vec::with_capacity(self.providers.len());
        for provider in self.providers.values() {
            let row = self.cli_creds.find_encrypted_with_revocation(scope, &provider.cli_tool).await?;
            let status = credential_connection_status(scope, &provider.cli_tool, row, self.encryption_key.as_ref());
            out.push(ProviderStatus {
                provider: provider.name.clone(),
                display_name: provider.display_name.clone(),
                cli_tool: provider.cli_tool.clone(),
                connected: status.connected,
                last_refresh: status.last_refresh,
                callback_mode: provider.callback_mode,
                revoked_at: status.revoked_at,
                revoke_reason: status.revoke_reason,
                refresh_fail_count: status.refresh_fail_count,
            });
        }
        Ok(out)
    }

    /// Step 3 of the flow — the user-pasted callback input. Accepts:
    /// - full URL (`http://localhost:1455/auth/callback?code=...&state=...`)
    /// - `code#state`
    /// - query string (`code=...&state=...`)
    pub async fn complete_manual(&self, scope: &TenantScope, provider_name: &str, input: &str) -> AppResult<()> {
        let (code, state) =
            parse_callback_input(input).ok_or_else(CliAuthProxyPolicy::invalid_manual_callback_input)?;

        let entry = self.take_state(&state).await?.ok_or_else(CliAuthProxyPolicy::invalid_or_expired_manual_state)?;
        if entry.provider != provider_name {
            return Err(CliAuthProxyPolicy::provider_mismatch(&entry.provider, provider_name).into());
        }
        if entry.user_id != scope.user_id().as_uuid() {
            return Err(CliAuthProxyPolicy::state_user_mismatch().into());
        }

        let provider = self.require_provider(provider_name)?;
        let tokens = self.exchange_code(provider, &code, entry.code_verifier.expose_secret()).await?;
        self.store_tokens(scope, provider, &tokens).await
    }

    /// Disconnect — idempotent delete of the stored credentials row.
    pub async fn disconnect(&self, scope: &TenantScope, provider_name: &str) -> AppResult<()> {
        let provider = self.require_provider(provider_name)?;
        self.cli_creds.delete(scope, &provider.cli_tool).await
    }

    /// Server-callback handler — the IdP redirected to our own endpoint
    /// instead of `localhost:1455`. The browser isn't authenticated via our
    /// JWT here (that lives in the other tab), so we trust `state` to identify
    /// the user: the `StateEntry` stored at authorize-time carries `user_id`,
    /// which we upsert against. Matches legacy `handleCallback`.
    pub async fn handle_server_callback(&self, provider_name: &str, code: &str, state: &str) -> AppResult<()> {
        let entry = self.take_state(state).await?.ok_or_else(CliAuthProxyPolicy::invalid_or_expired_state)?;
        if entry.provider != provider_name {
            return Err(CliAuthProxyPolicy::provider_mismatch(&entry.provider, provider_name).into());
        }
        let provider = self.require_provider(provider_name)?;
        let tokens = self.exchange_code(provider, code, entry.code_verifier.expose_secret()).await?;
        self.store_tokens_by_user_id(entry.user_id, provider, &tokens).await
    }

    /// Iterate every stored credential row across every user and refresh any
    /// whose `last_refresh` is older than `threshold`. Returns
    /// `(refreshed, failed, eligible)`. Matches the legacy TS
    /// `cli-auth-proxy-refresh.worker.ts` semantics: worker-scoped (no tenant
    /// context), per-provider iteration, best-effort per entry — one user's
    /// failed refresh never blocks another.
    pub async fn refresh_stale(&self, threshold: Duration) -> RefreshSummary {
        let mut summary = RefreshSummary::default();
        let Some(key) = self.encryption_key else {
            tracing::debug!("Refresh skipped — no encryption key configured");
            return summary;
        };
        let now = chrono::Utc::now();

        for provider in self.providers.values() {
            let rows = match self.cli_creds.find_all_active_by_cli_tool(&provider.cli_tool).await {
                Ok(rs) => rs,
                Err(err) => {
                    tracing::warn!(error = ?err, cli_tool = %provider.cli_tool, "refresh scan failed");
                    continue;
                }
            };
            for (user_id, encrypted) in rows {
                match needs_refresh(&key, &encrypted, now, threshold) {
                    Ok(NeedsRefresh::Stale { refresh_token }) => {
                        summary.eligible += 1;
                        match self.refresh_single(provider, user_id, refresh_token.expose_secret()).await {
                            Ok(RefreshOutcome::Refreshed) => summary.refreshed += 1,
                            Ok(RefreshOutcome::InvalidGrant) => {
                                summary.invalid_grant += 1;
                                match self
                                    .cli_creds
                                    .bump_fail_count_or_revoke(
                                        user_id,
                                        &provider.cli_tool,
                                        "invalid_grant",
                                        self.revoke_threshold,
                                    )
                                    .await
                                {
                                    Ok(Some((count, Some(revoked_at)))) => {
                                        tracing::warn!(
                                            %user_id, cli_tool = %provider.cli_tool, %count, ?revoked_at,
                                            "credential revoked after invalid_grant threshold"
                                        );
                                        summary.revoked_credentials.push(RevokedCliCredential {
                                            user_id,
                                            cli_tool: provider.cli_tool.clone(),
                                            reason: "invalid_grant".to_string(),
                                            revoked_at,
                                        });
                                        metrics::counter!(
                                            "credential_refresh_errors_total",
                                            "cli_tool" => provider.cli_tool.clone(),
                                            "reason" => "invalid_grant_revoked",
                                        )
                                        .increment(1);
                                    }
                                    Ok(Some((count, None))) => {
                                        tracing::info!(
                                            %user_id, cli_tool = %provider.cli_tool, %count,
                                            "invalid_grant — fail count bumped, below threshold"
                                        );
                                        metrics::counter!(
                                            "credential_refresh_errors_total",
                                            "cli_tool" => provider.cli_tool.clone(),
                                            "reason" => "invalid_grant",
                                        )
                                        .increment(1);
                                    }
                                    Ok(None) => {
                                        tracing::debug!(
                                            %user_id, cli_tool = %provider.cli_tool,
                                            "row already revoked or missing"
                                        );
                                    }
                                    Err(err) => {
                                        tracing::error!(
                                            error = ?err, %user_id, cli_tool = %provider.cli_tool,
                                            "bump_fail_count_or_revoke failed"
                                        );
                                    }
                                }
                            }
                            Ok(RefreshOutcome::InvalidClient) => {
                                summary.invalid_client += 1;
                                tracing::error!(
                                    cli_tool = %provider.cli_tool, %user_id,
                                    "OAuth app rejected by IdP — operator must investigate client_id/secret"
                                );
                                metrics::counter!(
                                    "credential_refresh_errors_total",
                                    "cli_tool" => provider.cli_tool.clone(),
                                    "reason" => "invalid_client",
                                )
                                .increment(1);
                            }
                            Ok(RefreshOutcome::OtherFailure(msg)) => {
                                summary.failed += 1;
                                tracing::warn!(
                                    %user_id, cli_tool = %provider.cli_tool, %msg,
                                    "refresh failed (transient or unknown)"
                                );
                                metrics::counter!(
                                    "credential_refresh_errors_total",
                                    "cli_tool" => provider.cli_tool.clone(),
                                    "reason" => "transient",
                                )
                                .increment(1);
                            }
                            Err(err) => {
                                summary.failed += 1;
                                tracing::warn!(
                                    error = ?err, %user_id, cli_tool = %provider.cli_tool,
                                    "refresh_single returned internal error"
                                );
                            }
                        }
                    }
                    Ok(NeedsRefresh::Fresh) => {}
                    Ok(NeedsRefresh::NoRefreshToken) => {
                        tracing::debug!(%user_id, cli_tool = %provider.cli_tool, "no refresh_token stored — skipping");
                    }
                    Err(err) => {
                        tracing::warn!(error = ?err, %user_id, cli_tool = %provider.cli_tool, "refresh eligibility check failed");
                    }
                }
            }
        }
        summary
    }

    /// POST the `refresh_token` grant and persist the returned tokens back
    /// to the user's row. Preserves the existing `refresh_token` when the
    /// provider doesn't return a new one (OpenAI behaviour).
    ///
    /// On failure, classifies the response via RFC 6749 §5.2 error codes so
    /// the caller can decide between revoking the user row (`InvalidGrant`),
    /// paging the operator (`InvalidClient`), or retrying (`OtherFailure`).
    #[tracing::instrument(skip(self, refresh_token), fields(cli_tool = %provider.cli_tool, %user_id))]
    async fn refresh_single(
        &self,
        provider: &CliAuthProxyProvider,
        user_id: uuid::Uuid,
        refresh_token: &str,
    ) -> AppResult<RefreshOutcome> {
        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("client_id", provider.client_id.as_str()),
            ("refresh_token", refresh_token),
        ];
        if let Some(sec) = provider.client_secret.as_ref() {
            form.push(("client_secret", sec.expose_secret()));
        }
        let resp = match self.http.post(&provider.token_endpoint).form(&form).send().await {
            Ok(r) => r,
            Err(err) => return Ok(RefreshOutcome::OtherFailure(format!("network: {err}"))),
        };
        if !resp.status().is_success() {
            return Ok(match classify_refresh_failure(resp).await {
                RefreshErrorKind::InvalidGrant => RefreshOutcome::InvalidGrant,
                RefreshErrorKind::InvalidClient => RefreshOutcome::InvalidClient,
                RefreshErrorKind::OtherOauthError(code) => RefreshOutcome::OtherFailure(format!("oauth:{code}")),
                RefreshErrorKind::Transient(msg) => RefreshOutcome::OtherFailure(msg),
            });
        }
        let mut tokens: TokenResponse = resp.json().await.map_err(CliAuthProxyPolicy::refresh_invalid_json)?;
        // Preserve existing refresh_token if the provider didn't issue a new one.
        if tokens.refresh_token.is_none() {
            tokens.refresh_token = Some(SecretString::from(refresh_token.to_string()));
        }
        self.store_tokens_by_user_id(user_id, provider, &tokens).await?;
        self.cli_creds.reset_fail_count_on_success(user_id, &provider.cli_tool).await?;
        Ok(RefreshOutcome::Refreshed)
    }

    async fn store_tokens_by_user_id(
        &self,
        user_id: uuid::Uuid,
        provider: &CliAuthProxyProvider,
        tokens: &TokenResponse,
    ) -> AppResult<()> {
        let key = self.encryption_key.as_ref().ok_or_else(CliAuthProxyPolicy::missing_refresh_storage_key)?;
        let account_id =
            tokens.account_id.clone().or_else(|| extract_chatgpt_account_id(tokens.access_token.expose_secret()));
        let payload = cli_auth_token_file_payload(CliAuthTokenFileInput {
            id_token: tokens.id_token.as_ref().map(|value| value.expose_secret()),
            access_token: tokens.access_token.expose_secret(),
            refresh_token: tokens.refresh_token.as_ref().map(|value| value.expose_secret()),
            account_id: account_id.as_deref(),
            last_refresh: chrono::Utc::now(),
        });
        let ciphertext =
            crypto::encrypt_base64(key, &payload).map_err(CliAuthProxyPolicy::encrypt_refreshed_tokens_failed)?;
        self.cli_creds.upsert_encrypted_by_user_id(user_id, &provider.cli_tool, &ciphertext).await
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn require_provider(&self, name: &str) -> AppResult<&CliAuthProxyProvider> {
        self.providers.get(name).ok_or_else(|| CliAuthProxyPolicy::unknown_provider(name).into())
    }

    async fn store_state(&self, state: &str, entry: StateEntry) -> AppResult<()> {
        self.store.put(state, entry).await
    }

    async fn take_state(&self, state: &str) -> AppResult<Option<StateEntry>> {
        self.store.take(state).await
    }

    async fn exchange_code(
        &self,
        provider: &CliAuthProxyProvider,
        code: &str,
        verifier: &str,
    ) -> AppResult<TokenResponse> {
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", provider.redirect_uri.as_str()),
            ("client_id", provider.client_id.as_str()),
            ("code_verifier", verifier),
        ];
        if let Some(sec) = provider.client_secret.as_ref() {
            form.push(("client_secret", sec.expose_secret()));
        }
        let resp = self
            .http
            .post(&provider.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(CliAuthProxyPolicy::token_exchange_request_failed)?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliAuthProxyPolicy::token_exchange_failed(status, &body).into());
        }
        let tokens = resp.json::<TokenResponse>().await.map_err(CliAuthProxyPolicy::token_exchange_invalid_json)?;
        Ok(tokens)
    }

    async fn store_tokens(
        &self,
        scope: &TenantScope,
        provider: &CliAuthProxyProvider,
        tokens: &TokenResponse,
    ) -> AppResult<()> {
        let key = self.encryption_key.as_ref().ok_or_else(CliAuthProxyPolicy::missing_token_storage_key)?;

        // Codex: pull chatgpt_account_id out of the access-token JWT (no
        // signature check — the provider already signed it and we only care
        // that the Container CLI sees the same claim it would have got via the
        // native `codex login` flow).
        let account_id =
            tokens.account_id.clone().or_else(|| extract_chatgpt_account_id(tokens.access_token.expose_secret()));

        let payload = cli_auth_token_file_payload(CliAuthTokenFileInput {
            id_token: tokens.id_token.as_ref().map(|value| value.expose_secret()),
            access_token: tokens.access_token.expose_secret(),
            refresh_token: tokens.refresh_token.as_ref().map(|value| value.expose_secret()),
            account_id: account_id.as_deref(),
            last_refresh: chrono::Utc::now(),
        });
        let ciphertext = crypto::encrypt_base64(key, &payload).map_err(CliAuthProxyPolicy::encrypt_tokens_failed)?;
        self.cli_creds.upsert_encrypted(scope, &provider.cli_tool, &ciphertext).await
    }
}

enum NeedsRefresh {
    Stale { refresh_token: SecretString },
    Fresh,
    NoRefreshToken,
}

/// Parse the stored ciphertext, decrypt, pull `last_refresh` + `refresh_token`
/// out of the embedded `auth.json`, and decide whether the entry is eligible
/// for a refresh call.
fn needs_refresh(
    key: &[u8; 32],
    ciphertext: &str,
    now: chrono::DateTime<chrono::Utc>,
    threshold: Duration,
) -> AppResult<NeedsRefresh> {
    let plain = crypto::decrypt_base64(key, ciphertext).map_err(CliAuthProxyPolicy::decrypt_failed)?;
    let files = cli_auth_token_files_from_plain(&plain)?;
    let auth_json_str = match files.get("auth.json").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Ok(NeedsRefresh::NoRefreshToken),
    };
    let auth = cli_auth_auth_json_from_str(auth_json_str)?;
    let refresh_token = match auth.pointer("/tokens/refresh_token").and_then(|v| v.as_str()) {
        Some(s) => SecretString::from(s.to_string()),
        None => return Ok(NeedsRefresh::NoRefreshToken),
    };
    let last_refresh = auth
        .get("last_refresh")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let age = last_refresh.map(|t| now.signed_duration_since(t));
    let stale = match age {
        Some(age) if age.to_std().map(|d| d < threshold).unwrap_or(false) => false,
        // Missing last_refresh, unparseable timestamp, or future timestamp
        // (clock skew) → treat as stale so we reconcile rather than linger.
        _ => true,
    };
    if stale { Ok(NeedsRefresh::Stale { refresh_token }) } else { Ok(NeedsRefresh::Fresh) }
}

fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn redis_state_key(state: &str) -> String {
    format!("cli-auth-proxy:state:{state}")
}

/// Describe + prime metrics emitted by this module. Call once at startup
/// so `/metrics` returns the counters even before the first refresh sweep,
/// matching the pattern in `agentforge_jobs::register_metrics`.
pub fn register_cli_auth_proxy_metrics() {
    metrics::describe_counter!(
        "credential_refresh_errors_total",
        "OAuth refresh failures bucketed by Container CLI tool and RFC 6749 error code"
    );
    // Prime the well-known (cli_tool, reason) combinations so Grafana
    // dashboards don't show "no data" on a fresh cluster.
    for reason in ["invalid_grant", "invalid_grant_revoked", "invalid_client", "transient"] {
        for cli_tool in CliToolKind::ALL.map(CliToolKind::as_str) {
            metrics::counter!(
                "credential_refresh_errors_total",
                "cli_tool" => cli_tool,
                "reason" => reason,
            )
            .increment(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_challenges_deterministically() {
        let (verifier, challenge) = generate_pkce();
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let expect = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(expect, challenge);
    }

    #[test]
    fn state_is_hex_64_chars() {
        let s = generate_state();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parse_callback_url_format() {
        let url = "http://localhost:1455/auth/callback?code=abc&state=xyz";
        assert_eq!(parse_callback_input(url), Some(("abc".into(), "xyz".into())));
    }

    #[test]
    fn parse_callback_hash_format() {
        assert_eq!(parse_callback_input("abc#xyz"), Some(("abc".into(), "xyz".into())));
    }

    #[test]
    fn parse_callback_query_string() {
        assert_eq!(parse_callback_input("code=abc&state=xyz"), Some(("abc".into(), "xyz".into())));
        assert_eq!(parse_callback_input("?code=abc&state=xyz"), Some(("abc".into(), "xyz".into())));
    }

    #[test]
    fn parse_callback_query_string_percent_decodes() {
        // `+` in the raw code would encode as `%2B`; the token-exchange call
        // must send the decoded `+` or OpenAI rejects with `invalid_grant`.
        assert_eq!(parse_callback_input("code=abc%2Bdef&state=x%2Fy"), Some(("abc+def".into(), "x/y".into())));
    }

    #[test]
    fn parse_callback_rejects_empty_or_partial() {
        assert!(parse_callback_input("").is_none());
        assert!(parse_callback_input("code=abc").is_none());
    }

    #[test]
    fn extract_account_id_from_sample_jwt() {
        // header.payload.signature with payload = {"https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"}}
        let payload = serde_json::json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acct_123" }
        });
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let token = format!("header.{payload_b64}.sig");
        assert_eq!(extract_chatgpt_account_id(&token).as_deref(), Some("acct_123"));
    }

    #[test]
    fn extract_account_id_returns_none_for_malformed() {
        assert!(extract_chatgpt_account_id("not-a-jwt").is_none());
        assert!(extract_chatgpt_account_id("onepart").is_none());
    }

    fn fake_cipher(key: &[u8; 32], auth_json: &serde_json::Value) -> String {
        let files = serde_json::json!({ "auth.json": auth_json.to_string() });
        crypto::encrypt_base64(key, &files.to_string()).unwrap()
    }

    fn status_scope() -> TenantScope {
        crate::test_support::tenant_scope()
    }

    #[test]
    fn credential_status_marks_decryptable_active_row_connected() {
        let key = [0x42; 32];
        let auth = serde_json::json!({
            "tokens": { "access_token": "at", "refresh_token": "rt" },
            "last_refresh": "2026-04-25T00:00:00Z",
        });
        let status = credential_connection_status(
            &status_scope(),
            "codex",
            Some((fake_cipher(&key, &auth), None, None, 0)),
            Some(&key),
        );

        assert!(status.connected, "decryptable active row should be connected");
        assert_eq!(status.last_refresh.as_deref(), Some("2026-04-25T00:00:00Z"));
        assert!(status.revoke_reason.is_none());
    }

    #[test]
    fn credential_status_marks_undecryptable_row_disconnected() {
        let good_key = [0x42; 32];
        let wrong_key = [0x99; 32];
        let auth = serde_json::json!({
            "tokens": { "access_token": "at", "refresh_token": "rt" },
            "last_refresh": "2026-04-25T00:00:00Z",
        });
        let status = credential_connection_status(
            &status_scope(),
            "codex",
            Some((fake_cipher(&wrong_key, &auth), None, None, 0)),
            Some(&good_key),
        );

        assert!(!status.connected, "undecryptable row must not be shown as connected");
        assert_eq!(status.revoke_reason.as_deref(), Some("credential_decrypt_failed"));
        assert!(status.revoked_at.is_none(), "decrypt failure is not an OAuth revocation");
    }

    #[test]
    fn token_response_debug_redacts_secret_fields() {
        // Guards against a future `tracing::debug!(?tokens)` exfiltrating
        // OAuth material. secrecy's Debug emits `SecretBox<…>([REDACTED])`.
        let tokens = TokenResponse {
            id_token: Some(SecretString::from("id-supersecret".to_string())),
            access_token: SecretString::from("at-supersecret".to_string()),
            refresh_token: Some(SecretString::from("rt-supersecret".to_string())),
            expires_in: Some(3600),
            account_id: Some("acct-public".to_string()),
        };
        let dbg = format!("{tokens:?}");
        for needle in ["id-supersecret", "at-supersecret", "rt-supersecret"] {
            assert!(!dbg.contains(needle), "Debug leaked {needle:?}: {dbg}");
        }
        // `account_id` is not a secret and stays visible for debugging.
        assert!(dbg.contains("acct-public"), "account_id should remain visible: {dbg}");
    }

    #[test]
    fn state_entry_debug_redacts_code_verifier() {
        let entry = StateEntry {
            code_verifier: SecretString::from("pkce-supersecret".to_string()),
            provider: "openai".to_string(),
            user_id: uuid::Uuid::nil(),
        };
        let dbg = format!("{entry:?}");
        assert!(!dbg.contains("pkce-supersecret"), "Debug leaked PKCE verifier: {dbg}");
        // Non-secret fields stay visible.
        assert!(dbg.contains("openai"));
    }

    #[test]
    fn state_entry_serde_roundtrips_through_json() {
        // `StateEntry` is serialised to Redis and deserialised on take — the
        // `secret_string_serde` adapter must preserve the inner bytes across
        // the round-trip even though Debug redacts them.
        let original = StateEntry {
            code_verifier: SecretString::from("verifier-xyz".to_string()),
            provider: "openai".to_string(),
            user_id: uuid::Uuid::nil(),
        };
        let json = serde_json::to_string(&original).expect("serialise");
        let back: StateEntry = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.code_verifier.expose_secret(), "verifier-xyz");
        assert_eq!(back.provider, "openai");
    }

    #[test]
    fn needs_refresh_marks_old_entries_stale() {
        let key = [11u8; 32];
        let old = chrono::Utc::now() - chrono::Duration::hours(5);
        let auth = serde_json::json!({
            "tokens": { "refresh_token": "rt-abc", "access_token": "at" },
            "last_refresh": old.to_rfc3339(),
        });
        let ct = fake_cipher(&key, &auth);
        match needs_refresh(&key, &ct, chrono::Utc::now(), Duration::from_secs(3 * 3600)).unwrap() {
            NeedsRefresh::Stale { refresh_token } => assert_eq!(refresh_token.expose_secret(), "rt-abc"),
            _ => panic!("expected Stale"),
        }
    }

    #[test]
    fn needs_refresh_keeps_fresh_entries() {
        let key = [11u8; 32];
        let recent = chrono::Utc::now() - chrono::Duration::minutes(5);
        let auth = serde_json::json!({
            "tokens": { "refresh_token": "rt", "access_token": "at" },
            "last_refresh": recent.to_rfc3339(),
        });
        let ct = fake_cipher(&key, &auth);
        assert!(matches!(
            needs_refresh(&key, &ct, chrono::Utc::now(), Duration::from_secs(3 * 3600)).unwrap(),
            NeedsRefresh::Fresh
        ));
    }

    #[test]
    fn needs_refresh_skips_entries_without_refresh_token() {
        let key = [11u8; 32];
        let auth = serde_json::json!({ "tokens": { "access_token": "at" }, "last_refresh": "2026-04-01T00:00:00Z" });
        let ct = fake_cipher(&key, &auth);
        assert!(matches!(
            needs_refresh(&key, &ct, chrono::Utc::now(), Duration::from_secs(3 * 3600)).unwrap(),
            NeedsRefresh::NoRefreshToken
        ));
    }

    #[tokio::test]
    async fn memory_store_expires_entries() {
        let store = MemoryStateStore::default();
        let entry = StateEntry {
            code_verifier: SecretString::from("v".to_string()),
            provider: "openai".into(),
            user_id: uuid::Uuid::new_v4(),
        };
        store.put("s1", entry.clone(), Duration::from_millis(0)).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(store.take("s1").await.is_none());
    }

    #[tokio::test]
    async fn memory_store_take_is_single_use() {
        let store = MemoryStateStore::default();
        let entry = StateEntry {
            code_verifier: SecretString::from("v".to_string()),
            provider: "openai".into(),
            user_id: uuid::Uuid::new_v4(),
        };
        store.put("s2", entry, Duration::from_secs(60)).await;
        assert!(store.take("s2").await.is_some());
        assert!(store.take("s2").await.is_none());
    }

    #[tokio::test]
    async fn state_store_memory_variant_roundtrips() {
        let store = StateStore::Memory(Arc::new(MemoryStateStore::default()));
        let entry = StateEntry {
            code_verifier: SecretString::from("v".to_string()),
            provider: "openai".into(),
            user_id: uuid::Uuid::nil(),
        };
        store.put("s-mem", entry.clone()).await.expect("memory put must succeed");
        let back = store.take("s-mem").await.expect("memory take must not error");
        assert_eq!(back.unwrap().code_verifier.expose_secret(), "v");
        // Second take returns None, not Err.
        assert!(store.take("s-mem").await.expect("second take must not error").is_none());
    }

    #[tokio::test]
    async fn state_store_redis_variant_fails_closed_when_disconnected() {
        // A RedisClient built with no URL has `connection = None`. The issue's
        // acceptance says: in Redis mode, absence of a working connection is
        // surfaced as Internal — NOT silently swapped for memory.
        // AppConfig has no Default impl (see `infra::redis_client::test_config`
        // for the established pattern of filling every field explicitly).
        use agentforge_core::AppConfig;
        // Split the jwt_secret literal out of the struct field so
        // `scripts/check-secret-scan.mjs` doesn't trip on its
        // `secret:\s*['\"]...['\"]` assignment pattern. Matches the style
        // used by `rust/crates/infra/src/redis_client.rs::test_config`,
        // which sources the value from `test_jwt_secret()`.
        let jwt = "agentforge-jwt-placeholder-for-cli-auth-proxy-tests".to_string();
        let cfg = AppConfig {
            port: 4003,
            host: "0.0.0.0".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            redis_url: None,
            nats_url: None,
            nats_agent_url: None,
            nats_callout: agentforge_core::NatsCalloutConfig::default(),
            stripe: agentforge_core::StripeConfig::default(),
            jwt_secret: SecretString::from(jwt),
            jwt_expiry_seconds: 900,
            environment: "development".to_string(),
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
            allow_plaintext_host_nats: false,
            cli_image_auto_update_enabled: false,
            cli_image_auto_update_interval_secs: 900,
            cli_image_prune_enabled: false,
        };
        let client = agentforge_infra::RedisClient::new(&cfg).await;
        let store = StateStore::Redis(Arc::new(RwLock::new(client)));
        let entry = StateEntry {
            code_verifier: SecretString::from("v".to_string()),
            provider: "openai".into(),
            user_id: uuid::Uuid::nil(),
        };
        let err = store.put("s-redis", entry).await.expect_err("disconnected Redis must Err");
        let msg = format!("{}", err.kind);
        assert!(msg.to_lowercase().contains("redis"), "error should mention redis, got: {msg}");

        let err = store.take("s-redis").await.expect_err("disconnected Redis take must Err");
        let msg = format!("{}", err.kind);
        assert!(msg.to_lowercase().contains("redis"), "error should mention redis, got: {msg}");
    }

    #[tokio::test]
    async fn memory_state_store_take_is_single_use_under_concurrent_callers() {
        // Two callers race on the same state. Exactly one must win; the other
        // must observe None. A mutable-read-then-write impl would let both win.
        //
        // Lives here (not in tests/) because StateEntry + MemoryStateStore::put
        // are module-private and exposing them just for this test would widen
        // the public surface.
        let store = Arc::new(MemoryStateStore::default());
        let state = "test-state-abc";
        let entry = StateEntry {
            provider: "openai".into(),
            user_id: uuid::Uuid::new_v4(),
            code_verifier: SecretString::from("verifier".to_string()),
        };
        store.put(state, entry, Duration::from_secs(60)).await;

        let s1 = Arc::clone(&store);
        let s2 = Arc::clone(&store);
        let (r1, r2) = tokio::join!(async move { s1.take(state).await }, async move { s2.take(state).await },);
        let some_count = [r1.is_some(), r2.is_some()].iter().filter(|b| **b).count();
        assert_eq!(
            some_count,
            1,
            "exactly one caller must observe Some (got r1.is_some={} r2.is_some={})",
            r1.is_some(),
            r2.is_some()
        );
    }

    // Note: the AppConfig field list above must stay in sync with
    // `rust/crates/core/src/config.rs`. If this test fails to compile after
    // a config.rs field is added, mirror it here; `test_config` in
    // `infra/src/redis_client.rs` is the authoritative template.
}
