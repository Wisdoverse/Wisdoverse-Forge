use crate::domain::user::{UserEmail, UserPassword};
use crate::services::user::LoginResult;

// ---------------------------------------------------------------------------
// Email validation
// ---------------------------------------------------------------------------

#[test]
fn test_email_valid() {
    assert!(UserEmail::parse("user@example.com").is_ok());
    assert!(UserEmail::parse("dev@example.com").is_ok());
    assert!(UserEmail::parse("x@y.z").is_ok()); // minimal valid (5 chars with @)
}

#[test]
fn test_email_missing_at() {
    assert!(UserEmail::parse("invalidemail.com").is_err());
    assert!(UserEmail::parse("noemail").is_err());
}

#[test]
fn test_email_too_short() {
    assert!(UserEmail::parse("a@b").is_err()); // 3 chars
    assert!(UserEmail::parse("a@bc").is_err()); // 4 chars
}

#[test]
fn test_email_empty() {
    assert!(UserEmail::parse("").is_err());
}

#[test]
fn test_email_long_is_ok() {
    let long_email = format!("{}@example.com", "a".repeat(200));
    assert!(UserEmail::parse(&long_email).is_ok());
}

// ---------------------------------------------------------------------------
// Password validation
// ---------------------------------------------------------------------------

#[test]
fn test_password_exactly_8() {
    assert!(UserPassword::parse("12345678").is_ok());
}

#[test]
fn test_password_7_chars() {
    assert!(UserPassword::parse("1234567").is_err());
}

#[test]
fn test_password_empty() {
    assert!(UserPassword::parse("").is_err());
}

#[test]
fn test_password_long_is_ok() {
    assert!(UserPassword::parse(&"x".repeat(1000)).is_ok());
}

#[test]
fn test_password_single_char() {
    assert!(UserPassword::parse("a").is_err());
}

// ---------------------------------------------------------------------------
// Password hashing round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_password_hash_and_verify() {
    let hash = agentforge_auth::password::hash_password("test_password_123").unwrap();
    assert!(agentforge_auth::password::verify_password("test_password_123", &hash).unwrap());
    assert!(!agentforge_auth::password::verify_password("wrong_password", &hash).unwrap());
}

#[test]
fn test_password_hash_is_unique_per_call() {
    // Argon2id uses a random salt, so hashes differ.
    let h1 = agentforge_auth::password::hash_password("same_password").unwrap();
    let h2 = agentforge_auth::password::hash_password("same_password").unwrap();
    assert_ne!(h1, h2);
    // But both verify correctly.
    assert!(agentforge_auth::password::verify_password("same_password", &h1).unwrap());
    assert!(agentforge_auth::password::verify_password("same_password", &h2).unwrap());
}

#[test]
fn test_legacy_bcrypt_hash_verifies_and_needs_upgrade() {
    let hash = bcrypt::hash("legacy_password", 12).unwrap();
    let result = agentforge_auth::password::verify_password_compat("legacy_password", &hash);
    assert!(result.valid);
    assert!(result.needs_upgrade);
}

#[test]
fn test_legacy_sha256_hash_verifies_and_needs_upgrade() {
    use sha2::{Digest, Sha256};

    let hash = hex::encode(Sha256::digest(b"legacy_password"));
    let result = agentforge_auth::password::verify_password_compat("legacy_password", &hash);
    assert!(result.valid);
    assert!(result.needs_upgrade);
}

// ---------------------------------------------------------------------------
// LoginResult serialization
// ---------------------------------------------------------------------------

#[test]
fn test_login_result_serialization() {
    let result = LoginResult {
        user: crate::services::user::AuthenticatedUser {
            id: "user-1".to_string(),
            email: "dev@example.com".to_string(),
            username: "dev".to_string(),
            org_id: Some("org-1".to_string()),
            role: Some("owner".to_string()),
        },
        access_token: "eyJhbGciOiJIUzI1NiJ9.test.sig".to_string(),
        expires_in: 3600,
        refresh_token: "refresh-token".to_string(),
        refresh_expires_in: 604800,
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["user"]["username"], "dev");
    assert_eq!(json["access_token"], "eyJhbGciOiJIUzI1NiJ9.test.sig");
    assert_eq!(json["expires_in"], 3600);
    assert_eq!(json["refresh_token"], "refresh-token");
    assert_eq!(json["refresh_expires_in"], 604800);
}

#[test]
fn test_login_result_has_no_extra_fields() {
    let result = LoginResult {
        user: crate::services::user::AuthenticatedUser {
            id: "user-1".to_string(),
            email: "dev@example.com".to_string(),
            username: "dev".to_string(),
            org_id: Some("org-1".to_string()),
            role: Some("owner".to_string()),
        },
        access_token: "tok".to_string(),
        expires_in: 60,
        refresh_token: "refresh".to_string(),
        refresh_expires_in: 600,
    };
    let json = serde_json::to_value(&result).unwrap();
    let map = json.as_object().unwrap();
    assert_eq!(map.len(), 5);
    assert!(map.contains_key("user"));
    assert!(map.contains_key("access_token"));
    assert!(map.contains_key("expires_in"));
    assert!(map.contains_key("refresh_token"));
    assert!(map.contains_key("refresh_expires_in"));
}
