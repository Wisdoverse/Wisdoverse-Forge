//! NATS `ed25519-nkey` JWT encode/sign/parse primitives for the auth callout
//! exchange (ADR-26).
//!
//! # Why hand-rolled?
//!
//! NATS uses a custom JWT header (`{"typ":"JWT","alg":"ed25519-nkey"}`) that
//! `jsonwebtoken` rejects, so we assemble the token ourselves. All three
//! segments use the same codec — base64url no-pad — as the canonical
//! `nats-io/jwt` Go library's `doEncode` → `base64.RawURLEncoding.EncodeToString`.
//!
//! Earlier revisions of this module base32-encoded the signature (wrong —
//! that was the codec `nkeys` uses for key *strings*, not for JWT
//! signatures). NATS server v2.12 rejected the resulting JWTs with
//! `Claim failed V2 signature verification`. Fixed with the issue-#55
//! follow-up; the crypto inputs are unchanged, only the outer encoding
//! layer matches the spec now.
//!
//! Signature bytes come from `nkeys::KeyPair::sign` (64-byte ed25519 over
//! the ASCII `"<header_b64>.<claims_b64>"` string) and are base64url
//! encoded without padding before being appended as the third segment.
//!
//! # Primitives exposed
//!
//! | Function                              | Purpose                                                            |
//! | ------------------------------------- | ------------------------------------------------------------------ |
//! | [`sign_user_jwt`]                     | Build inner User JWT with pub/sub permissions (signed by `SA…`).    |
//! | [`sign_authorization_response_jwt`]   | Build outer AuthorizationResponse wrapping inner JWT or an error.   |
//! | [`parse_authorization_request`]       | Decode incoming request payload (signature NOT verified — see §).   |
//!
//! # Why `parse_authorization_request` does not verify the signature
//!
//! The request arrives wrapped in the [`super::xkey`] envelope (Curve25519 +
//! XSalsa20-Poly1305 authenticated encryption), so transport integrity is
//! already covered by Unit 4. The server's nkey public is also not available
//! at this layer — it lives in the NATS server config, not in our app config —
//! so verifying here would require surfacing that key across an interface
//! boundary we do not currently have. If a future unit adds the server nkey to
//! `AppConfig`, add a separate `verify_authorization_request` helper rather
//! than changing the parse contract.
//!
//! # Relevant specs / references
//!
//! - ADR-26 "NATS Authorization Callouts":
//!   <https://github.com/nats-io/nats-architecture-and-design/blob/main/adr/ADR-26.md>
//! - Go reference (exact JSON field names + base32 signature handling):
//!   <https://github.com/synadia-io/callout.go>
//! - `nkeys` Rust crate (underlying ed25519 + nkey string codec):
//!   <https://docs.rs/nkeys>

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use nkeys::KeyPair;
#[cfg(test)]
use secrecy::ExposeSecret;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Structured error surface for the JWT primitives.
///
/// Callers never leak these to external clients — they are internal enough to
/// be specific about which stage failed so operators can diagnose nkey prefix
/// mismatches vs. JSON serialization faults vs. malformed inputs.
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    /// Input that could not be parsed: malformed JWT string (wrong dot count,
    /// non-UTF8 segments, non-JSON payload), or keys with the wrong nkey
    /// prefix.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The `nkeys::KeyPair::sign` call failed. In practice this only happens
    /// if the seed-derived keypair has no private half, which should be
    /// impossible after `from_seed`, but we surface it as a recoverable
    /// error rather than panic.
    #[error("signing failed: {0}")]
    SigningFailed(String),

    /// `serde_json::to_vec` / `to_string` returned an error — realistically
    /// only possible if a claim field contains a non-serializable type
    /// (not reachable with the concrete types used here, but defensive).
    #[error("encoding failed: {0}")]
    EncodingFailed(String),
}

// -----------------------------------------------------------------------------
// Permission payload
// -----------------------------------------------------------------------------

/// Pub/sub allow/deny arrays embedded inside the inner User JWT's `nats.pub`
/// and `nats.sub` blocks.
///
/// Fields are `pub` because callers (Unit 7's allowlist builder) assemble
/// them inline. An empty `Vec` serializes to `[]` which NATS treats as
/// "deny all" for that direction — so Unit 7 must populate both sides
/// correctly or connections will silently fail to publish.
#[derive(Debug, Clone, Default)]
pub struct NatsPermissions {
    pub pub_allow: Vec<String>,
    pub pub_deny: Vec<String>,
    pub sub_allow: Vec<String>,
    pub sub_deny: Vec<String>,
}

