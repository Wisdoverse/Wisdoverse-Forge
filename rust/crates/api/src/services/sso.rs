//! Enterprise single sign-on service — generic OpenID Connect (Casdoor,
//! Keycloak, Authentik, Entra ID, …).
//!
//! Flow (kept compatible with the login page's existing `auth_code` contract):
//!
//! 1. `GET  /api/v1/auth/providers` — the login button (empty = SSO disabled).
//! 2. `GET  /api/v1/auth/sso/oidc` — mint a random state (also bound to an
//!    httpOnly cookie), redirect to the provider's authorization endpoint.
//! 3. Provider redirects to `/api/v1/auth/sso/oidc/callback?code&state` — the
//!    callback validates state (store take + cookie match), exchanges the code,
//!    reads the user's email, provisions/locates the account, then redirects
//!    the browser to `SPA_BASE/login?auth_code=<120s opaque code>`.
//! 4. `POST /api/v1/auth/sso/exchange {code}` — the login page redeems the
//!    opaque code for a normal access token + refresh cookie.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::Utc;
use redis::AsyncCommands;
use secrecy::ExposeSecret;

use agentforge_core::{AppConfig, AppError, AppResult, UserId};
use agentforge_infra::RedisClient;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::domain::sso::{SsoExchangeRecord, SsoPolicy, SsoProvider};
use crate::services::user::UserService;

const SSO_STATE_TTL_SECS: u64 = 300;
const SSO_EXCHANGE_CODE_TTL_SECS: u64 = 120;
const SSO_STATE_REDIS_KEY_PREFIX: &str = "agentforge:sso-state:";
const SSO_EXCHANGE_REDIS_KEY_PREFIX: &str = "agentforge:sso-exchange:";
const SSO_MEMORY_MAX_ENTRIES: usize = 10_000;

/// OIDC discovery document fields this flow needs.
#[derive(Debug, Clone, serde::Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: Option<String>,
}

/// In-memory SSO state store — used when Redis is not configured. Must live on
/// `AppState` (shared across requests) so the authorize put and the callback
/// take land on the same instance.
#[derive(Default)]
pub struct SsoMemoryStateStore {
    inner: Mutex<HashMap<String, (String, SystemTime)>>,
}

impl SsoMemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    async fn put(&self, key: &str, value: &str, ttl: Duration) -> AppResult<()> {
        let mut guard = self.inner.lock().await;
        let now = SystemTime::now();
        guard.retain(|_, (_, expires)| *expires >= now);
        if guard.len() >= SSO_MEMORY_MAX_ENTRIES && !guard.contains_key(key) {
            return Err(SsoPolicy::sso_unavailable("the in-memory sign-in store is full").into());
        }
        guard.insert(key.to_string(), (value.to_string(), now + ttl));
        Ok(())
    }

    async fn take(&self, key: &str) -> Option<String> {
        let mut guard = self.inner.lock().await;
        match guard.remove(key) {
            Some((value, expires)) if expires >= SystemTime::now() => Some(value),
            _ => None,
        }
    }
}

/// State store with the same Redis/Memory duality as the CLI auth proxy:
/// Redis when configured, memory otherwise (single-replica deployments).
pub enum SsoStateStore {
    Redis(Arc<RwLock<RedisClient>>),
    Memory(Arc<SsoMemoryStateStore>),
}

impl SsoStateStore {
    async fn put_value(&self, prefix: &str, key: &str, value: &str, ttl_secs: u64) -> AppResult<()> {
        match self {
            Self::Redis(client) => {
                let key = format!("{prefix}{key}");
                let mut guard = client.write().await;
                let conn = guard
                    .connection_mut()
                    .ok_or_else(|| SsoPolicy::sso_unavailable("shared state store is unavailable"))?;
                let _: () = conn.set_ex(&key, value, ttl_secs).await.map_err(|err| -> AppError {
                    tracing::warn!(error = %err, "SSO Redis SET_EX failed");
                    SsoPolicy::sso_unavailable("could not store the sign-in transaction").into()
                })?;
                Ok(())
            }
            Self::Memory(memory) => memory.put(key, value, Duration::from_secs(ttl_secs)).await,
        }
    }

    async fn take_value(&self, prefix: &str, key: &str) -> AppResult<Option<String>> {
        match self {
            Self::Redis(client) => {
                let key = format!("{prefix}{key}");
                let mut guard = client.write().await;
                let conn = guard
                    .connection_mut()
                    .ok_or_else(|| SsoPolicy::sso_unavailable("shared state store is unavailable"))?;
                let raw: Option<String> =
                    redis::cmd("GETDEL").arg(&key).query_async(conn).await.map_err(|err| -> AppError {
                        tracing::warn!(error = %err, "SSO Redis GETDEL failed");
                        SsoPolicy::sso_unavailable("could not read the sign-in transaction").into()
                    })?;
                Ok(raw)
            }
            Self::Memory(memory) => Ok(memory.take(key).await),
        }
    }

    async fn put_state(&self, state: &str) -> AppResult<()> {
        self.put_value(SSO_STATE_REDIS_KEY_PREFIX, state, state, SSO_STATE_TTL_SECS).await
    }

