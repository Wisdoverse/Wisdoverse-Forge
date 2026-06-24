//! NATS XKey seal/open primitives for the auth callout end-to-end encryption
//! layer.
//!
//! # Wire format (byte-exact with nats-io/nkeys Go `xkeys.go`)
//!
//! ```text
//! [ 4 bytes "xkv1" ][ 24 bytes XSalsa20 nonce ][ N + 16 bytes NaCl box ciphertext ]
//! ```
//!
//! - `"xkv1"` is the literal ASCII version tag (`XKEY_VERSION_V1`).
//! - The 24-byte nonce is generated from the OS CSPRNG on every seal.
//! - The ciphertext portion is `nacl.Box.Seal(plaintext, nonce, recipient_pub,
//!   sender_priv)` which is Curve25519 key agreement + XSalsa20-Poly1305
//!   authenticated encryption (the trailing 16 bytes are the Poly1305 tag).
//!
//! Cross-compat is verified by the upstream `nkeys` crate's `open_from_go`
//! test, which decrypts a ciphertext produced by the Go reference and asserts
//! the plaintext matches — meaning a full interop test already ships in the
//! dependency. We don't duplicate it here, only layer our error contract on
//! top and add regression coverage for CVE-2023-46129.
//!
//! # Implementation path
//!
//! **Path A (chosen): `nkeys::XKey`.** The Rust `nkeys` crate version 0.4.5
//! re-exports `XKey` from its `xkeys` module with `seal(&self, input,
//! &recipient_xkey)` and `open(&self, input, &sender_xkey)` methods that emit
//! and consume exactly the wire format above. Internally it uses
//! `crypto_box::SalsaBox` on borrowed `&PublicKey` / `&SecretKey` references,
//! which cannot trigger the pre-0.4.6 Go buffer-by-value mutation class of bug
//! behind CVE-2023-46129 — but we pin `nkeys >= 0.4.5` in
//! `rust/Cargo.toml` anyway so the floor is explicit, and we add a regression
//! test that asserts 10 seals with varied plaintexts produce distinct
//! ciphertexts with non-zero 32-byte prefixes (`"xkv1"` + nonce).
//!
//! Path B (driving `crypto_box` directly and re-deriving the `"xkv1"` prefix
//! plus 24-byte nonce layout) was considered as a fallback if `XKey::seal` /
//! `open` did not exist in the published crate; since they do, we skip the
//! fallback and all its adjacent risk of re-implementing the wire format
//! incorrectly.
//!
//! # API contract
//!
//! - `seal` requires the sender's xkey seed because NaCl box is an
//!   authenticated (not a sealed-box) construction: the recipient must be able
//!   to verify the sender via `open(sender_pub, …)`. Callers that want an
//!   ephemeral sender generate a throwaway `XKey::new()` themselves and pass
//!   both its seed and its public key alongside the ciphertext.
//! - Errors never leak cryptographic details to callers: `DecryptionFailed`
//!   covers wrong recipient key, sender impersonation, MAC failure, and
//!   truncation that makes it past the length check. `InvalidKey` covers
//!   mis-prefixed or mis-encoded nkey strings. `InvalidLength` is only
//!   returned when the blob is shorter than the `"xkv1"` + 24-byte-nonce
//!   header.

use nkeys::XKey;

/// Structured errors for the XKey seal/open API.
///
/// Deliberately coarse — callers should not be able to distinguish
/// between "wrong key", "MAC failure", and "truncated ciphertext" (all map to
/// `DecryptionFailed`) because leaking that distinction is a padding-oracle
/// style risk for the upper-layer JWT envelope.
#[derive(Debug, thiserror::Error)]
pub enum XkeyError {
    /// The provided nkey string did not decode as an XKey seed or public key.
    /// Covers wrong prefix (`U…` instead of `X…`), bad checksum, and bad
    /// base32 encoding.
    #[error("invalid xkey: {0}")]
    InvalidKey(String),

    /// The ciphertext was shorter than the `"xkv1"` header + 24-byte nonce,
    /// so it is definitionally not an XKey envelope.
    #[error("invalid ciphertext length")]
    InvalidLength,

    /// The MAC did not verify. Indistinguishable to the caller from a wrong
    /// recipient seed, wrong sender public key, or mid-flight tampering.
    #[error("decryption failed")]
    DecryptionFailed,
}

/// Length of the `"xkv1"` version prefix in the wire format.
const XKEY_PREFIX_LEN: usize = 4;

/// Length of the XSalsa20 nonce immediately following the prefix.
const XKEY_NONCE_LEN: usize = 24;

/// Combined length of the fixed-size header (`"xkv1"` + nonce) that must
/// precede any ciphertext body. Anything shorter is `InvalidLength`.
const XKEY_HEADER_LEN: usize = XKEY_PREFIX_LEN + XKEY_NONCE_LEN;