// -----------------------------------------------------------------------------
// Request parsing
// -----------------------------------------------------------------------------

/// Parsed fields from an incoming `AuthorizationRequest` claim body.
///
/// Everything on this struct comes from the middle (claims) segment of the
/// three-part JWT. The outer signature is not checked here — see the module
/// comment for the rationale.
///
/// `Debug` is derived but `connect_password` is wrapped in [`SecretString`]
/// so accidental `tracing::debug!(?request)` or panic-dump output redacts
/// the credential to `[REDACTED …]` — matches the project-wide secret
/// handling convention documented in `CLAUDE.md`.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// `N…` — the NATS server's public key. Used as `aud` on the response.
    pub server_id: String,
    /// `U…` — per-connection user nkey the server generated. Must be echoed
    /// as `sub` on the response user JWT or the server rejects the reply.
    pub user_nkey: String,
    /// `connect_opts.user` — for our callout this is the `agent_uuid`
    /// string the sidecar passes as the NATS username.
    pub connect_user: String,
    /// `connect_opts.pass` — the per-agent `nats_connect_password` which
    /// the callout handler validates against the `NatsConnectPasswordLookup`
    /// repo. Wrapped in [`SecretString`] so `Debug` redacts the plaintext;
    /// reach the bytes at the compare site with `.expose_secret()`.
    pub connect_password: SecretString,
    /// `client_info.id` — the NATS server's internal connection ID (CID).
    /// Needed later when we issue a KICK for a revoked agent.
    pub client_cid: u64,
    /// `server_id.xkey` — ephemeral server XKey public key. Present when
    /// the server is configured with `xkey` in the callout block; `None`
    /// if XKey encryption is disabled. Required by Unit 7 to seal the
    /// outer response back to the server.
    pub server_xkey: Option<String>,
}

/// Parse the incoming AuthorizationRequest JWT body. Signature **not**
/// verified — see module docs.
///
/// # Errors
///
/// Returns [`JwtError::InvalidInput`] when:
///   - the input is not a three-part `header.body.sig` string,
///   - the body segment is not valid base64url-no-pad,
///   - the body JSON cannot be deserialized into the expected shape
///     (missing `nats.server_id.id`, wrong type on `client_info.id`, etc.).
pub fn parse_authorization_request(jwt: &str) -> Result<AuthorizationRequest, JwtError> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidInput(format!("expected 3 JWT segments, got {}", parts.len())));
    }

    // URL-safe base64 NO-PAD per NATS/JWT spec.
    let body_bytes =
        URL_SAFE_NO_PAD.decode(parts[1]).map_err(|e| JwtError::InvalidInput(format!("body not base64url: {e}")))?;

    let raw: RequestClaims = serde_json::from_slice(&body_bytes)
        .map_err(|e| JwtError::InvalidInput(format!("body not valid request JSON: {e}")))?;

    Ok(AuthorizationRequest {
        server_id: raw.nats.server_id.id,
        user_nkey: raw.nats.user_nkey,
        connect_user: raw.nats.connect_opts.user.unwrap_or_default(),
        connect_password: SecretString::from(raw.nats.connect_opts.pass.unwrap_or_default()),
        client_cid: raw.nats.client_info.id,
        server_xkey: raw.nats.server_id.xkey,
    })
}

// Internal deserialization shapes. `#[serde(default)]` on Option fields
// handles the "field absent on the wire" case without failing — the NATS
// server omits `xkey` when callout encryption is disabled, and `connect_opts`
// may omit either `user` or `pass` for unauthenticated connects.
#[derive(Deserialize)]
struct RequestClaims {
    nats: RequestNats,
}

#[derive(Deserialize)]
struct RequestNats {
    server_id: RequestServerId,
    user_nkey: String,
    #[serde(default)]
    connect_opts: RequestConnectOpts,
    client_info: RequestClientInfo,
}

#[derive(Deserialize)]
struct RequestServerId {
    id: String,
    #[serde(default)]
    xkey: Option<String>,
}

#[derive(Deserialize, Default)]
struct RequestConnectOpts {
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    pass: Option<String>,
}

#[derive(Deserialize)]
struct RequestClientInfo {
    id: u64,
}

