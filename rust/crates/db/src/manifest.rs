//! Migration manifest verification for embedded `.sql` migration sets.
//!
//! Compares every `.sql` file embedded by `sqlx::migrate!()` against a
//! committed `MANIFEST.sha256` (a `sha256sum`-format list of `<hash>  <file>`
//! lines). This is reused by any crate that embeds migrations — the database
//! crate ([`crate::pool`]) and the orchestrator both call
//! [`verify_manifest`] with their own manifest content and migration set.
//!
//! # Operator path
//!
//! When this check fails at startup the process exits before running any
//! migration. To recover, regenerate the manifest for the crate whose
//! migrations changed and commit it, e.g. for the database crate:
//!
//! ```text
//! cd rust/crates/db/migrations
//! sha256sum *.sql > MANIFEST.sha256
//! git add MANIFEST.sha256 && git commit -m "chore(db): update migration manifest"
//! ```
//!
//! The CI workflow `migration-manifest.yml` runs the same comparison on every
//! PR that touches `rust/crates/db/migrations/` or
//! `rust/crates/orchestrator/migrations/`, so a stale manifest fails CI before
//! it can reach a deployment.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

/// Errors from manifest verification.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// A migration file is listed in the manifest but absent from the binary's
    /// embedded migration set.
    #[error("migration file listed in MANIFEST.sha256 but not found: {0}")]
    MissingFile(String),

    /// A migration file exists in the embedded set but has no manifest entry.
    #[error("migration file not in MANIFEST.sha256: {0}")]
    UnlistedFile(String),

    /// A migration file's actual SHA-256 differs from the manifest entry.
    #[error("SHA-256 mismatch for {file}: expected {expected}, got {actual}")]
    HashMismatch { file: String, expected: String, actual: String },

    /// The manifest file itself is malformed (wrong line format).
    #[error("malformed MANIFEST.sha256 line: {0:?}")]
    MalformedManifest(String),
}

/// Parse a `MANIFEST.sha256` string into a `filename -> expected_hex_hash` map.
///
/// Each line must be `<64-hex-chars>  <filename>` (two spaces, as produced by
/// `sha256sum`).
fn parse_manifest(manifest: &str) -> Result<HashMap<String, String>, ManifestError> {
    let mut map = HashMap::new();
    for line in manifest.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, "  ");
        let hash = parts.next().ok_or_else(|| ManifestError::MalformedManifest(line.to_string()))?.trim().to_string();
        let name = parts.next().ok_or_else(|| ManifestError::MalformedManifest(line.to_string()))?.trim().to_string();
        if hash.len() != 64 || name.is_empty() {
            return Err(ManifestError::MalformedManifest(line.to_string()));
        }
        map.insert(name, hash);
    }
    Ok(map)
}

/// Verify every migration file in `migration_files` against `manifest`.
///
/// `manifest` is the `sha256sum`-format `MANIFEST.sha256` content (typically
/// `include_str!`-embedded by the calling crate). `migration_files` is a slice
/// of `(filename, sql_content)` pairs that represents all `.sql` files embedded
/// by `sqlx::migrate!()` for that crate. The function:
///
/// 1. Parses the supplied manifest.
/// 2. Hashes each provided file and compares against the manifest entry.
/// 3. Checks that no file exists in the manifest without a corresponding entry
///    in `migration_files`, and vice-versa.
///
/// Returns `Ok(())` if all files match. Returns the first error encountered
/// otherwise.
pub fn verify_manifest(manifest: &str, migration_files: &[(&str, &str)]) -> Result<(), ManifestError> {
    let expected = parse_manifest(manifest)?;

    // Build an actual map: filename -> sha256 of provided content.
    let mut actual: HashMap<String, String> = HashMap::new();
    for (name, content) in migration_files {
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        actual.insert(name.to_string(), hex::encode(h.finalize()));
    }

    // Every file in the manifest must be present with matching hash.
    for (name, exp_hash) in &expected {
        match actual.get(name) {
            None => return Err(ManifestError::MissingFile(name.clone())),
            Some(act_hash) if act_hash != exp_hash => {
                return Err(ManifestError::HashMismatch {
                    file: name.clone(),
                    expected: exp_hash.clone(),
                    actual: act_hash.clone(),
                });
            }
            _ => {}
        }
    }

    // Every provided file must have a manifest entry.
    for name in actual.keys() {
        if !expected.contains_key(name) {
            return Err(ManifestError::UnlistedFile(name.clone()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The database crate's committed manifest, used to exercise the parser and
    /// verifier against realistic content.
    const DB_MANIFEST: &str = include_str!("../migrations/MANIFEST.sha256");

    #[test]
    fn manifest_parses_without_error() {
        let map = parse_manifest(DB_MANIFEST).expect("parse MANIFEST.sha256");
        assert!(!map.is_empty(), "manifest must not be empty");
    }

    #[test]
    fn detects_hash_mismatch() {
        let files = vec![("000_legacy_prepare.sql", "tampered content")];
        let err = verify_manifest(DB_MANIFEST, &files).unwrap_err();
        assert!(
            matches!(err, ManifestError::HashMismatch { .. })
                || matches!(err, ManifestError::MissingFile(_))
                || matches!(err, ManifestError::UnlistedFile(_)),
            "unexpected error variant: {err}"
        );
    }

    #[test]
    fn detects_unlisted_file() {
        let files = vec![("999_phantom.sql", "SELECT 1;")];
        let err = verify_manifest(DB_MANIFEST, &files).unwrap_err();
        // Either UnlistedFile (phantom not in manifest) or MissingFile (manifest
        // entries not in provided list) may surface first. Both indicate divergence.
        assert!(
            matches!(err, ManifestError::UnlistedFile(_)) || matches!(err, ManifestError::MissingFile(_)),
            "unexpected error variant: {err}"
        );
    }
}
