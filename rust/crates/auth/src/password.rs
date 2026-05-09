//! Argon2id password hashing and verification.
//!
//! Uses Argon2id with OWASP-recommended default parameters via the `argon2` crate.
//! Passwords are hashed with a random salt and stored in PHC string format.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use sha2::{Digest, Sha256};

/// Result of verifying a stored password hash against a plaintext password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordVerification {
    pub valid: bool,
    pub needs_upgrade: bool,
}

/// Hash a password with Argon2id and a random salt.
///
/// Returns the hash in PHC string format (e.g. `$argon2id$v=19$m=...`).
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default(); // Argon2id with safe defaults
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against a stored PHC-format hash.
///
/// Returns `Ok(true)` if the password matches, `Ok(false)` if it does not.
/// Returns `Err` only if the hash string is malformed.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
}

/// Verify a password against current and legacy hash formats.
///
/// Supported formats:
/// - Argon2 PHC strings: valid, no upgrade needed.
/// - bcrypt hashes: valid, should be upgraded to Argon2 on successful login.
/// - legacy SHA-256 hex digests: valid, should be upgraded to Argon2 on successful login.
pub fn verify_password_compat(password: &str, hash: &str) -> PasswordVerification {
    if hash.starts_with("$argon2") {
        return PasswordVerification { valid: verify_password(password, hash).unwrap_or(false), needs_upgrade: false };
    }

    if is_bcrypt_hash(hash) {
        return PasswordVerification { valid: bcrypt::verify(password, hash).unwrap_or(false), needs_upgrade: true };
    }

    if is_sha256_hex(hash) {
        let digest = Sha256::digest(password.as_bytes());
        let expected = format!("{digest:x}");
        return PasswordVerification { valid: expected.eq_ignore_ascii_case(hash), needs_upgrade: true };
    }

    PasswordVerification { valid: false, needs_upgrade: false }
}

fn is_bcrypt_hash(hash: &str) -> bool {
    hash.starts_with("$2a$") || hash.starts_with("$2b$") || hash.starts_with("$2x$") || hash.starts_with("$2y$")
}

fn is_sha256_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_correct_password() {
        let hash = hash_password("my-secure-password").unwrap();
        assert!(verify_password("my-secure-password", &hash).unwrap());
    }

    #[test]
    fn wrong_password_returns_false() {
        let hash = hash_password("correct-password").unwrap();
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn hash_is_phc_format() {
        let hash = hash_password("test").unwrap();
        // PHC string format starts with $argon2id$
        assert!(hash.starts_with("$argon2id$"), "hash was: {hash}");
    }

    #[test]
    fn different_calls_produce_different_hashes() {
        let h1 = hash_password("same-password").unwrap();
        let h2 = hash_password("same-password").unwrap();
        // Different salts → different hashes
        assert_ne!(h1, h2);
        // But both verify
        assert!(verify_password("same-password", &h1).unwrap());
        assert!(verify_password("same-password", &h2).unwrap());
    }

    #[test]
    fn malformed_hash_returns_error() {
        let result = verify_password("password", "not-a-valid-hash");
        assert!(result.is_err());
    }

    #[test]
    fn compat_verifies_argon2_without_upgrade() {
        let hash = hash_password("compat-password").unwrap();
        let result = verify_password_compat("compat-password", &hash);
        assert!(result.valid);
        assert!(!result.needs_upgrade);
    }

    #[test]
    fn compat_verifies_bcrypt_with_upgrade_flag() {
        let hash = bcrypt::hash("legacy-password", 12).unwrap();
        let result = verify_password_compat("legacy-password", &hash);
        assert!(result.valid);
        assert!(result.needs_upgrade);
    }

    #[test]
    fn compat_verifies_sha256_hex_with_upgrade_flag() {
        let hash = format!("{:x}", Sha256::digest(b"legacy-password"));
        let result = verify_password_compat("legacy-password", &hash);
        assert!(result.valid);
        assert!(result.needs_upgrade);
    }

    #[test]
    fn compat_rejects_unknown_hash_format() {
        let result = verify_password_compat("password", "legacy:opaque");
        assert!(!result.valid);
        assert!(!result.needs_upgrade);
    }
}