// -----------------------------------------------------------------------------
// JWT header + claims serialization helpers
// -----------------------------------------------------------------------------

/// Canonical NATS JWT header: `{"typ":"JWT","alg":"ed25519-nkey"}`.
///
/// Declared as a function (not a `const`) so the JSON string is computed once
/// with `serde_json` to guarantee byte-exact output rather than depending on
/// source-literal spacing. Encodes identically every call.
fn nats_jwt_header() -> serde_json::Value {
    serde_json::json!({
        "typ": "JWT",
        "alg": "ed25519-nkey",
    })
}

/// Current Unix epoch seconds. Returns `0` on the astronomically unlikely
/// case of clock-before-epoch, which is a safer fallback than panicking —
/// the JWT will just fail `exp` validation downstream.
fn now_unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Sign a `header.body` string with an nkey seed keypair, base64url no-pad
/// encode the 64-byte signature, return `header.body.sig`.
fn assemble_and_sign(header_b64: &str, claims_b64: &str, signing_key_seed: &str) -> Result<String, JwtError> {
    let keypair =
        KeyPair::from_seed(signing_key_seed).map_err(|e| JwtError::InvalidInput(format!("bad signing seed: {e}")))?;

    let signing_input = format!("{header_b64}.{claims_b64}");
    let sig_bytes = keypair.sign(signing_input.as_bytes()).map_err(|e| JwtError::SigningFailed(format!("{e}")))?;

    // NATS JWT signatures are base64url no-pad — `base64.RawURLEncoding` in
    // `nats-io/jwt`. (An earlier version of this module used base32 by
    // mistake — that's the codec `nkeys` uses for key strings, not JWT
    // signatures. Server responded with `Claim failed V2 signature
    // verification`; fixed with the issue-#55 follow-up.)
    let sig_b64 = URL_SAFE_NO_PAD.encode(&sig_bytes);
    Ok(format!("{signing_input}.{sig_b64}"))
}