    async fn take_state(&self, state: &str) -> AppResult<bool> {
        Ok(self.take_value(SSO_STATE_REDIS_KEY_PREFIX, state).await?.as_deref() == Some(state))
    }

    async fn put_exchange(&self, code: &str, user_id: UserId, organization_id: Uuid) -> AppResult<()> {
        let value = SsoExchangeRecord::new(user_id, organization_id, Utc::now().timestamp()).to_storage()?;
        self.put_value(SSO_EXCHANGE_REDIS_KEY_PREFIX, code, &value, SSO_EXCHANGE_CODE_TTL_SECS).await
    }

    async fn take_exchange(&self, code: &str) -> AppResult<Option<SsoExchangeRecord>> {
        Ok(self
            .take_value(SSO_EXCHANGE_REDIS_KEY_PREFIX, code)
            .await?
            .and_then(|value| SsoExchangeRecord::from_storage(&value)))
    }
}

/// Business logic layer for the OIDC sign-in flow.
pub struct SsoService {
    config: Arc<AppConfig>,
    store: SsoStateStore,
    client: reqwest::Client,
    discovery: Mutex<Option<OidcDiscovery>>,
}

impl SsoService {
    pub fn new(config: Arc<AppConfig>, store: SsoStateStore) -> Self {
        Self { config, store, client: reqwest::Client::new(), discovery: Mutex::new(None) }
    }

    /// Configured sign-in providers (empty = SSO disabled for this instance).
    pub fn providers(&self) -> Vec<SsoProvider> {
        let sso = &self.config.auth_sso;
        if !sso.enabled {
            return Vec::new();
        }
        vec![SsoProvider {
            name: "oidc".into(),
            display_name: sso.display_name.clone().unwrap_or_else(|| "Single sign-on".into()),
        }]
    }

    /// Build the provider authorization URL and mint the state that must come
    /// back on the callback (store put here; the route also binds it to a cookie).
    /// Returns `(authorization_url, state)`.
    pub async fn authorize_url(&self, redirect_uri: &str) -> AppResult<(String, String)> {
        if self.providers().is_empty() {
            return Err(SsoPolicy::not_configured().into());
        }
        let sso = &self.config.auth_sso;
        let discovery = self.fetch_discovery().await?;
        let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        self.store.put_state(&state).await?;
        let mut url = discovery.authorization_endpoint;
        url = append_query(&url, "response_type", "code");
        url = append_query(&url, "client_id", sso.oidc_client_id.as_deref().unwrap_or_default());
        url = append_query(&url, "redirect_uri", redirect_uri);
        url = append_query(&url, "scope", &sso.oidc_scopes);
        url = append_query(&url, "state", &state);
        Ok((url, state))
    }

