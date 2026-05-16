use crate::domain::credential::{ApiKeyFormat, ApiKeyName, ApiKeyScopePolicy};
use crate::services::api_key::{generate_api_key, generate_api_key_parts, hash_key};

// ---------------------------------------------------------------------------
// Key generation
// ---------------------------------------------------------------------------

#[test]
fn test_key_generation_format() {
    let key = generate_api_key();
    assert!(key.starts_with("af_"));
    assert_eq!(key.len(), 67); // "af_" (3) + 64 hex chars
    // Verify hex portion is valid
    assert!(hex::decode(&key[3..]).is_ok());
}

#[test]
fn test_key_generation_uniqueness() {
    let key1 = generate_api_key();
    let key2 = generate_api_key();
    assert_ne!(key1, key2);
}

#[test]
fn test_generate_api_key_parts() {
    let (key, hash, prefix) = generate_api_key_parts();
    assert!(key.starts_with("af_"));
    assert_eq!(key.len(), 67);
    assert_eq!(prefix.len(), 8);
    assert_eq!(&key[3..11], prefix); // prefix matches first 8 chars after "af_"
    // Hash should be a valid SHA-256 hex string
    assert_eq!(hash.len(), 64);
    assert!(hex::decode(&hash).is_ok());
}

// ---------------------------------------------------------------------------
// Hash consistency
// ---------------------------------------------------------------------------

#[test]
fn test_key_hash_consistency() {
    let key = "af_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let hash1 = hash_key(key);
    let hash2 = hash_key(key);
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA-256 hex digest
}

#[test]
fn test_key_hash_from_generated() {
    let (key, hash, _) = generate_api_key_parts();
    // Re-hashing the same key should produce the same hash
    let rehash = hash_key(&key);
    assert_eq!(hash, rehash);
}

#[test]
fn test_different_keys_different_hashes() {
    let key1 = generate_api_key();
    let key2 = generate_api_key();
    assert_ne!(hash_key(&key1), hash_key(&key2));
}

// ---------------------------------------------------------------------------
// Key format validation
// ---------------------------------------------------------------------------

#[test]
fn test_validate_key_format_valid() {
    let key = format!("af_{}", "a".repeat(64));
    assert!(ApiKeyFormat::validate(&key).is_ok());
}

#[test]
fn test_validate_key_format_generated() {
    let key = generate_api_key();
    assert!(ApiKeyFormat::validate(&key).is_ok());
}

#[test]
fn test_validate_key_format_bad_prefix() {
    let key = format!("bad_{}", "a".repeat(64));
    assert!(ApiKeyFormat::validate(&key).is_err());
}

#[test]
fn test_validate_key_format_too_short() {
    assert!(ApiKeyFormat::validate("af_short").is_err());
}

#[test]
fn test_validate_key_format_empty() {
    assert!(ApiKeyFormat::validate("").is_err());
}

#[test]
fn test_validate_key_format_too_long() {
    let key = format!("af_{}", "a".repeat(65));
    assert!(ApiKeyFormat::validate(&key).is_err());
}

#[test]
fn test_validate_key_format_invalid_hex() {
    // 'g' is not a valid hex character
    let key = format!("af_{}", "g".repeat(64));
    assert!(ApiKeyFormat::validate(&key).is_err());
}

// ---------------------------------------------------------------------------
// Scope validation
// ---------------------------------------------------------------------------

#[test]
fn test_validate_scopes_valid() {
    assert!(ApiKeyScopePolicy::validate(&["read".into(), "write".into()]).is_ok());
    assert!(ApiKeyScopePolicy::validate(&["admin".into()]).is_ok());
    assert!(ApiKeyScopePolicy::validate(&["read".into(), "write".into(), "admin".into()]).is_ok());
}

#[test]
fn test_validate_scopes_empty_is_ok() {
    assert!(ApiKeyScopePolicy::validate(&[]).is_ok());
}

#[test]
fn test_validate_scopes_invalid() {
    assert!(ApiKeyScopePolicy::validate(&["invalid_scope".into()]).is_err());
    assert!(ApiKeyScopePolicy::validate(&["delete".into()]).is_err());
    assert!(ApiKeyScopePolicy::validate(&["READ".into()]).is_err()); // case-sensitive
}

#[test]
fn test_validate_scopes_mixed_valid_invalid() {
    assert!(ApiKeyScopePolicy::validate(&["read".into(), "invalid".into()]).is_err());
}

// ---------------------------------------------------------------------------
// Key name validation
// ---------------------------------------------------------------------------

#[test]
fn test_validate_key_name_valid() {
    assert!(ApiKeyName::parse("My API Key").is_ok());
    assert!(ApiKeyName::parse("a").is_ok());
    assert!(ApiKeyName::parse(&"x".repeat(255)).is_ok());
}

#[test]
fn test_validate_key_name_empty() {
    assert!(ApiKeyName::parse("").is_err());
}

#[test]
fn test_validate_key_name_whitespace_only() {
    assert!(ApiKeyName::parse("   ").is_err()); // trims to empty
}

#[test]
fn test_validate_key_name_too_long() {
    assert!(ApiKeyName::parse(&"x".repeat(256)).is_err());
}

#[test]
fn test_validate_key_name_trims_whitespace() {
    assert!(ApiKeyName::parse("  valid  ").is_ok()); // trimmed length is 5
}