/// Serialize a JSON-compatible value to bytes, base64url-no-pad encode.
fn b64_json<T: Serialize>(value: &T) -> Result<String, JwtError> {
    let bytes = serde_json::to_vec(value).map_err(|e| JwtError::EncodingFailed(format!("json: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(&bytes))
}

// -----------------------------------------------------------------------------
// Inner User JWT
// -----------------------------------------------------------------------------

/// Build and sign the inner NATS User JWT embedded in the outer
/// `AuthorizationResponse`.
///
/// # Parameters
///
/// - `subject_nkey`: the per-connection `U…` public nkey the NATS server
///   generated. Passed through from `AuthorizationRequest.user_nkey`.
/// - `issuer_account_seed`: an `SA…` account signing-key seed used as the
///   inner JWT's `iss`. In server-config mode the outer AuthorizationResponse
///   signature is the trust anchor (matched against
///   `authorization.auth_callout.issuer` in `nats.conf`); the inner JWT's
///   `iss` is informational and its public half does **not** need to appear
///   in `nats.conf` — the `signing_keys` array under server-config accounts
///   is rejected by `nats-server` anyway (see issue #55).
/// - `audience_account_name`: the NATS **account name** — the identifier
///   used as the label of an `accounts { … }` block in `nats.conf`
///   (`"AGENTFORGE"` in this deployment). In server-config / non-operator
///   mode NATS maps the minted user onto an account via
///   `s.LookupAccount(userJwt.Audience)`, i.e. the `aud` claim is matched
///   against the account name string — **not** the account's public nkey.
///   Passing a public key here would yield `"no valid account … for auth
///   callout response"` and the CONNECT would fail with `-ERR Authorization
///   Violation`. Spec reference: `server/auth_callout.go` in nats-io/nats-server.
/// - `name`: human-friendly identifier, surfaces in `$SYS.ACCOUNT`
///   tooling; typically `"agent-<uuid>"`.
/// - `expires_in`: sets `exp = iat + expires_in`. Callers should pick a
///   value slightly longer than the expected agent container connection so the
///   JWT does not expire mid-connection. The JWT is a one-shot connection
///   credential; expiry drops the existing connection, it does not force a
///   reconnect.
/// - `permissions`: pub/sub allow/deny arrays embedded under `nats.pub`
///   and `nats.sub`.
///
/// # Errors
///
/// [`JwtError::InvalidInput`] if the seed is not a valid nkey seed.
/// [`JwtError::SigningFailed`] if ed25519 signing fails (seed has no
/// private half — not expected from `from_seed`).
/// [`JwtError::EncodingFailed`] if JSON serialization fails.
pub fn sign_user_jwt(
    subject_nkey: &str,
    issuer_account_seed: &str,
    audience_account_name: &str,
    name: &str,
    expires_in: Duration,
    permissions: &NatsPermissions,
) -> Result<String, JwtError> {
    let issuer_kp =
        KeyPair::from_seed(issuer_account_seed).map_err(|e| JwtError::InvalidInput(format!("bad issuer seed: {e}")))?;
    let issuer_pub = issuer_kp.public_key();

    let iat = now_unix_seconds();
    let exp = iat.saturating_add(expires_in.as_secs());
    let jti = Uuid::new_v4().to_string();

    let claims = serde_json::json!({
        "iss": issuer_pub,
        "sub": subject_nkey,
        "aud": audience_account_name,
        "iat": iat,
        "exp": exp,
        "jti": jti,
        "name": name,
        "nats": {
            "pub": {
                "allow": permissions.pub_allow,
                "deny": permissions.pub_deny,
            },
            "sub": {
                "allow": permissions.sub_allow,
                "deny": permissions.sub_deny,
            },
            "type": "user",
            "version": 2,
        },
    });

    let header_b64 = b64_json(&nats_jwt_header())?;
    let claims_b64 = b64_json(&claims)?;

    assemble_and_sign(&header_b64, &claims_b64, issuer_account_seed)
}

// -----------------------------------------------------------------------------
// Outer AuthorizationResponse JWT
// -----------------------------------------------------------------------------

/// Build and sign the outer `AuthorizationResponse` JWT the callout service
/// returns to the NATS server on `$SYS.REQ.USER.AUTH`.
///
/// # Parameters
///
/// - `audience_server_id`: the NATS server's public nkey (`N…`), taken from
///   the incoming request's `iss` field. The server rejects the response
///   if `aud` does not match its own server ID.
/// - `issuer_seed`: the callout issuer seed (`A…` / `SA…`). Its public
///   half must appear in the `authorization.auth_callout.issuer` slot of
///   `nats.conf`.
/// - `subject_user_nkey`: must equal `AuthorizationRequest.user_nkey` — the
///   server binds the response to the specific per-connection user nkey
///   it asked about.
/// - `inner_user_jwt`: output of [`sign_user_jwt`] on the allow path; set
///   to an empty string when `error` is `Some` (the server treats a
///   non-empty `nats.error` as a deny regardless of whether `nats.jwt`
///   contains a parseable token).
/// - `error`: `None` for allow, `Some("reason")` for deny. The string is
///   logged on the server side but not shown to the client (NATS only
///   returns `-ERR Authorization Violation`).
/// - `expires_in`: the outer JWT's `exp`. Short lifetimes (30-60s) are
///   fine — this JWT is validated once at connect time and discarded.
///
/// # Errors
///
/// Same shape as [`sign_user_jwt`].
pub fn sign_authorization_response_jwt(
    audience_server_id: &str,
    issuer_seed: &str,
    subject_user_nkey: &str,
    inner_user_jwt: &str,
    error: Option<&str>,
    expires_in: Duration,
) -> Result<String, JwtError> {
    let issuer_kp =
        KeyPair::from_seed(issuer_seed).map_err(|e| JwtError::InvalidInput(format!("bad issuer seed: {e}")))?;
    let issuer_pub = issuer_kp.public_key();

    let iat = now_unix_seconds();
    let exp = iat.saturating_add(expires_in.as_secs());
    let jti = Uuid::new_v4().to_string();

    // `nats.error` must be omitted entirely on allow (not set to empty
    // string or null) — the upstream Go reference uses `omitempty`.
    let nats_inner = if let Some(err) = error {
        serde_json::json!({
            "jwt": inner_user_jwt,
            "error": err,
            "type": "authorization_response",
            "version": 2,
        })
    } else {
        serde_json::json!({
            "jwt": inner_user_jwt,
            "type": "authorization_response",
            "version": 2,
        })
    };

    let claims = serde_json::json!({
        "iss": issuer_pub,
        "sub": subject_user_nkey,
        "aud": audience_server_id,
        "iat": iat,
        "exp": exp,
        "jti": jti,
        "nats": nats_inner,
    });

    let header_b64 = b64_json(&nats_jwt_header())?;
    let claims_b64 = b64_json(&claims)?;

    assemble_and_sign(&header_b64, &claims_b64, issuer_seed)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nkeys::KeyPair;
    use serde_json::Value;

    /// Build a fresh account signing keypair for tests.
    fn fresh_account_kp() -> (KeyPair, String, String) {
        let kp = KeyPair::new_account();
        let seed = kp.seed().expect("account has seed");
        let pub_ = kp.public_key();
        (kp, seed, pub_)
    }

    fn fresh_user_pub() -> String {
        KeyPair::new_user().public_key()
    }

    fn fresh_server_pub() -> String {
        KeyPair::new_server().public_key()
    }

    /// Decode the claims segment (middle) of a three-part JWT.
    fn decode_claims(jwt: &str) -> Value {
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "expected 3 segments, got {}", parts.len());
        let bytes = URL_SAFE_NO_PAD.decode(parts[1]).expect("body is valid b64");
        serde_json::from_slice(&bytes).expect("body is valid JSON")
    }

    /// Decode the signature segment (third) of a three-part JWT.
    fn decode_signature(jwt: &str) -> Vec<u8> {
        let parts: Vec<&str> = jwt.split('.').collect();
        URL_SAFE_NO_PAD.decode(parts[2].as_bytes()).expect("sig is base64url")
    }

    #[test]
    fn sign_user_jwt_produces_three_part_token() {
        let (_kp, seed, _pub_) = fresh_account_kp();
        let user = fresh_user_pub();
        let perms = NatsPermissions::default();

        let jwt = sign_user_jwt(&user, &seed, "AGENTFORGE", "agent-1", Duration::from_secs(60), &perms).expect("signs");

        assert_eq!(jwt.matches('.').count(), 2, "must have exactly 2 dots");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
        assert!(!parts[2].is_empty());
    }

    #[test]
    fn sign_user_jwt_signature_verifies() {
        let (kp, seed, pub_) = fresh_account_kp();
        let user = fresh_user_pub();
        let perms = NatsPermissions::default();

        let jwt = sign_user_jwt(&user, &seed, "AGENTFORGE", "agent-1", Duration::from_secs(60), &perms).expect("signs");

        let parts: Vec<&str> = jwt.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig = decode_signature(&jwt);

        // Verify with the account public key via the raw KeyPair::verify API.
        kp.verify(signing_input.as_bytes(), &sig).expect("signature verifies against issuer public key");

        // Also assert verify via a public-only keypair — proves `iss` is
        // what a plain NATS server sees from the wire.
        let verifier = KeyPair::from_public_key(&pub_).expect("account pub round-trips");
        verifier.verify(signing_input.as_bytes(), &sig).expect("verifies via public-only keypair");
    }

    #[test]
    fn sign_user_jwt_aud_is_account_name_not_public_key() {
        // Pins the issue-55 follow-up: in server-config / non-operator mode
        // NATS maps callout-minted users onto an account via
        // `LookupAccount(userJwt.Audience)`, so the `aud` claim MUST be the
        // account NAME string (e.g. "AGENTFORGE"), not an `A…` public nkey.
        // A regression that passed an account public key here would surface
        // at runtime as `no valid account "A…" for auth callout response`
        // and the agent CONNECT would be rejected.
        let (_kp, seed, pub_) = fresh_account_kp();
        let user = fresh_user_pub();

        let jwt =
            sign_user_jwt(&user, &seed, "AGENTFORGE", "agent-1", Duration::from_secs(60), &NatsPermissions::default())
                .expect("signs");

        let claims = decode_claims(&jwt);
        assert_eq!(claims["aud"], "AGENTFORGE", "aud must be the account name string");
        assert!(
            claims["aud"].as_str().map(|s| !s.starts_with('A') || s.len() != 56).unwrap_or(true),
            "regression guard: aud must NOT look like an account public nkey",
        );
        // `iss` continues to be the signing key's public half. Not a routing
        // claim in server-config mode — just informational / audit.
        assert_eq!(claims["iss"], pub_);
    }

    #[test]
    fn sign_user_jwt_embeds_permissions() {
        let (_kp, seed, pub_) = fresh_account_kp();
        let user = fresh_user_pub();
        let perms = NatsPermissions {
            pub_allow: vec!["events.ingest.>".to_string(), "sidecar.agent-1.heartbeat".to_string()],
            pub_deny: vec!["$SYS.>".to_string()],
            sub_allow: vec!["sidecar.agent-1.cmd".to_string()],
            sub_deny: vec!["events.broadcast.>".to_string()],
        };

        let jwt = sign_user_jwt(&user, &seed, "AGENTFORGE", "agent-1", Duration::from_secs(60), &perms).expect("signs");

        let claims = decode_claims(&jwt);
        let nats = &claims["nats"];
        assert_eq!(nats["type"], "user");
        assert_eq!(nats["version"], 2);
        assert_eq!(nats["pub"]["allow"][0], "events.ingest.>");
        assert_eq!(nats["pub"]["allow"][1], "sidecar.agent-1.heartbeat");
        assert_eq!(nats["pub"]["deny"][0], "$SYS.>");
        assert_eq!(nats["sub"]["allow"][0], "sidecar.agent-1.cmd");
        assert_eq!(nats["sub"]["deny"][0], "events.broadcast.>");

        // Top-level claims fields round-trip too.
        assert_eq!(claims["sub"], user);
        assert_eq!(claims["aud"], "AGENTFORGE");
        assert_eq!(claims["iss"], pub_);
        assert_eq!(claims["name"], "agent-1");
        assert!(claims["iat"].as_u64().expect("iat is u64") > 0);
        assert!(claims["exp"].as_u64().expect("exp is u64") > 0);
        assert!(claims["jti"].as_str().expect("jti is str").len() >= 32);
    }

    #[test]
    fn sign_authorization_response_jwt_embeds_inner_jwt() {
        let (_acc_kp, acc_seed, _acc_pub) = fresh_account_kp();
        let (_iss_kp, iss_seed, iss_pub) = fresh_account_kp();
        let user = fresh_user_pub();
        let server = fresh_server_pub();

        let inner = sign_user_jwt(
            &user,
            &acc_seed,
            "AGENTFORGE",
            "agent-x",
            Duration::from_secs(60),
            &NatsPermissions::default(),
        )
        .expect("inner signs");

        let outer = sign_authorization_response_jwt(&server, &iss_seed, &user, &inner, None, Duration::from_secs(30))
            .expect("outer signs");

        let claims = decode_claims(&outer);
        assert_eq!(claims["iss"], iss_pub);
        assert_eq!(claims["sub"], user);
        assert_eq!(claims["aud"], server);
        assert_eq!(claims["nats"]["jwt"], inner);
        assert_eq!(claims["nats"]["type"], "authorization_response");
        assert_eq!(claims["nats"]["version"], 2);
        assert!(claims["nats"].get("error").is_none(), "error field must be OMITTED (not null) on allow path");
    }

    #[test]
    fn sign_authorization_response_jwt_deny_path() {
        let (_iss_kp, iss_seed, _iss_pub) = fresh_account_kp();
        let user = fresh_user_pub();
        let server = fresh_server_pub();

        let outer = sign_authorization_response_jwt(
            &server,
            &iss_seed,
            &user,
            "", // inner_user_jwt is empty on deny
            Some("auth failed"),
            Duration::from_secs(30),
        )
        .expect("outer signs");

        let claims = decode_claims(&outer);
        assert_eq!(claims["nats"]["error"], "auth failed");
        assert_eq!(claims["nats"]["jwt"], "");
    }

    #[test]
    fn parse_authorization_request_extracts_all_fields() {
        // Build a synthetic request JWT. Signature doesn't need to be valid
        // because the parser never verifies it — but we still build a
        // well-formed three-part token so the split succeeds.
        let (_kp, server_nkey_seed, server_nkey_pub) = {
            let kp = KeyPair::new_server();
            let seed = kp.seed().expect("server has seed");
            let pub_ = kp.public_key();
            (kp, seed, pub_)
        };
        let user_nkey = KeyPair::new_user().public_key();
        let xkey_pub = nkeys::XKey::new().public_key();

        let claims = serde_json::json!({
            "iss": server_nkey_pub,
            "sub": user_nkey,
            "aud": "nats-authorization-request",
            "iat": 1000000000,
            "exp": 2000000000,
            "jti": "test-jti",
            "nats": {
                "server_id": {
                    "id": server_nkey_pub,
                    "xkey": xkey_pub,
                },
                "user_nkey": user_nkey,
                "connect_opts": {
                    "user": "agent-uuid-abc",
                    "pass": "the-nats-connect-password",
                },
                "client_info": {
                    "id": 4242u64,
                    "host": "127.0.0.1",
                },
                "request_nonce": "nonce-bytes",
                "type": "authorization_request",
                "version": 2,
            }
        });

        let header_b64 = b64_json(&nats_jwt_header()).unwrap();
        let claims_b64 = b64_json(&claims).unwrap();
        let jwt = assemble_and_sign(&header_b64, &claims_b64, &server_nkey_seed).expect("synthetic req signs");

        let parsed = parse_authorization_request(&jwt).expect("parse");
        assert_eq!(parsed.server_id, server_nkey_pub);
        assert_eq!(parsed.user_nkey, user_nkey);
        assert_eq!(parsed.connect_user, "agent-uuid-abc");
        assert_eq!(parsed.connect_password.expose_secret(), "the-nats-connect-password");
        assert_eq!(parsed.client_cid, 4242);
        assert_eq!(parsed.server_xkey.as_deref(), Some(xkey_pub.as_str()));

        // Also verify that server_xkey comes back None when the field is
        // absent — the NATS server omits it entirely when XKey encryption
        // is disabled.
        let claims_no_xkey = serde_json::json!({
            "iss": server_nkey_pub,
            "sub": user_nkey,
            "aud": "nats-authorization-request",
            "nats": {
                "server_id": { "id": server_nkey_pub },
                "user_nkey": user_nkey,
                "connect_opts": { "user": "u", "pass": "p" },
                "client_info": { "id": 1u64 },
                "request_nonce": "n",
                "type": "authorization_request",
                "version": 2,
            }
        });
        let claims_b64 = b64_json(&claims_no_xkey).unwrap();
        let jwt = assemble_and_sign(&header_b64, &claims_b64, &server_nkey_seed).expect("no-xkey req signs");
        let parsed = parse_authorization_request(&jwt).expect("parse no-xkey");
        assert!(parsed.server_xkey.is_none());
    }

    #[test]
    fn parse_authorization_request_rejects_malformed() {
        // No dots — single string.
        let err = parse_authorization_request("this-has-no-dots").expect_err("no-dots rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));

        // Two segments instead of three.
        let err = parse_authorization_request("aaa.bbb").expect_err("2-parts rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));

        // Four segments.
        let err = parse_authorization_request("a.b.c.d").expect_err("4-parts rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));

        // Three parts, but the middle is not valid base64url-no-pad.
        let err = parse_authorization_request("hdr.not!valid!b64.sig").expect_err("bad b64 rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));

        // Three parts, body is valid base64url but decodes to non-JSON.
        let bad_body = URL_SAFE_NO_PAD.encode(b"not json");
        let j = format!("hdr.{bad_body}.sig");
        let err = parse_authorization_request(&j).expect_err("non-json body rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));

        // Three parts, body is JSON but missing required `nats` block.
        let partial = URL_SAFE_NO_PAD.encode(br#"{"iss":"x"}"#);
        let j = format!("hdr.{partial}.sig");
        let err = parse_authorization_request(&j).expect_err("missing nats rejects");
        assert!(matches!(err, JwtError::InvalidInput(_)));
    }

    #[test]
    fn body_tampering_breaks_signature() {
        let (kp, seed, _pub_) = fresh_account_kp();
        let user = fresh_user_pub();

        let jwt =
            sign_user_jwt(&user, &seed, "AGENTFORGE", "agent-1", Duration::from_secs(60), &NatsPermissions::default())
                .expect("signs");

        let parts: Vec<&str> = jwt.split('.').collect();
        // Mutate one byte of the claims segment. Because base64url-no-pad
        // is a bijection on ASCII, flipping any character that's in the
        // b64 alphabet yields a different decoded body and therefore a
        // different signing input. We flip the last character of the
        // claims segment to another alphabet character.
        let mut body: Vec<char> = parts[1].chars().collect();
        let last = *body.last().expect("body non-empty");
        let replacement = if last == 'A' { 'B' } else { 'A' };
        let len = body.len();
        body[len - 1] = replacement;
        let tampered_body: String = body.into_iter().collect();
        let tampered_signing_input = format!("{}.{tampered_body}", parts[0]);

        let orig_sig = decode_signature(&jwt);
        assert!(
            kp.verify(tampered_signing_input.as_bytes(), &orig_sig).is_err(),
            "tampered body must fail signature verification"
        );
    }
}