    /// Handle the provider redirect: validate state, exchange the code, read
    /// the email, and return the browser redirect to the SPA login page with a
    /// single-use `auth_code`.
    pub async fn handle_callback(
        &self,
        code: &str,
        state: &str,
        cookie_state: Option<&str>,
        redirect_uri: &str,
        user_service: &UserService,
    ) -> AppResult<String> {
        if self.providers().is_empty() {
            return Err(SsoPolicy::not_configured().into());
        }
        if cookie_state != Some(state) || !self.store.take_state(state).await? {
            return Err(SsoPolicy::invalid_state().into());
        }
        let sso = &self.config.auth_sso;
        let discovery = self.fetch_discovery().await?;

        let token_response = self
            .client
            .post(&discovery.token_endpoint)
            .basic_auth(
                sso.oidc_client_id.as_deref().unwrap_or_default(),
                sso.oidc_client_secret.as_ref().map(|s| s.expose_secret() as &str),
            )
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", sso.oidc_client_id.as_deref().unwrap_or_default()),
            ])
            .send()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::authorization_failed(err.to_string())))?;

        if !token_response.status().is_success() {
            let detail = token_response.text().await.unwrap_or_default();
            let snippet: String = detail.chars().take(160).collect();
            return Err(SsoPolicy::into_app_error(SsoPolicy::authorization_failed(snippet)));
        }

        let tokens: serde_json::Value = token_response
            .json()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::authorization_failed(err.to_string())))?;
        let access_token = tokens
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| SsoPolicy::into_app_error(SsoPolicy::authorization_failed("no access token returned")))?;

        let userinfo_endpoint = discovery.userinfo_endpoint.ok_or_else(|| {
            SsoPolicy::into_app_error(SsoPolicy::authorization_failed(
                "the provider does not expose a userinfo endpoint",
            ))
        })?;
        let userinfo_response = self
            .client
            .get(&userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::authorization_failed(err.to_string())))?;
        if !userinfo_response.status().is_success() {
            return Err(SsoPolicy::into_app_error(SsoPolicy::authorization_failed(
                "could not read the account profile",
            )));
        }
        let userinfo: serde_json::Value = userinfo_response
            .json()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::authorization_failed(err.to_string())))?;

        if userinfo.get("email_verified").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(SsoPolicy::unverified_email().into());
        }
        let email = userinfo
            .get("email")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| SsoPolicy::into_app_error(SsoPolicy::missing_email()))?;
        let display_name = userinfo.get("name").and_then(serde_json::Value::as_str).map(str::to_owned);
        let groups = provider_group_claim(&userinfo, sso.role_claim.as_deref());
        let admin_groups = csv_list(sso.admin_groups.as_deref());
        let org_map = parse_org_group_map(sso.org_group_map.as_deref());
        if sso.deprovision
            && !org_map.is_empty()
            && !org_map.iter().any(|(_, required_group)| groups.contains(required_group))
        {
            return Err(SsoPolicy::access_not_assigned().into());
        }
        let user = user_service.ensure_sso_user(email, display_name.as_deref(), false).await?;
        let (org_id, _) = user_service.default_membership(user.id).await?;

        // When configured, the IdP group mapping is authoritative in both
        // directions. Owners remain protected by the repository predicate.
        if user_service.sync_sso_role(org_id, user.id, &groups, &admin_groups).await? {
            tracing::info!(user_id = %user.id, "SSO role mapping updated");
        }

        // Org provisioning (and optional deprovisioning) from the group map.
        if !org_map.is_empty() {
            user_service.sync_sso_org_memberships(&org_map, &groups, &admin_groups, user.id, sso.deprovision).await?;
        }

        // Team provisioning from the group map (inside the user's org).
        let team_map = parse_org_group_map(sso.team_group_map.as_deref());
        if !team_map.is_empty() {
            user_service.sync_sso_team_memberships(&team_map, &groups, &admin_groups, user.id, sso.deprovision).await?;
        }

        // Opaque, destructive-read code: it cannot be replayed or used as a
        // bearer token against authenticated API routes.
        let (exchange_org_id, _) = user_service.default_membership(user.id).await?;
        let exchange_code = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        self.store.put_exchange(&exchange_code, user.id, exchange_org_id).await?;

        let spa_base = sso.spa_base_url.as_deref().unwrap_or("http://localhost:4002").trim_end_matches('/');
        Ok(format!("{spa_base}/login?auth_code={}", urlencode(&exchange_code)))
    }

    /// Redeem a 120 s opaque code for a full session (access token + refresh).
    pub async fn exchange(
        &self,
        code: &str,
        user_service: &UserService,
    ) -> AppResult<crate::services::user::LoginResult> {
        let record = self
            .store
            .take_exchange(code)
            .await?
            .ok_or_else(|| SsoPolicy::into_app_error(SsoPolicy::invalid_exchange_code()))?;
        user_service.sso_sign_in(UserId::from(record.user_id), record.organization_id, record.issued_at).await
    }

    async fn fetch_discovery(&self) -> AppResult<OidcDiscovery> {
        if let Some(discovery) = self.discovery.lock().await.as_ref() {
            return Ok(discovery.clone());
        }
        let url = self
            .config
            .auth_sso
            .oidc_discovery_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| SsoPolicy::into_app_error(SsoPolicy::not_configured()))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::discovery_failed(err.to_string())))?;
        if !response.status().is_success() {
            let detail = response.text().await.unwrap_or_default();
            let snippet: String = detail.chars().take(120).collect();
            return Err(SsoPolicy::into_app_error(SsoPolicy::discovery_failed(snippet)));
        }
        let discovery: OidcDiscovery = response
            .json()
            .await
            .map_err(|err| SsoPolicy::into_app_error(SsoPolicy::discovery_failed(err.to_string())))?;
        *self.discovery.lock().await = Some(discovery.clone());
        Ok(discovery)
    }
}

/// Read the provider's group/role list from the userinfo claim (array of
/// strings, or a single comma-separated string).
fn provider_group_claim(userinfo: &serde_json::Value, claim: Option<&str>) -> Vec<String> {
    let Some(claim) = claim else { return Vec::new() };
    match userinfo.get(claim) {
        Some(serde_json::Value::Array(values)) => {
            values.iter().filter_map(serde_json::Value::as_str).map(str::to_owned).collect()
        }
        Some(serde_json::Value::String(value)) => csv_list(Some(value)),
        _ => Vec::new(),
    }
}

/// Parse the org provisioning map: `orgSlug=group1;orgSlug2=group2`.
fn parse_org_group_map(value: Option<&str>) -> Vec<(String, String)> {
    value
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (org_slug, group) = entry.split_once('=')?;
            let org_slug = org_slug.trim();
            let group = group.trim();
            if org_slug.is_empty() || group.is_empty() { None } else { Some((org_slug.to_string(), group.to_string())) }
        })
        .collect()
}

/// Split a comma-separated config list, trimming entries.
fn csv_list(value: Option<&str>) -> Vec<String> {
    value.unwrap_or_default().split(',').map(str::trim).filter(|entry| !entry.is_empty()).map(str::to_owned).collect()
}

fn append_query(url: &str, key: &str, value: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{key}={}", urlencode(value))
}

fn urlencode(value: &str) -> String {
    // Minimal URL-safe percent encoding for query values (RFC 3986 unreserved
    // characters are preserved; everything else is hex-encoded).
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