/// Seal `plaintext` to the XKey public key `recipient_xkey_pub`, authenticated
/// by the sender's XKey seed `sender_xkey_seed`.
///
/// Returns a `Vec<u8>` laid out as `"xkv1"` || nonce(24) || box_ciphertext.
///
/// The recipient opens the result with
/// `open(recipient_seed, sender_xkey_pub, &ciphertext)`.
pub fn seal(sender_xkey_seed: &str, recipient_xkey_pub: &str, plaintext: &[u8]) -> Result<Vec<u8>, XkeyError> {
    let sender = XKey::from_seed(sender_xkey_seed).map_err(|e| XkeyError::InvalidKey(format!("sender seed: {e}")))?;
    let recipient =
        XKey::from_public_key(recipient_xkey_pub).map_err(|e| XkeyError::InvalidKey(format!("recipient pub: {e}")))?;

    sender
        .seal(plaintext, &recipient)
        // `seal` only fails when the sender has no private key, which we
        // just constructed from a seed — impossible in this code path.
        // Map to DecryptionFailed defensively rather than panic.
        .map_err(|_| XkeyError::DecryptionFailed)
}

/// Open an XKey envelope produced by `seal`.
///
/// `recipient_xkey_seed` is the local private key. `sender_xkey_pub` is the
/// counterparty's public key, used to authenticate the MAC. Both are required
/// — NaCl box is not a sealed-box construction.
pub fn open(recipient_xkey_seed: &str, sender_xkey_pub: &str, ciphertext: &[u8]) -> Result<Vec<u8>, XkeyError> {
    if ciphertext.len() <= XKEY_HEADER_LEN {
        // <= because even a zero-byte plaintext seals to header + 16-byte MAC,
        // so anything at-or-below the header length cannot be a valid blob.
        return Err(XkeyError::InvalidLength);
    }

    let recipient =
        XKey::from_seed(recipient_xkey_seed).map_err(|e| XkeyError::InvalidKey(format!("recipient seed: {e}")))?;
    let sender =
        XKey::from_public_key(sender_xkey_pub).map_err(|e| XkeyError::InvalidKey(format!("sender pub: {e}")))?;

    recipient.open(ciphertext, &sender).map_err(|_| XkeyError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nkeys::XKey;

    /// Convenience: fresh sender+recipient pair with materialised seed strings
    /// for the happy-path tests.
    fn pair() -> (XKey, String, String, XKey, String, String) {
        let sender = XKey::new();
        let s_seed = sender.seed().expect("sender has seed");
        let s_pub = sender.public_key();
        let receiver = XKey::new();
        let r_seed = receiver.seed().expect("receiver has seed");
        let r_pub = receiver.public_key();
        (sender, s_seed, s_pub, receiver, r_seed, r_pub)
    }

    #[test]
    fn seal_open_roundtrip() {
        let (_s, s_seed, s_pub, _r, r_seed, r_pub) = pair();
        let plaintext = b"jwt.user.claims.payload".to_vec();

        let ct = seal(&s_seed, &r_pub, &plaintext).expect("seal succeeds");
        let pt = open(&r_seed, &s_pub, &ct).expect("open succeeds");

        assert_eq!(pt, plaintext);
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let (_s, s_seed, s_pub, _r, r_seed, r_pub) = pair();

        let ct = seal(&s_seed, &r_pub, b"").expect("seal empty");
        let pt = open(&r_seed, &s_pub, &ct).expect("open empty");

        assert!(pt.is_empty());
    }

    #[test]
    fn wrong_recipient_fails() {
        let (_s, s_seed, s_pub, _r, _real_r_seed, r_pub) = pair();
        let plaintext = b"classified";

        let ct = seal(&s_seed, &r_pub, plaintext).expect("seal succeeds");

        // Attacker holds a different recipient seed (same prefix, wrong material).
        let other = XKey::new();
        let other_seed = other.seed().expect("other has seed");
        let err = open(&other_seed, &s_pub, &ct).expect_err("wrong key must fail");

        assert!(matches!(err, XkeyError::DecryptionFailed), "expected DecryptionFailed, got {err:?}");
    }

    #[test]
    fn truncated_ciphertext_fails() {
        let (_s, s_seed, _s_pub, _r, r_seed, r_pub) = pair();
        let plaintext = b"x";
        let ct = seal(&s_seed, &r_pub, plaintext).expect("seal succeeds");

        // Strictly shorter than header — must be InvalidLength.
        let short = &ct[..XKEY_HEADER_LEN - 1];
        let sender_pub = XKey::from_seed(&s_seed).unwrap().public_key();
        let err = open(&r_seed, &sender_pub, short).expect_err("truncated must fail");
        assert!(
            matches!(err, XkeyError::InvalidLength),
            "expected InvalidLength for {}-byte blob, got {err:?}",
            short.len()
        );

        // One byte past header — header is intact but the Poly1305 tag is
        // missing. Must surface as DecryptionFailed (NOT InvalidLength — our
        // length check only rejects blobs that cannot possibly have a body).
        let header_plus_one = &ct[..XKEY_HEADER_LEN + 1];
        let err2 = open(&r_seed, &sender_pub, header_plus_one).expect_err("header+1 byte must fail to decrypt");
        assert!(
            matches!(err2, XkeyError::DecryptionFailed),
            "expected DecryptionFailed for header+1 blob, got {err2:?}"
        );
    }

    #[test]
    fn invalid_xkey_prefix_fails() {
        // Generate a User (prefix `U`) keypair and try to pass its public key
        // as the recipient XKey. seal must reject with InvalidKey.
        let user = nkeys::KeyPair::new_user();
        let user_pub = user.public_key();
        assert!(user_pub.starts_with('U'), "sanity: user keys start with U");

        let sender = XKey::new();
        let sender_seed = sender.seed().expect("sender has seed");

        let err = seal(&sender_seed, &user_pub, b"payload").expect_err("U-prefix must reject");
        assert!(matches!(err, XkeyError::InvalidKey(_)), "expected InvalidKey, got {err:?}");

        // Also verify the sender-seed side: a User SEED (SU…) is not an xkey
        // seed (SX…). seal must reject it up front.
        let user_seed = user.seed().expect("user seed");
        let recipient = XKey::new();
        let recipient_pub = recipient.public_key();
        let err2 = seal(&user_seed, &recipient_pub, b"payload").expect_err("U-prefix seed must reject as sender");
        assert!(matches!(err2, XkeyError::InvalidKey(_)), "expected InvalidKey for U-prefix seed, got {err2:?}");
    }

    /// CVE-2023-46129 regression. Pre-0.4.6 nkeys on Go encrypted to an
    /// all-zero recipient key because of a buffer-by-value mutation bug,
    /// producing identical ciphertext prefixes across unrelated seals. The
    /// Rust crate was never known to be vulnerable (it borrows `&PublicKey`
    /// and `&SecretKey`), but this test asserts the property so that any
    /// future regression that re-introduces a static / all-zero key path in
    /// our call chain fails loudly.
    ///
    /// Two invariants:
    /// 1. All 10 ciphertexts are distinct (no key OR nonce collision).
    /// 2. At least one of the 32-byte prefixes is NOT all zero — i.e. the
    ///    `"xkv1"` + nonce header actually carries entropy.
    #[test]
    fn cve_2023_46129_regression() {
        let (_s, s_seed, s_pub, _r, r_seed, r_pub) = pair();

        let plaintexts: [&[u8]; 10] = [
            b"alpha",
            b"bravo-42",
            b"charlie",
            b"delta delta delta",
            b"echo\n\n\0",
            b"foxtrot-with-padding-to-force-block-boundary-xxxxxxxxxxxxxxxx",
            b"golf",
            b"hotel-0",
            b"india",
            b"juliet zulu alfa bravo charlie delta",
        ];

        let ciphertexts: Vec<Vec<u8>> =
            plaintexts.iter().map(|pt| seal(&s_seed, &r_pub, pt).expect("seal succeeds")).collect();

        // Distinctness.
        for i in 0..ciphertexts.len() {
            for j in (i + 1)..ciphertexts.len() {
                assert_ne!(ciphertexts[i], ciphertexts[j], "ciphertexts {i} and {j} collided — nonce or key reuse");
            }
        }

        // At least one ciphertext has a non-zero first-32-byte prefix. The
        // prefix is `"xkv1"` (4 bytes, non-zero) + 24 bytes of nonce + first
        // 4 bytes of body, so in practice every entry trivially satisfies
        // this — if any are all-zero, the `"xkv1"` constant itself was
        // overwritten, which is exactly the failure mode we want to detect.
        let any_nonzero = ciphertexts.iter().any(|c| !c[..32].iter().all(|&b| b == 0));
        assert!(any_nonzero, "every ciphertext had all-zero first 32 bytes");

        // And decrypt them all with the real recipient seed to make sure we
        // didn't silently produce junk.
        for (i, ct) in ciphertexts.iter().enumerate() {
            let pt = open(&r_seed, &s_pub, ct).expect("open succeeds");
            assert_eq!(pt.as_slice(), plaintexts[i]);
        }
    }

    /// Freshness: two seals of the same plaintext to the same recipient with
    /// the same sender seed must produce different ciphertexts. The 24-byte
    /// nonce is generated from OsRng per call, so the probability of
    /// collision is ~2^-96.
    #[test]
    fn nonce_uniqueness() {
        let (_s, s_seed, _s_pub, _r, _r_seed, r_pub) = pair();
        let plaintext = b"same-input-different-nonce";

        let ct1 = seal(&s_seed, &r_pub, plaintext).expect("seal 1");
        let ct2 = seal(&s_seed, &r_pub, plaintext).expect("seal 2");

        assert_ne!(ct1, ct2, "identical seals — nonce reuse detected");
        // Specifically the nonce portion should differ; everything before the
        // nonce is the constant `"xkv1"` prefix and everything after depends
        // on the nonce, so checking the full blob is equivalent.
        assert_eq!(&ct1[..XKEY_PREFIX_LEN], &ct2[..XKEY_PREFIX_LEN], "prefix should still be xkv1");
        assert_ne!(
            &ct1[XKEY_PREFIX_LEN..XKEY_PREFIX_LEN + XKEY_NONCE_LEN],
            &ct2[XKEY_PREFIX_LEN..XKEY_PREFIX_LEN + XKEY_NONCE_LEN],
            "nonces should differ"
        );
    }
}
