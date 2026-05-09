//! `AuthCalloutWorker` — tokio task that wires Units 4–7 to a live NATS
//! connection (issue #38 phase 2).
//!
//! The worker opens up to **two** NATS connections:
//!
//! 1. **AUTH-account `auth_service` user (always).** Subscribes
//!    `$SYS.REQ.USER.AUTH` with the queue group `auth_callout`, pipes each
//!    inbound message through [`handle_auth_request`], and publishes the
//!    resulting [`CalloutResponse`] on the reply subject. `auth_service`
//!    lives in the AUTH account created in phase-2 `nats.conf` specifically
//!    for this purpose.
//!
//! 2. **SYS-account `sys` user (lazy, optional).** Opened on demand the
//!    first time [`AuthCalloutService::revoke`] is called and thereafter
//!    reused. Needed because the `$SYS.REQ.SERVER.<name>.KICK` subject is
//!    reserved for members of the SYS account — the API's primary backend
//!    user lives in AGENTFORGE and would be rejected with an authorisation
//!    violation. When `NatsCalloutConfig.sys_password` is absent, this
//!    connection is never attempted and revocation silently falls back to
//!    the DB-clear + 15-min JWT TTL path (`clear_container` at the stop
//!    site already removes `nats_connect_password` so the next CONNECT is
//!    denied by the callout handler).
//!
//! # Two URLs, one source of truth
//!
//! [`Self::new`] synthesises both URLs from the **same** `NATS_URL` base.
//! `strip` + rebuild ensures we inherit the host/port from whatever the
//! operator configured (`nats:4222` in docker, `127.0.0.1:4222` in dev)
//! without leaking backend credentials into either derived URL.
//!
//! # Shutdown
//!
//! [`Self::run`] listens on a `watch::Receiver<bool>` shutdown channel —
//! identical pattern to `OrchestrationResultWorker::run` in
//! `agentforge_jobs`. When shutdown is signalled, the subscriber stream is
//! dropped and the task returns cleanly; any in-flight request is not
//! explicitly cancelled (it completes its reply before the select loop
//! rechecks). The SYS client, if ever opened, is dropped with the worker.
//!
//! # Why `AuthCalloutService` is a separate facade
//!
//! HTTP route handlers (`stop_agent`, admin delete) need `revoke(agent_id)`
//! but must not hold a reference to the entire worker — the worker owns
//! the subscriber stream and cannot be `Clone`. [`AuthCalloutService`] is
//! a cheap `Clone` handle exposing only the revoke surface; it is stored
//! on `AppState` as `Option<Arc<AuthCalloutService>>` so the handlers
//! can `state.auth_callout.as_ref()` cleanly.

use std::sync::Arc;

use agentforge_core::{AppResult, ErrorKind, NatsCalloutConfig};
use agentforge_jobs::NatsConnectPasswordLookup;
use async_nats::Client;
use secrecy::ExposeSecret;
use std::time::Duration;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use super::handler::{CalloutSigningKeys, handle_auth_request};
use super::kick::ConnectionTracker;

/// Subject the NATS server publishes auth-callout requests to. Fixed by the
/// ADR-26 spec.
const AUTH_SUBJECT: &str = "$SYS.REQ.USER.AUTH";

/// Queue group for the auth callout subscription. With multiple API
/// replicas running, NATS load-balances each request to exactly one member
/// of this group — essential so an auth decision isn't attempted twice
/// (and `ConnectionTracker.record` isn't called twice for the same
/// `client_cid`).
const AUTH_QUEUE_GROUP: &str = "auth_callout";

/// Header name the NATS server sets on every callout request carrying the
/// ephemeral xkey public used to encrypt the request body. `None` means the
/// request arrived in plaintext mode — acceptable only in dev overrides.
const SERVER_XKEY_HEADER: &str = "Nats-Server-Xkey";

/// Strip any `user:password@` user-info segment from a NATS URL so
/// `ConnectOptions::connect(url)` sees only the scheme + host. The
/// backend's `NATS_URL` may carry backend-account credentials; those
/// must never be handed to the callout-account / SYS-account CONNECT.
///
/// Same `rsplit_once('@')` rationale as `containers.rs::strip_nats_url_user_info`:
/// passwords may contain `:` but never `@`, so the host/user-info
/// boundary is the last `@` before the path.
fn strip_user_info(backend_url: &str) -> AppResult<String> {
    let (scheme, rest) = backend_url
        .split_once("://")
        .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_URL missing scheme://host separator")))?;
    let host = match rest.rsplit_once('@') {
        Some((_, host)) => host,
        None => rest,
    };
    Ok(format!("{scheme}://{host}"))
}

