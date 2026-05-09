use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::ensure;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const ACCESS_TOKEN_TTL_MINUTES: i64 = 15;
const REFRESH_TOKEN_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: String,
    #[serde(default)]
    pub email: String,
    #[serde(rename = "name", default)]
    pub display_name: String,
    #[serde(rename = "org_id", default)]
    pub org_id: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
struct RefreshEntry {
    sub: String,
    email: String,
    display_name: String,
    org_id: String,
    expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("invalid or expired token")]
    InvalidToken,
    #[error("invalid or expired refresh token")]
    InvalidRefreshToken,
    #[error("subject is required for token issuance")]
    InvalidSubject,
    #[error("failed to sign token")]
    Sign(#[source] jsonwebtoken::errors::Error),
}

pub struct SessionManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    refresh_tokens: Mutex<HashMap<String, RefreshEntry>>,
}

impl SessionManager {
    pub fn new(signing_key: Vec<u8>) -> anyhow::Result<Self> {
        ensure!(signing_key.len() >= 32, "JWT signing key must be at least 32 bytes");
        Ok(Self {
            encoding_key: EncodingKey::from_secret(&signing_key),
            decoding_key: DecodingKey::from_secret(&signing_key),
            refresh_tokens: Mutex::new(HashMap::new()),
        })
    }

    pub async fn issue_token_pair(
        &self,
        sub: &str,
        email: &str,
        display_name: &str,
        org_id: &str,
    ) -> Result<TokenPair, SessionError> {
        if sub.trim().is_empty() {
            return Err(SessionError::InvalidSubject);
        }

        let now = Utc::now();
        let expires_at = now + Duration::minutes(ACCESS_TOKEN_TTL_MINUTES);
        let claims = AccessClaims {
            sub: sub.to_string(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            org_id: org_id.to_string(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
        };
        let access_token =
            encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key).map_err(SessionError::Sign)?;
        let refresh_token = generate_refresh_token();
        let refresh_entry = RefreshEntry {
            sub: sub.to_string(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            org_id: org_id.to_string(),
            expires_at: (now + Duration::days(REFRESH_TOKEN_TTL_DAYS)).timestamp(),
        };

        self.refresh_tokens.lock().expect("refresh token lock poisoned").insert(refresh_token.clone(), refresh_entry);

        Ok(TokenPair { access_token, refresh_token, expires_at: expires_at.timestamp() })
    }

    pub fn validate_access_token(&self, token: &str) -> Result<AccessClaims, SessionError> {
        let validation = Validation::new(Algorithm::HS256);
        decode::<AccessClaims>(token, &self.decoding_key, &validation)
            .map(|token| token.claims)
            .map_err(|_| SessionError::InvalidToken)
    }

    pub async fn refresh_tokens(&self, refresh_token: &str) -> Result<TokenPair, SessionError> {
        let entry = {
            let mut refresh_tokens = self.refresh_tokens.lock().expect("refresh token lock poisoned");
            let Some(entry) = refresh_tokens.remove(refresh_token) else {
                return Err(SessionError::InvalidRefreshToken);
            };
            entry
        };

        if entry.expires_at <= Utc::now().timestamp() {
            return Err(SessionError::InvalidRefreshToken);
        }

        self.issue_token_pair(&entry.sub, &entry.email, &entry.display_name, &entry.org_id).await
    }
}

fn generate_refresh_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
