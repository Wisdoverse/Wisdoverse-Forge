//! Canonical SHA-256 / HMAC-SHA256 test vectors.
//!
//! These pin the byte output of the `sha2` and `hmac` crates to independently
//! known-good values. The 0.11/0.13 RustCrypto wave migrated the digest output
//! type from `generic_array::GenericArray` to `hybrid_array::Array`, which is
//! why the hex rendering had to move from `format!("{:x}", ..)` (a `LowerHex`
//! impl that only `GenericArray` carries) to `hex::encode(..)`. The algorithms
//! themselves are unchanged, so the bytes must be identical across the bump.
//!
//! This matters because the output of these primitives is persisted and later
//! re-verified: legacy SHA-256 password hashes (`agentforge-auth`), per-agent
//! HMAC signatures on orchestration results (`orchestration_protocol`), the
//! sidecar relay publisher, and Stripe webhook signatures. A silent byte change
//! on a future RustCrypto bump would reject every stored credential and every
//! in-flight signature; this test fails loudly instead.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

/// NIST FIPS 180-2 canonical vector: SHA-256("abc").
#[test]
fn sha256_matches_nist_abc_vector() {
    assert_eq!(hex::encode(Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",);
}

/// Widely published HMAC-SHA256 vector:
/// HMAC(key = "key", msg = "The quick brown fox jumps over the lazy dog").
#[test]
fn hmac_sha256_matches_known_vector() {
    let mut mac = Hmac::<Sha256>::new_from_slice(b"key").expect("HMAC accepts a key of any length");
    mac.update(b"The quick brown fox jumps over the lazy dog");
    assert_eq!(
        hex::encode(mac.finalize().into_bytes()),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
    );
}