/// Live NATS auth-callout worker. Opens the AUTH connection at
/// construction time (fail-fast — wrong password should crash startup
/// rather than bounce every CONNECT for 15 minutes) and exposes
/// [`Self::service_handle`] for the revocation surface.
pub struct AuthCalloutWorker<L: NatsConnectPasswordLookup> {
    auth_client: Client,
    lookup: L,
    signing_keys: CalloutSigningKeys,
    tracker: ConnectionTracker,
    /// Lazy SYS client, opened on first `revoke()` that actually needs it.
    /// `Arc<Mutex<Option<…>>>` because [`AuthCalloutService`] shares the
    /// same cell — opening it once is enough, subsequent revokes reuse.
    sys_client: Arc<Mutex<Option<Client>>>,
    /// SYS (host_url, password) pair. `None` means SYS credentials
    /// weren't configured — revoke() silently short-circuits to the
    /// DB-clear + JWT-TTL fallback. Stored split so the revoke path
    /// can use `ConnectOptions::user_and_password(...).connect(url)`
    /// rather than URL-embedded creds that `async_nats` ignores.
    sys_credentials: Option<(String, String)>,
    server_name: String,
}

impl<L: NatsConnectPasswordLookup> AuthCalloutWorker<L> {
    /// Construct the worker — opens the AUTH connection and validates
    /// configuration; does NOT start the subscription loop ([`Self::run`]
    /// does).
    pub async fn new(
        backend_nats_url: &str,
        config: &NatsCalloutConfig,
        audience_account_name: String,
        lookup: L,
    ) -> AppResult<Self> {
        let auth_pw = config
            .auth_service_password
            .as_ref()
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_CALLOUT__AUTH_SERVICE_PASSWORD required")))?
            .expose_secret()
            .to_string();
        // `async_nats::connect` silently ignores user-info embedded in
        // the URL (confirmed against v0.44 — only the builder path
        // applies CONNECT credentials). Use `ConnectOptions` so the
        // AUTH-account CONNECT actually authenticates. Discovered during
        // the issue-#55 E2E bring-up: a URL-creds CONNECT returned
        // `authorization violation` because the server saw an
        // anonymous client, which then bounced off the callout's
        // `auth_users` whitelist.
        let host_url = strip_user_info(backend_nats_url)?;
        let auth_client = async_nats::ConnectOptions::new()
            .user_and_password("auth_service".to_string(), auth_pw)
            .connect(&host_url)
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("AUTH NATS connect: {err}")))?;

        let issuer_seed = config
            .issuer_seed
            .as_ref()
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_CALLOUT__ISSUER_SEED required")))?
            .clone();
        let account_signing_key_seed = config
            .account_signing_key_seed
            .as_ref()
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_CALLOUT__ACCOUNT_SIGNING_KEY_SEED required")))?
            .clone();
        let xkey_seed = config
            .xkey_seed
            .as_ref()
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_CALLOUT__XKEY_SEED required")))?
            .clone();
        let signing_keys =
            CalloutSigningKeys { issuer_seed, account_signing_key_seed, xkey_seed, audience_account_name };

        let server_name = config
            .server_name
            .clone()
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("NATS_CALLOUT__SERVER_NAME required")))?;

        // SYS credentials are optional — stored as (host_url, password)
        // so the revoke path can use the `ConnectOptions` builder and
        // avoid the same URL-auth pitfall as the AUTH connection above.
        let sys_credentials: Option<(String, String)> = config
            .sys_password
            .as_ref()
            .map(|pw| -> AppResult<_> { Ok((host_url.clone(), pw.expose_secret().to_string())) })
            .transpose()?;

        if sys_credentials.is_none() {
            tracing::warn!("NATS_CALLOUT__SYS_PASSWORD unset — revocation will fall back to 15-min JWT TTL (no KICK)");
        }

        Ok(Self {
            auth_client,
            lookup,
            signing_keys,
            tracker: ConnectionTracker::default(),
            sys_client: Arc::new(Mutex::new(None)),
            sys_credentials,
            server_name,
        })
    }

    /// Produce a cheap `Clone` handle exposing only the revoke surface.
    /// Must be called BEFORE [`Self::run`] consumes `self`.
    pub fn service_handle(&self) -> AuthCalloutService {
        AuthCalloutService {
            tracker: self.tracker.clone(),
            sys_client: Arc::clone(&self.sys_client),
            sys_credentials: self.sys_credentials.clone(),
            server_name: self.server_name.clone(),
        }
    }

    /// Long-running task body. Mirrors `OrchestrationResultWorker::run` —
    /// queue-subscribe, loop `tokio::select!{}` until `shutdown` fires or
    /// the subscription closes, dispatch each message to
    /// `handle_auth_request`.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        use futures::StreamExt;

        let mut subscriber = match self.auth_client.queue_subscribe(AUTH_SUBJECT, AUTH_QUEUE_GROUP.to_string()).await {
            Ok(sub) => sub,
            Err(err) => {
                tracing::error!(error = %err, subject = AUTH_SUBJECT, "Auth callout worker failed to subscribe");
                super::metrics::record_callout_worker_status("subscribe_failed");
                return;
            }
        };
        tracing::info!(subject = AUTH_SUBJECT, queue = AUTH_QUEUE_GROUP, "Auth callout worker listening");
        super::metrics::record_callout_worker_status("subscribed");

        // Periodic tracker reaper. Runs every 60s, drops entries older than
        // 2× JWT TTL (30 min) so long-dead agents that never hit stop_agent
        // do not grow the tracker unboundedly on long-lived API instances.
        // Spawned inside `run` (not `new`) so it shares the shutdown watch
        // channel and exits cleanly on Ctrl-C.
        let reap_tracker = self.tracker.clone();
        let mut reap_shutdown = shutdown.clone();
        let reap_handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            let max_age = Duration::from_secs(2 * 15 * 60); // 2 × DEFAULT_JWT_TTL
            loop {
                tokio::select! {
                    _ = reap_shutdown.changed() => {
                        if *reap_shutdown.borrow() { break; }
                    }
                    _ = ticker.tick() => {
                        reap_tracker.reap_expired(max_age).await;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Auth callout worker shutting down");
                        super::metrics::record_callout_worker_status("shutdown");
                        break;
                    }
                }
                msg = subscriber.next() => {
                    match msg {
                        Some(nats_msg) => {
                            let server_xkey = nats_msg
                                .headers
                                .as_ref()
                                .and_then(|h| h.get(SERVER_XKEY_HEADER))
                                .map(|v| v.to_string());
                            let reply = nats_msg.reply.clone();
                            let resp = handle_auth_request(
                                &self.lookup,
                                &self.signing_keys,
                                &self.tracker,
                                &nats_msg.payload,
                                server_xkey.as_deref(),
                            )
                            .await;
                            match reply {
                                Some(reply_subject) => {
                                    let result = match resp.reply_headers {
                                        Some(headers) => {
                                            self.auth_client
                                                .publish_with_headers(reply_subject, headers, resp.payload.into())
                                                .await
                                        }
                                        None => self.auth_client.publish(reply_subject, resp.payload.into()).await,
                                    };
                                    if let Err(err) = result {
                                        tracing::warn!(error = %err, "Auth callout reply publish failed");
                                    }
                                }
                                None => {
                                    tracing::warn!("Auth callout request had no reply subject — dropping response");
                                }
                            }
                        }
                        None => {
                            tracing::info!("Auth callout subscription closed by server");
                            super::metrics::record_callout_worker_status("subscription_closed");
                            break;
                        }
                    }
                }
            }
        }

        // Give the reaper a moment to exit cleanly on the same shutdown
        // signal before returning from `run`.
        let _ = reap_handle.await;
    }
}

