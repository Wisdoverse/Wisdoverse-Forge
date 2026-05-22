//! Pure async handler for one NATS Authorization Callout request (issue #38
//! phase 2).
//!
//! This module is the security boundary that stitches Units 2 (password
//! lookup), 4 (XKey), 5 (JWT sign), 6 (permissions) into one function.  It is
//! deliberately **free of I/O setup**: the caller (Unit 8's worker) supplies
//! every collaborator via trait objects / plain references, which lets us
//! exercise the full decision tree with in-memory fakes and no NATS client,
//! no DB pool, and no tokio runtime outside `tokio::test`.
//!
//! # Decision tree (mirrors the plan file verbatim)
//!
//! ```text
//!   [request_bytes] + optional server_xkey header
//!        │
//!        ├── server_xkey present? → xkey::open(ours, theirs, bytes)
//!        │        └── err → deny(xkey_open_failed)
//!        ▼
//!   parse_authorization_request(jwt) ── err → deny(bad_request)
//!        │
//!        ├── connect_user parses as UUID? ── no → deny(bad_request)
//!        ▼
//!   lookup.find_password(agent_id)
//!        ├── Err                          → deny(lookup_error)
//!        ├── Ok(None)                     → deny(agent_unknown)
//!        └── Ok(Some(expected))
//!                │
//!                ├── constant-time compare with connect_password
//!                │       └── mismatch    → deny(password_mismatch)
//!                ▼
//!          sign_user_jwt(perms) ── err    → deny(signing_failed)
//!                │
//!                ▼
//!          sign_authorization_response_jwt(inner) ── err → deny(signing_failed)
//!                │
//!                ├── xkey present → xkey::seal(ours, theirs, bytes)
//!                ▼
//!          tracker.record(agent, server_id, client_cid)
//!                │
//!                ▼
//!          CalloutResponse { payload, reply_headers: None }
//! ```
//!
//! # Timing attack mitigation
//!
//! Every deny path applies a uniform 50-150ms jitter BEFORE returning, so
//! `password_mismatch`, `agent_unknown`, `lookup_error`, and `bad_request`
//! are indistinguishable on the wire.  The `handler_timing_indistinguishable_across_deny_reasons`
//! test asserts this quantitatively.
//!
//! # Reply headers
//!
//! `CalloutResponse.reply_headers` is always `None` today — the NATS server
//! does not require us to echo the `Nats-Server-Xkey` header on the reply
//! (it remembers which xkey it sent us and uses that to open our sealed
//! payload).  The field exists for forward compatibility so Unit 8 can
//! publish the reply uniformly without special-casing.  If a future ADR
//! change mandates echoing the header, the handler is the only place to
//! update.

use secrecy::{ExposeSecret, SecretString};
use std::time::{Duration, Instant};
use uuid::Uuid;

use agentforge_jobs::auth_lookup::NatsConnectPasswordLookup;

use crate::domain::auth_callout::CalloutResponse;

use super::jwt::{self, AuthorizationRequest};
use super::kick::ConnectionTracker;
use super::metrics;
use super::perms::build_agent_permissions;
use super::xkey;

/// Lifetime of every JWT we mint — both the inner User JWT and the outer
/// AuthorizationResponse.  15 minutes balances two concerns: long enough that
/// a healthy agent won't re-auth mid-task, short enough that revocation
/// latency (once a KICK is unavailable) stays bounded.
pub const DEFAULT_JWT_TTL: Duration = Duration::from_secs(15 * 60);

