//! AES-256-GCM encryption compatible with the legacy TS `encryptAesGcm` / `decryptAesGcm`
//! helpers (`server/src/common/crypto/aes-gcm.ts`).
//!
//! On-wire format: `base64(iv[12] || authTag[16] || ciphertext)`. Matches Node's
//! `createCipheriv('aes-256-gcm', key, iv)` output concatenation. Anything
//! encrypted by the legacy TS stack (e.g. `user_cli_credentials.encrypted_credentials`,
//! `user_llm_configs.encrypted_api_key`) must decrypt round-trip here.

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

const IV_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid encryption key length: expected 32 bytes")]
    KeyLength,
    #[error("invalid ciphertext: too short")]
    CiphertextTooShort,
    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("aes-gcm operation failed")]
    Aead,
}

/// Parse a 32-byte AES-256 key from 64 hex chars (matches `LLM_ENCRYPTION_KEY` format).
pub fn decode_key_hex(s: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = hex::decode(s.trim()).map_err(|_| CryptoError::KeyLength)?;
    bytes.try_into().map_err(|_| CryptoError::KeyLength)
}

/// Encrypt UTF-8 plaintext to `base64(iv || tag || ct)` with a fresh random IV.
/// Matches the legacy TS `encryptAesGcm` layout — a blob produced here
/// decrypts round-trip with the TS `decryptAesGcm` helper and vice versa.
pub fn encrypt_base64(key: &[u8; 32], plaintext: &str) -> Result<String, CryptoError> {
    let mut iv_bytes = [0u8; IV_LEN];
    OsRng.fill_bytes(&mut iv_bytes);
    let cipher = Aes256Gcm::new(key.into());
    // aes-gcm returns `ct || tag`; we need to re-order to `iv || tag || ct`
    // so the legacy TS reader decrypts it correctly.
    let ct_with_tag =
        cipher.encrypt(Nonce::from_slice(&iv_bytes), plaintext.as_bytes()).map_err(|_| CryptoError::Aead)?;
    let (ct, tag) = ct_with_tag.split_at(ct_with_tag.len() - TAG_LEN);
    let mut framed = Vec::with_capacity(IV_LEN + TAG_LEN + ct.len());
    framed.extend_from_slice(&iv_bytes);
    framed.extend_from_slice(tag);
    framed.extend_from_slice(ct);
    Ok(BASE64.encode(framed))
}

/// Decrypt base64(iv || tag || ct) into UTF-8 plaintext.
pub fn decrypt_base64(key: &[u8; 32], b64: &str) -> Result<String, CryptoError> {
    let data = BASE64.decode(b64.trim())?;
    if data.len() < IV_LEN + TAG_LEN {
        return Err(CryptoError::CiphertextTooShort);
    }
    let (iv, rest) = data.split_at(IV_LEN);
    let (tag, ct) = rest.split_at(TAG_LEN);

    // aes-gcm crate wants ciphertext || tag; legacy stores tag || ciphertext.
    let mut ct_with_tag = Vec::with_capacity(ct.len() + TAG_LEN);
    ct_with_tag.extend_from_slice(ct);
    ct_with_tag.extend_from_slice(tag);

    let cipher = Aes256Gcm::new(key.into());
    let plaintext = cipher.decrypt(Nonce::from_slice(iv), ct_with_tag.as_slice()).map_err(|_| CryptoError::Aead)?;
    String::from_utf8(plaintext).map_err(|_| CryptoError::Aead)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Manual framing helper — we deliberately don't use `encrypt_base64` to
    // build decrypt-test inputs, so a bug in the production encrypter can't
    // hide behind the production decrypter. Reproduces the legacy TS
    // `iv || tag || ct` layout so a payload written by Node's
    // `createCipheriv('aes-256-gcm', ...)` decrypts here and vice versa.
    fn encrypt_legacy_layout(key: &[u8; 32], iv: &[u8; 12], plaintext: &str) -> String {
        let cipher = Aes256Gcm::new(key.into());
        let out = cipher.encrypt(Nonce::from_slice(iv), plaintext.as_bytes()).unwrap();
        // aes-gcm outputs ct || tag; TS layout is iv || tag || ct.
        let (ct, tag) = out.split_at(out.len() - TAG_LEN);
        let mut framed = Vec::with_capacity(IV_LEN + TAG_LEN + ct.len());
        framed.extend_from_slice(iv);
        framed.extend_from_slice(tag);
        framed.extend_from_slice(ct);
        BASE64.encode(framed)
    }

    #[test]
    fn decode_key_hex_accepts_64_hex_chars() {
        let hex_key = "88996bccb1bc22152853d8ef4175823c7e456172ef1f00c29c719a4b40be0657";
        assert_eq!(decode_key_hex(hex_key).unwrap().len(), 32);
    }

    #[test]
    fn decode_key_hex_rejects_wrong_length() {
        assert!(matches!(decode_key_hex("deadbeef").unwrap_err(), CryptoError::KeyLength));
    }

    #[test]
    fn decrypt_roundtrips_legacy_layout() {
        let key = [7u8; 32];
        let iv = [3u8; 12];
        let b64 = encrypt_legacy_layout(&key, &iv, "hello-world");
        assert_eq!(decrypt_base64(&key, &b64).unwrap(), "hello-world");
    }

    #[test]
    fn decrypt_rejects_short_input() {
        let key = [7u8; 32];
        let short = BASE64.encode([0u8; 10]);
        assert!(matches!(decrypt_base64(&key, &short).unwrap_err(), CryptoError::CiphertextTooShort));
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let plain = "{\"auth.json\":\"hello-world\"}";
        let cipher_b64 = encrypt_base64(&key, plain).unwrap();
        assert_eq!(decrypt_base64(&key, &cipher_b64).unwrap(), plain);
        // Two encrypts produce different blobs (fresh IV each time).
        assert_ne!(encrypt_base64(&key, plain).unwrap(), cipher_b64);
    }

    #[test]
    fn decrypt_rejects_tampered_tag() {
        let key = [7u8; 32];
        let iv = [3u8; 12];
        let b64 = encrypt_legacy_layout(&key, &iv, "payload");
        let mut bytes = BASE64.decode(&b64).unwrap();
        bytes[IV_LEN] ^= 0xFF; // flip a bit in the tag
        let tampered = BASE64.encode(&bytes);
        assert!(matches!(decrypt_base64(&key, &tampered).unwrap_err(), CryptoError::Aead));
    }
}