/// Cheap `Clone` handle exposing only the revocation surface. Stored on
/// `AppState` so HTTP handlers (`stop_agent`, admin delete) can trigger
/// targeted KICKs without owning the worker itself.
#[derive(Clone)]
pub struct AuthCalloutService {
    tracker: ConnectionTracker,
    sys_client: Arc<Mutex<Option<Client>>>,
    sys_credentials: Option<(String, String)>,
    server_name: String,
}

impl AuthCalloutService {
    /// Revoke an agent's live NATS connection.
    ///
    /// Best-effort: any failure along the path (no tracked connection, no
    /// SYS credentials, SYS connect failure, KICK publish failure) is
    /// logged and silently absorbed. Correctness is guaranteed by the
    /// 15-minute JWT TTL and the `clear_container` call upstream that
    /// wipes `agents.nats_connect_password` so the next CONNECT is denied
    /// anyway. The KICK path is a latency optimisation (≤2s revocation
    /// vs. 15 min) rather than a security-critical primitive.
    ///
    /// Tracker state is consumed on success — a second concurrent revoke
    /// for the same agent finds `None` and exits early, avoiding a
    /// double-KICK.
    pub async fn revoke(&self, agent_id: Uuid) {
        let Some(tracked) = self.tracker.take(agent_id).await else {
            tracing::debug!(
                %agent_id,
                "revoke: no tracked connection (JWT already expired or agent never connected)"
            );
            super::metrics::record_callout_revoke("no_tracked_connection");
            return;
        };

        let Some((sys_host_url, sys_password)) = self.sys_credentials.as_ref() else {
            tracing::warn!(
                %agent_id,
                "revoke: NATS_CALLOUT__SYS_PASSWORD unset — skipping KICK (revocation falls back to 15-min JWT TTL)"
            );
            super::metrics::record_callout_revoke("no_sys_creds");
            return;
        };

        // Double-checked-locking: take a snapshot under the lock, release,
        // then dial outside the lock. Prevents concurrent revokes from
        // serialising behind the SYS `async_nats::connect` await (which can
        // hang on DNS / TCP backoff), and avoids holding a Tokio mutex
        // across an indefinitely-long await.
        //
        // Use `ConnectOptions::user_and_password(...).connect(host_url)` —
        // the URL form is ignored by `async_nats` (same v0.44 pitfall the
        // AUTH client above works around).
        let existing = self.sys_client.lock().await.clone();
        let client = match existing {
            Some(c) => c,
            None => match async_nats::ConnectOptions::new()
                .user_and_password("sys".to_string(), sys_password.clone())
                .connect(sys_host_url)
                .await
            {
                Ok(new_client) => {
                    tracing::info!("SYS NATS connection opened for revocation");
                    let mut guard = self.sys_client.lock().await;
                    guard.get_or_insert(new_client).clone()
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        %agent_id,
                        "revoke: SYS NATS connect failed — skipping KICK"
                    );
                    super::metrics::record_callout_revoke("sys_connect_failed");
                    return;
                }
            },
        };