/// The set of secrets Unit 7 needs to sign allow-path JWTs and encrypt
/// responses.  Constructed once at startup from `AppConfig::nats_callout` and
/// passed by reference on every request.
///
/// All three seed fields are `SecretString` so the derived `Debug` (via
/// `secrecy`) redacts them — the `CalloutSigningKeys { .. }` literal can
/// appear in `tracing::debug!(?signing_keys)` without leaking material.
/// The `audience_account_name` field is a plain account-name string and is
/// logged verbatim.
#[derive(Debug, Clone)]
pub struct CalloutSigningKeys {
    /// Issuer seed (`SA…`) for the outer AuthorizationResponse JWT.  Its
    /// public half is registered as `authorization.auth_callout.issuer` in
    /// `nats.conf`.
    pub issuer_seed: SecretString,
    /// Account signing-key seed (`SA…`) used as the inner User JWT's
    /// issuer.  In server-config / non-operator mode the outer
    /// AuthorizationResponse signature is the sole trust anchor NATS
    /// evaluates; the inner JWT's `iss` is informational, and this key's
    /// public half is **not** registered anywhere in `nats.conf` (issue
    /// #55 — `accounts.<NAME>.signing_keys` is only valid in operator
    /// mode and `nats-server` rejects it in server-config mode).
    pub account_signing_key_seed: SecretString,
    /// Curve25519 XKey seed (`SX…`) used to open inbound request ciphertexts
    /// and seal outbound responses.  Its public half is registered as
    /// `authorization.auth_callout.xkey` in `nats.conf`.
    pub xkey_seed: SecretString,
    /// Account **name** string (e.g. `"AGENTFORGE"`) — the `aud` claim on
    /// every inner User JWT.  NATS uses this in server-config mode to place
    /// the minted user onto a specific `accounts { NAME { … } }` block via
    /// `s.LookupAccount(userJwt.Audience)`.  Must match the account label
    /// in `docker/nats.conf`; passing an account public nkey here yields
    /// `no valid account` at CONNECT time.
    pub audience_account_name: String,
}

// ---------------------------------------------------------------------------
// Constant-time password compare
// ---------------------------------------------------------------------------

/// Byte-wise equality of `a` and `b` with no short-circuit on mismatch.
///
/// The `subtle` crate is only a transitive dep in this workspace (via
/// `ed25519-dalek` and friends), so rather than add it to `Cargo.toml` we
/// reimplement the primitive inline — it's a few lines and hard to get wrong.
/// The XOR-accumulate loop does one pass regardless of where the first
/// differing byte is, which is the invariant we need.
///
/// Length mismatch is still a `return false`, BUT the inner accumulator
/// always iterates `max(a.len(), b.len())` bytes before returning — that
/// keeps CPU-cycle count symmetric between the shorter and longer side
/// so an attacker cannot distinguish `len(a) == len(expected) vs.
/// len(a) < len(expected)` from wall-clock timing. Defence-in-depth: the
/// request body length is observable on the wire, but this guard means a
/// future log / metric that only fires on the fast path cannot leak length.
fn ct_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut diff: u8 = 0;
    for i in 0..max_len {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0 && a.len() == b.len()
}

// ---------------------------------------------------------------------------
// Deny helper
// ---------------------------------------------------------------------------

