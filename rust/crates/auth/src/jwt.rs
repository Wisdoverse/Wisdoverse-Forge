//! JWT token creation and verification.
//!
//! Uses HS256 (HMAC-SHA256) with the `jsonwebtoken` crate. The API is designed
//! so the algorithm can be swapped to ES256 in Phase 2 without changing callers.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use uuid::Uuid;

use crate::claims::Claims;

/// Manages JWT token creation and verification.
///
/// Holds pre-computed encoding/decoding keys to avoid repeated key derivation.
/// The algorithm is an internal detail — callers use `create_token` / `verify_token`.
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    algorithm: Algorithm,
    expiry_seconds: u64,
}

impl JwtManager {
    /// Create a new `JwtManager` with the given HMAC secret and token lifetime.
    ///
    /// # Arguments
    /// - `secret` — HMAC signing key (should be >= 32 bytes).
    /// - `expiry_seconds` — token validity duration in seconds.
    pub fn new(secret: &str, expiry_seconds: u64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            algorithm: Algorithm::HS256,
            expiry_seconds,
        }
    }

    /// Create a signed JWT token for the given user, org, and role.
    pub fn create_token(&self, user_id: Uuid, org_id: Uuid, role: &str) -> Result<String, jsonwebtoken::errors::Error> {
        self.create_token_with_expiry(user_id, org_id, role, self.expiry_seconds)
    }

    /// Create a signed JWT token carrying active governance axes.
    pub fn create_token_with_axes(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        role: &str,
        workspace_id: Option<Uuid>,
        team_id: Option<Uuid>,
        project_id: Option<Uuid>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.create_token_with_axes_and_expiry(
            user_id,
            org_id,
            role,
            workspace_id,
            team_id,
            project_id,
            self.expiry_seconds,
        )
    }

    /// Create a signed JWT token with a custom expiry.
    pub fn create_token_with_expiry(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        role: &str,
        expiry_seconds: u64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.create_token_with_axes_and_expiry(user_id, org_id, role, None, None, None, expiry_seconds)
    }

    /// Create a signed JWT token with active governance axes and custom expiry.
    pub fn create_token_with_axes_and_expiry(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        role: &str,
        workspace_id: Option<Uuid>,
        team_id: Option<Uuid>,
        project_id: Option<Uuid>,
        expiry_seconds: u64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims::new(user_id, org_id, role, now + expiry_seconds, now).with_scope_axes(
            workspace_id,
            team_id,
            project_id,
        );
        encode(&Header::new(self.algorithm), &claims, &self.encoding_key)
    }

    /// Verify a JWT token and return its claims.
    ///
    /// Returns an error if the token is expired, malformed, or has an invalid signature.
    pub fn verify_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(self.algorithm);
        validation.validate_exp = true;
        let data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(data.claims)
    }

    /// Token expiry duration in seconds.
    pub fn expiry_seconds(&self) -> u64 {
        self.expiry_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key-that-is-at-least-32-chars!!";

    fn make_manager() -> JwtManager {
        JwtManager::new(TEST_SECRET, 3600)
    }

    #[test]
    fn create_and_verify_token() {
        let mgr = make_manager();
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();

        let token = mgr.create_token(user_id, org_id, "admin").unwrap();
        let claims = mgr.verify_token(&token).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.org, org_id);
        assert_eq!(claims.role, "admin");
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.exp - claims.iat, 3600);
    }

    #[test]
    fn expired_token_rejected() {
        // Create a manager with 0-second expiry, then verify immediately.
        // The token will be created with exp == iat, which should be in the past
        // by the time verification runs (or at best equal to now, which jsonwebtoken
        // treats as expired with leeway=0... but default leeway is 60s).
        // Instead, manually craft a token with an already-expired exp.
        let mgr = make_manager();
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();

        // Craft expired claims manually
        let claims = Claims::new(
            user_id,
            org_id,
            "member",
            1_000_000_000, // well in the past (2001-09-09)
            999_999_000,
        );

        let token =
            encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(TEST_SECRET.as_bytes())).unwrap();

        let result = mgr.verify_token(&token);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().kind(), jsonwebtoken::errors::ErrorKind::ExpiredSignature));
    }

    #[test]
    fn tampered_token_rejected() {
        let mgr = make_manager();
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();

        let token = mgr.create_token(user_id, org_id, "member").unwrap();

        // Tamper with the last character of the signature
        let mut tampered = token.clone();
        let last = tampered.pop().unwrap();
        let replacement = if last == 'A' { 'B' } else { 'A' };
        tampered.push(replacement);

        let result = mgr.verify_token(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let mgr = make_manager();
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();

        let token = mgr.create_token(user_id, org_id, "member").unwrap();

        let wrong_mgr = JwtManager::new("a-completely-different-secret-key-here!!", 3600);
        let result = wrong_mgr.verify_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn expiry_seconds_accessor() {
        let mgr = JwtManager::new(TEST_SECRET, 1800);
        assert_eq!(mgr.expiry_seconds(), 1800);
    }

    #[test]
    fn scoped_axes_roundtrip() {
        let mgr = make_manager();
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let team_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();

        let token = mgr
            .create_token_with_axes(user_id, org_id, "member", Some(workspace_id), Some(team_id), Some(project_id))
            .unwrap();
        let claims = mgr.verify_token(&token).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.org, org_id);
        assert_eq!(claims.workspace_id, Some(workspace_id));
        assert_eq!(claims.team_id, Some(team_id));
        assert_eq!(claims.project_id, Some(project_id));
    }
}