        let subject = format!("$SYS.REQ.SERVER.{}.KICK", self.server_name);
        let payload = serde_json::json!({ "cid": tracked.client_cid }).to_string();
        match client.publish(subject.clone(), payload.into_bytes().into()).await {
            Ok(()) => {
                tracing::info!(
                    %agent_id,
                    server_id = %tracked.server_id,
                    cid = tracked.client_cid,
                    "revoke: KICK published"
                );
                super::metrics::record_callout_revoke("kick_published");
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    %agent_id,
                    %subject,
                    "revoke: KICK publish failed"
                );
                super::metrics::record_callout_revoke("kick_publish_failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_user_info_removes_embedded_creds() {
        assert_eq!(strip_user_info("nats://backend:pw@nats:4222").unwrap(), "nats://nats:4222");
    }

    #[test]
    fn strip_user_info_preserves_plain_host() {
        assert_eq!(strip_user_info("nats://nats:4222").unwrap(), "nats://nats:4222");
    }

    #[test]
    fn strip_user_info_rejects_malformed_input() {
        assert!(strip_user_info("not-a-url").is_err());
    }

    #[test]
    fn strip_user_info_uses_last_at_boundary() {
        // Passwords may contain `:`; `rsplit_once('@')` picks the last
        // `@`, which is the user-info boundary. Hostnames never contain
        // `@`, so this is unambiguous.
        assert_eq!(strip_user_info("nats://u:a:b@nats:4222").unwrap(), "nats://nats:4222");
    }
}