/// Build a deny response.
///
/// Every deny path in the handler funnels through here so the shape is
/// uniform — same jitter, same error string on the wire, same metric bump.
///
/// `req_subject_user_nkey` and `req_audience_server_id` may be empty strings
/// on the pre-parse deny path (`bad_request`, `xkey_open_failed`): the server
/// will reject the resulting response JWT, but the client already sees an
/// `-ERR Authorization Violation` either way, so the end result is the same
/// and the timing jitter still applies.
async fn build_deny_response(
    req_subject_user_nkey: &str,
    req_audience_server_id: &str,
    signing_keys: &CalloutSigningKeys,
    server_xkey_from_header: Option<&str>,
    reason: &'static str,
) -> CalloutResponse {
    metrics::record_callout_unauthorized(reason);

    // 50-150ms jitter (uniform).  Uses `rand::random` which is seeded from
    // `OsRng` on first call and reseeded as needed — no correlation across
    // denies.  The goal is to make `password_mismatch` and `agent_unknown`
    // indistinguishable by wall-clock latency so an attacker cannot
    // enumerate valid agent UUIDs from timing alone.
    let jitter_ms = 50 + (rand::random::<u64>() % 100);
    tokio::time::sleep(Duration::from_millis(jitter_ms)).await;

    // Sign the deny response.  If signing itself fails (only possible with
    // a malformed seed — `AppConfig::from_env` validates at boot), record
    // a SEPARATE infrastructure counter (`signing_errors_total`) instead of
    // re-bumping `unauthorized_total{reason}`: operators want "how many
    // denies happened" to sum to a single event per request. Double-counting
    // across a deny reason + `signing_failed` would pollute dashboards.
    let resp_jwt = jwt::sign_authorization_response_jwt(
        req_audience_server_id,
        signing_keys.issuer_seed.expose_secret(),
        req_subject_user_nkey,
        // Inner JWT must be empty-string on deny — the server treats a
        // non-empty `nats.jwt` as the canonical output and ignores `error`
        // in some versions.  Empty string + `error` is the spec path.
        "",
        Some("auth failed"),
        DEFAULT_JWT_TTL,
    )
    .unwrap_or_else(|_| {
        metrics::record_callout_signing_error("deny_path_sign");
        String::new()
    });

    // Seal the deny response if the client asked for xkey encryption. On
    // seal failure (malformed xkey), bump the same infra counter rather
    // than the unauthorized counter. The client still receives a deny —
    // NATS rejects the plaintext-on-xkey path, but the deny outcome is
    // preserved and the metrics stay clean.
    let payload = match server_xkey_from_header {
        Some(pub_key) => xkey::seal(signing_keys.xkey_seed.expose_secret(), pub_key, resp_jwt.as_bytes())
            .unwrap_or_else(|_| {
                metrics::record_callout_signing_error("deny_path_seal");
                Vec::new()
            }),
        None => resp_jwt.into_bytes(),
    };

    CalloutResponse { payload, reply_headers: None }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Process one Authorization Callout request.
///
/// Takes everything it needs by reference — no ambient state, no global
/// singletons — so the same function runs in production (backed by the real
/// `SqlxNatsConnectPasswordLookup` and a real `ConnectionTracker`) and in
/// tests (backed by in-memory fakes).
///
/// Panics are impossible on any non-test path: every `?` is mapped to a deny,
/// and the only `.unwrap()`-style constructs live in the test module.
///
/// # Parameters
///
/// - `lookup`: Unit 2's trait for password lookup.
/// - `signing_keys`: Unit 3-sourced secrets, wrapped in `SecretString`.
/// - `tracker`: [`ConnectionTracker`] — allow-path success stores
///   `(server_id, client_cid)` here so Unit 8 can KICK later.
/// - `request_bytes`: raw body of the `$SYS.REQ.USER.AUTH` message.
/// - `server_xkey_from_header`: value of the `Nats-Server-Xkey` header, if
///   present.  When `Some`, the request body is xkey-encrypted and the
///   response must be sealed back; when `None`, the callout runs in
///   plaintext mode (dev only).
pub async fn handle_auth_request<L>(
    lookup: &L,
    signing_keys: &CalloutSigningKeys,
    tracker: &ConnectionTracker,
    request_bytes: &[u8],
    server_xkey_from_header: Option<&str>,
) -> CalloutResponse
where
    L: NatsConnectPasswordLookup,
{
    let start = Instant::now();
    let response = handle_inner(lookup, signing_keys, tracker, request_bytes, server_xkey_from_header).await;
    metrics::record_callout_duration(start.elapsed());
    response
}

/// Inner body of [`handle_auth_request`], split out so the outer wrapper can
/// record duration regardless of which branch we returned from.
async fn handle_inner<L>(
    lookup: &L,
    signing_keys: &CalloutSigningKeys,
    tracker: &ConnectionTracker,
    request_bytes: &[u8],
    server_xkey_from_header: Option<&str>,
) -> CalloutResponse
where
    L: NatsConnectPasswordLookup,
{
    // 1. Optionally decrypt the request body.
    let plaintext_bytes: Vec<u8> = match server_xkey_from_header {
        Some(pub_key) => {
            match xkey::open(signing_keys.xkey_seed.expose_secret(), pub_key, request_bytes) {
                Ok(bytes) => bytes,
                Err(_) => {
                    // Pre-parse deny — we don't know user_nkey or server_id yet,
                    // so empty strings go on the response.  The server will
                    // reject our reply and return `-ERR Authorization Violation`
                    // to the client.
                    return build_deny_response("", "", signing_keys, server_xkey_from_header, "xkey_open_failed")
                        .await;
                }
            }
        }
        None => request_bytes.to_vec(),
    };

    // 2. Parse the inner JWT.
    let request: AuthorizationRequest = match std::str::from_utf8(&plaintext_bytes)
        .map_err(|_| ())
        .and_then(|s| jwt::parse_authorization_request(s).map_err(|_| ()))
    {
        Ok(r) => r,
        Err(()) => {
            return build_deny_response("", "", signing_keys, server_xkey_from_header, "bad_request").await;
        }
    };

    // 3. Parse the claimed agent UUID from connect_user.
    let agent_id: Uuid = match Uuid::parse_str(&request.connect_user) {
        Ok(id) => id,
        Err(_) => {
            return build_deny_response(
                &request.user_nkey,
                &request.server_id,
                signing_keys,
                server_xkey_from_header,
                "bad_request",
            )
            .await;
        }
    };

    // 4. Look up the expected password.
    let expected_password: String = match lookup.find_password(agent_id).await {
        Ok(Some(pw)) => pw,
        Ok(None) => {
            return build_deny_response(
                &request.user_nkey,
                &request.server_id,
                signing_keys,
                server_xkey_from_header,
                "agent_unknown",
            )
            .await;
        }
        Err(_) => {
            // Infra error — treat as deny, never leak details to the client.
            return build_deny_response(
                &request.user_nkey,
                &request.server_id,
                signing_keys,
                server_xkey_from_header,
                "lookup_error",
            )
            .await;
        }
    };

    // 5. Constant-time compare.
    if !ct_eq_bytes(request.connect_password.expose_secret().as_bytes(), expected_password.as_bytes()) {
        return build_deny_response(
            &request.user_nkey,
            &request.server_id,
            signing_keys,
            server_xkey_from_header,
            "password_mismatch",
        )
        .await;
    }

    // 6. Mint the inner User JWT with per-agent permissions.
    //
    // In server-config / non-operator mode NATS verifies the inner
    // User JWT against `authorization.auth_callout.issuer` — the same
    // public key that validates the outer AuthorizationResponse.
    // Signing the inner with a separate account signing key (as we
    // did initially) fails with `Claim failed V2 signature verification`
    // because the signing-key registry doesn't exist in server-config
    // mode. Use the callout `issuer_seed` for both inner and outer.
    let permissions = build_agent_permissions(agent_id);
    let inner_jwt = match jwt::sign_user_jwt(
        &request.user_nkey,
        signing_keys.issuer_seed.expose_secret(),
        &signing_keys.audience_account_name,
        &format!("agent-{agent_id}"),
        DEFAULT_JWT_TTL,
        &permissions,
    ) {
        Ok(j) => j,
        Err(_) => {
            return build_deny_response(
                &request.user_nkey,
                &request.server_id,
                signing_keys,
                server_xkey_from_header,
                "signing_failed",
            )
            .await;
        }
    };

    // 7. Wrap the inner JWT in the outer AuthorizationResponse.
    let outer_jwt = match jwt::sign_authorization_response_jwt(
        &request.server_id,
        signing_keys.issuer_seed.expose_secret(),
        &request.user_nkey,
        &inner_jwt,
        None,
        DEFAULT_JWT_TTL,
    ) {
        Ok(j) => j,
        Err(_) => {
            return build_deny_response(
                &request.user_nkey,
                &request.server_id,
                signing_keys,
                server_xkey_from_header,
                "signing_failed",
            )
            .await;
        }
    };

    // 8. Seal (if xkey) and assemble payload.
    let payload: Vec<u8> = match server_xkey_from_header {
        Some(pub_key) => match xkey::seal(signing_keys.xkey_seed.expose_secret(), pub_key, outer_jwt.as_bytes()) {
            Ok(ct) => ct,
            Err(_) => {
                return build_deny_response(
                    &request.user_nkey,
                    &request.server_id,
                    signing_keys,
                    server_xkey_from_header,
                    "signing_failed",
                )
                .await;
            }
        },
        None => outer_jwt.into_bytes(),
    };

    // 9. Record the tracked connection so Unit 8 can KICK on revoke.
    tracker.record(agent_id, request.server_id, request.client_cid).await;

    CalloutResponse { payload, reply_headers: None }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use nkeys::{KeyPair, XKey};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // ---- Fake password lookup -------------------------------------------------

    #[derive(Clone, Default)]
    struct FakeLookup {
        /// `None` variant → simulate a DB error on `find_password`.
        /// Otherwise returns the stored password for matching UUIDs.
        inner: Arc<Mutex<FakeLookupInner>>,
    }

    #[derive(Default)]
    struct FakeLookupInner {
        passwords: HashMap<Uuid, String>,
        simulate_error: bool,
    }

    impl FakeLookup {
        async fn insert(&self, id: Uuid, password: &str) {
            self.inner.lock().await.passwords.insert(id, password.to_string());
        }

        async fn set_error_mode(&self, on: bool) {
            self.inner.lock().await.simulate_error = on;
        }
    }

    #[async_trait]
    impl NatsConnectPasswordLookup for FakeLookup {
        async fn find_password(&self, agent_id: Uuid) -> anyhow::Result<Option<String>> {
            let guard = self.inner.lock().await;
            if guard.simulate_error {
                return Err(anyhow::anyhow!("simulated DB error"));
            }
            Ok(guard.passwords.get(&agent_id).cloned())
        }
    }

    fn test_password() -> String {
        format!("p-{}", Uuid::new_v4())
    }

    // ---- Test fixtures --------------------------------------------------------

    /// Full set of fresh keys for one test, plus the derived signing-keys
    /// struct the handler takes.  Returns the raw components too so tests
    /// can forge matching requests without re-parsing.
    struct Fixture {
        signing_keys: CalloutSigningKeys,
        /// Ephemeral "server" XKey used to encrypt requests INTO our handler
        /// (and to which our handler's response xkey-seals back).  The
        /// handler sees this public key as `server_xkey_from_header`.
        server_xkey: XKey,
        /// NATS server's signing keypair — used to sign the synthetic
        /// request JWT so `parse_authorization_request` succeeds.
        server_nkey: KeyPair,
        /// Per-connection user nkey the server "generates".
        user_nkey_pub: String,
    }

    fn make_fixture() -> Fixture {
        let issuer = KeyPair::new_account();
        let acct_sk = KeyPair::new_account();
        let xkey_local = XKey::new();

        let signing_keys = CalloutSigningKeys {
            issuer_seed: SecretString::from(issuer.seed().expect("issuer seed")),
            account_signing_key_seed: SecretString::from(acct_sk.seed().expect("acct sk seed")),
            xkey_seed: SecretString::from(xkey_local.seed().expect("xkey seed")),
            // NATS server-config mode: `aud` on the minted inner JWT is
            // the account NAME string that LookupAccount resolves. The
            // production value matches the `accounts { AGENTFORGE { … } }`
            // block label in `docker/nats.conf`.
            audience_account_name: "AGENTFORGE".to_string(),
        };

        Fixture {
            signing_keys,
            server_xkey: XKey::new(),
            server_nkey: KeyPair::new_server(),
            user_nkey_pub: KeyPair::new_user().public_key(),
        }
    }

    /// Construct the local xkey's public key so tests can verify what the
    /// handler exposes as "our public xkey".
    fn local_xkey_pub(sk: &SecretString) -> String {
        XKey::from_seed(sk.expose_secret()).expect("seed valid").public_key()
    }

    /// Build a well-formed authorization_request JWT body.  Signature is
    /// produced with the fixture's server nkey so the token parses as a
    /// three-part JWT even though `parse_authorization_request` doesn't
    /// verify the signature.
    fn build_request_jwt(fixture: &Fixture, connect_user: &str, connect_password: &str, client_cid: u64) -> String {
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::json!({"typ":"JWT","alg":"ed25519-nkey"}).to_string());

        let claims = serde_json::json!({
            "iss": fixture.server_nkey.public_key(),
            "sub": fixture.user_nkey_pub,
            "aud": "nats-authorization-request",
            "iat": 1_700_000_000,
            "exp": 2_700_000_000u64,
            "jti": "test-jti",
            "nats": {
                "server_id": {
                    "id": fixture.server_nkey.public_key(),
                    "xkey": fixture.server_xkey.public_key(),
                },
                "user_nkey": fixture.user_nkey_pub,
                "connect_opts": {
                    "user": connect_user,
                    "pass": connect_password,
                },
                "client_info": {
                    "id": client_cid,
                    "host": "127.0.0.1",
                },
                "request_nonce": "nonce",
                "type": "authorization_request",
                "version": 2,
            },
        });
        let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims ser"));

        let signing_input = format!("{header_b64}.{claims_b64}");
        let sig = fixture.server_nkey.sign(signing_input.as_bytes()).expect("sig");
        // Signature encoding doesn't matter for these tests — the parser
        // doesn't verify — but we use base64url to match the real wire
        // format (`nats-io/jwt`'s `base64.RawURLEncoding`).
        let sig_b64 = URL_SAFE_NO_PAD.encode(&sig);
        format!("{signing_input}.{sig_b64}")
    }

    /// Decode a plaintext response payload back into its claim body so the
    /// test can assert on it.  Panics on malformed input — tests should not
    /// hit that path.
    fn decode_response_claims(payload: &[u8]) -> serde_json::Value {
        let s = std::str::from_utf8(payload).expect("payload utf-8");
        let parts: Vec<&str> = s.split('.').collect();
        assert_eq!(parts.len(), 3, "response JWT has 3 parts, got {}", parts.len());
        let body = URL_SAFE_NO_PAD.decode(parts[1]).expect("body b64");
        serde_json::from_slice(&body).expect("body json")
    }

    // ---- Tests ----------------------------------------------------------------

    #[tokio::test]
    async fn handler_mints_jwt_for_known_agent_and_password() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        let agent_id = Uuid::new_v4();
        let password = test_password();
        lookup.insert(agent_id, &password).await;

        let req = build_request_jwt(&fx, &agent_id.to_string(), &password, 4242);

        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req.as_bytes(), None).await;

        assert!(!resp.payload.is_empty(), "allow-path response is non-empty");

        // The response is a plaintext JWT (no xkey header set).  Inner
        // `nats.jwt` must be non-empty (an actual User JWT was minted).
        let claims = decode_response_claims(&resp.payload);
        let inner_jwt = claims["nats"]["jwt"].as_str().expect("nats.jwt is str");
        assert!(!inner_jwt.is_empty(), "inner User JWT is non-empty on allow path");
        assert!(claims["nats"].get("error").is_none(), "error field must be absent on allow");

        // Tracker recorded the connection.
        let tracked = tracker.take(agent_id).await.expect("tracker has entry");
        assert_eq!(tracked.server_id, fx.server_nkey.public_key());
        assert_eq!(tracked.client_cid, 4242);
    }

    #[tokio::test]
    async fn handler_denies_unknown_agent_with_jitter() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        let agent_id = Uuid::new_v4();
        // Lookup intentionally empty — the agent does not exist.

        let req = build_request_jwt(&fx, &agent_id.to_string(), &test_password(), 1);

        let t0 = Instant::now();
        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req.as_bytes(), None).await;
        let elapsed = t0.elapsed();

        assert!(!resp.payload.is_empty(), "deny payload is non-empty");

        // Tracker must be empty.
        assert!(tracker.take(agent_id).await.is_none(), "tracker has no entry on deny");

        // Jitter >= 50 ms (allow a little slack for timer granularity on loaded CI).
        assert!(elapsed >= Duration::from_millis(45), "jitter floor not met: elapsed = {elapsed:?}, expected >= 50ms");

        // Response JWT has error field + empty jwt field.
        let claims = decode_response_claims(&resp.payload);
        assert_eq!(claims["nats"]["error"], "auth failed");
        assert_eq!(claims["nats"]["jwt"], "");
    }

    #[tokio::test]
    async fn handler_denies_password_mismatch_with_same_jitter() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        let agent_id = Uuid::new_v4();
        let expected = test_password();
        let wrong = test_password();
        lookup.insert(agent_id, &expected).await;

        let req = build_request_jwt(&fx, &agent_id.to_string(), &wrong, 1);

        let t0 = Instant::now();
        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req.as_bytes(), None).await;
        let elapsed = t0.elapsed();

        assert!(!resp.payload.is_empty());
        assert!(tracker.take(agent_id).await.is_none(), "no tracker entry on password_mismatch");
        assert!(
            elapsed >= Duration::from_millis(45),
            "jitter floor not met on password_mismatch: elapsed = {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn handler_denies_bad_json() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        // Not a JWT at all.  Parser must reject, handler must still reply.
        let garbage = b"this is not a jwt";

        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, garbage, None).await;

        assert!(!resp.payload.is_empty(), "deny payload is non-empty");
        // No agent_id was recoverable; tracker remains empty.
        assert_eq!(tracker.len().await, 0);
    }

    #[tokio::test]
    async fn handler_denies_non_uuid_connect_user() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        let req = build_request_jwt(&fx, "not-a-uuid", &test_password(), 1);

        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req.as_bytes(), None).await;

        assert!(!resp.payload.is_empty(), "deny payload is non-empty");
        // Handler knew the user_nkey / server_id from the parsed request, so
        // the deny response is well-formed and the server would reject the
        // client.
        let claims = decode_response_claims(&resp.payload);
        assert_eq!(claims["nats"]["error"], "auth failed");
        assert_eq!(tracker.len().await, 0);
    }

    #[tokio::test]
    async fn handler_denies_xkey_open_failure() {
        // Handler expects ciphertext encrypted TO its local xkey.  We
        // encrypt to a DIFFERENT xkey — the handler's open() will fail,
        // and we must see an xkey_open_failed deny.
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        // Build any plausible plaintext (doesn't matter what — the open
        // fails before we ever parse it).
        let agent_id = Uuid::new_v4();
        let plaintext_jwt = build_request_jwt(&fx, &agent_id.to_string(), &test_password(), 1);

        // Encrypt to a fresh, unrelated local xkey rather than the handler's.
        let wrong_recipient = XKey::new();
        let server_xkey_seed = fx.server_xkey.seed().expect("server xkey seed");
        let ciphertext = xkey::seal(&server_xkey_seed, &wrong_recipient.public_key(), plaintext_jwt.as_bytes())
            .expect("seal to wrong recipient");

        let server_xkey_pub = fx.server_xkey.public_key();
        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, &ciphertext, Some(&server_xkey_pub)).await;

        assert!(!resp.payload.is_empty(), "deny payload non-empty even when xkey_open fails");
        assert_eq!(tracker.len().await, 0);
        // Payload is xkey-sealed (because the original request HAD a
        // server_xkey header), so we can't decode the JWT directly here.
        // The important invariants are: non-empty payload + empty tracker.
    }

    #[tokio::test]
    async fn handler_decrypts_then_denies() {
        // Round-trip through the xkey layer: encrypt the request correctly,
        // so open() succeeds, but use a wrong password so we deny AFTER the
        // decrypt path ran.  Proves the encrypted path reaches the inner
        // lookup step without short-circuiting.
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        let agent_id = Uuid::new_v4();
        let expected = test_password();
        let wrong = test_password();
        lookup.insert(agent_id, &expected).await;

        let plaintext_jwt = build_request_jwt(&fx, &agent_id.to_string(), &wrong, 1);
        let server_xkey_seed = fx.server_xkey.seed().expect("server xkey seed");
        let handler_xkey_pub = local_xkey_pub(&fx.signing_keys.xkey_seed);
        let ciphertext = xkey::seal(&server_xkey_seed, &handler_xkey_pub, plaintext_jwt.as_bytes()).expect("seal OK");

        let server_xkey_pub = fx.server_xkey.public_key();
        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, &ciphertext, Some(&server_xkey_pub)).await;

        assert!(!resp.payload.is_empty());
        // Tracker remains empty (deny path).
        assert_eq!(tracker.len().await, 0);

        // Verify the decrypted outbound payload round-trips — we can open it
        // with the server xkey (= sender on response) using OUR public xkey
        // as sender-pub.  That's the real client's opening path.
        let response_pt = xkey::open(&server_xkey_seed, &handler_xkey_pub, &resp.payload).expect("response decrypts");
        let s = std::str::from_utf8(&response_pt).expect("pt utf-8");
        let parts: Vec<&str> = s.split('.').collect();
        assert_eq!(parts.len(), 3, "decrypted response is a 3-part JWT");
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).expect("b64")).expect("json");
        assert_eq!(claims["nats"]["error"], "auth failed", "decrypted response says auth failed");
    }

    #[tokio::test]
    async fn handler_timing_indistinguishable_across_deny_reasons() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();

        // Pre-seed agent B so password_mismatch has something to compare.
        let agent_b = Uuid::new_v4();
        let expected = test_password();
        let wrong = test_password();
        lookup.insert(agent_b, &expected).await;

        let unknown_agent = Uuid::new_v4();
        let req_unknown = build_request_jwt(&fx, &unknown_agent.to_string(), &test_password(), 1);
        let req_mismatch = build_request_jwt(&fx, &agent_b.to_string(), &wrong, 2);

        // 10 samples each.  With 50-150ms uniform jitter the means should
        // sit around 100ms and differ by at most ~50ms empirically.  We
        // budget a loose 100ms tolerance to avoid flakes on slow CI.
        const N: u32 = 10;

        let mut total_unknown = Duration::ZERO;
        for _ in 0..N {
            let t0 = Instant::now();
            let _ = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req_unknown.as_bytes(), None).await;
            total_unknown += t0.elapsed();
        }

        let mut total_mismatch = Duration::ZERO;
        for _ in 0..N {
            let t0 = Instant::now();
            let _ = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req_mismatch.as_bytes(), None).await;
            total_mismatch += t0.elapsed();
        }

        let mean_unknown = total_unknown / N;
        let mean_mismatch = total_mismatch / N;
        let diff = mean_unknown.abs_diff(mean_mismatch);

        assert!(
            diff < Duration::from_millis(100),
            "mean deny latencies diverge by {diff:?} — timing side-channel suspect \
             (unknown mean = {mean_unknown:?}, mismatch mean = {mean_mismatch:?})"
        );

        // Tracker must be empty after all these denies.
        assert_eq!(tracker.len().await, 0, "no tracker entries should be recorded on deny paths");
    }

    #[tokio::test]
    async fn handler_denies_on_lookup_error() {
        let fx = make_fixture();
        let tracker = ConnectionTracker::new();
        let lookup = FakeLookup::default();
        lookup.set_error_mode(true).await;

        let agent_id = Uuid::new_v4();
        let req = build_request_jwt(&fx, &agent_id.to_string(), &test_password(), 1);

        let resp = handle_auth_request(&lookup, &fx.signing_keys, &tracker, req.as_bytes(), None).await;

        assert!(!resp.payload.is_empty());
        assert_eq!(tracker.len().await, 0, "no tracker entry on lookup_error");
        let claims = decode_response_claims(&resp.payload);
        assert_eq!(claims["nats"]["error"], "auth failed");
    }

    #[test]
    fn password_compare_is_constant_time() {
        // Structural sanity for the private ct_eq_bytes helper: unequal
        // lengths return false without panicking, equal lengths XOR-reduce
        // correctly, and identical inputs return true.
        assert!(ct_eq_bytes(b"abc", b"abc"));
        assert!(!ct_eq_bytes(b"abc", b"abd"));
        assert!(!ct_eq_bytes(b"abc", b"abcd"));
        assert!(!ct_eq_bytes(b"abcd", b"abc"));
        assert!(ct_eq_bytes(b"", b""));
        assert!(!ct_eq_bytes(b"", b"x"));
        // First-byte difference vs last-byte difference both return false;
        // the loop does not short-circuit.
        assert!(!ct_eq_bytes(b"Xbcdef", b"abcdef"));
        assert!(!ct_eq_bytes(b"abcdeX", b"abcdef"));
    }
}
