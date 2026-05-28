//! Migration manifest verification for embedded `.sql` migration sets.
//!
//! Compares every `.sql` file embedded by `sqlx::migrate!()` against a
//! committed `MANIFEST.sha256` (a `sha256sum`-format list of `<hash>  <file>`
//! lines). This is reused by any crate that embeds migrations — the database
//! crate ([`crate::pool`]) and the orchestrator both call
//! [`verify_manifest`] with their own manifest content and migration set.
//!
//! # What this actually guarantees
//!
//! This check detects two specific failure modes:
//!
//! 1. **A migration file changed without its `MANIFEST.sha256` entry being
//!    updated.** This is caught at PR time by `migration-manifest.yml` (which
//!    recomputes the manifest from the committed `.sql` files and fails the
//!    check on any difference) and again at process startup by
//!    [`verify_manifest`] before any migration runs.
//! 2. **Accidental drift** between the embedded migration set and the manifest
//!    — for example, adding a `.sql` file but forgetting to regenerate the
//!    manifest, or removing a file the manifest still lists.
//!
//! # What this does NOT guarantee
//!
//! Both the migration `.sql` files and `MANIFEST.sha256` are `include_str!`-
//! embedded at compile time, so an attacker who can recompile the binary can
//! edit a migration, regenerate the manifest, rebuild, and the check will pass
//! against the tampered set — the binary self-attests. This check therefore
//! does **not** provide post-build integrity against an adversary with build
//! access. For that the manifest would need to be signed against an external
//! trust root (a separate, currently-untracked piece of work). The real
//! protection here is the PR-time CI diff plus startup staleness detection,
//! not cryptographic supply-chain integrity of a shipped binary.
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
    // An empty manifest would make verification vacuously pass against any
    // migration set (including an empty one), so reject it explicitly. Both
    // real manifests are non-empty, so this changes nothing for valid input.
    if map.is_empty() {
        return Err(ManifestError::MalformedManifest("manifest is empty".into()));
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

    /// SHA-256 hex digest of `content`, matching what `verify_manifest` hashes.
    fn sha256_hex(content: &str) -> String {
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        hex::encode(h.finalize())
    }

    /// One `sha256sum`-format line: `<64-hex>  <name>` (two spaces).
    fn manifest_line(content: &str, name: &str) -> String {
        format!("{}  {}", sha256_hex(content), name)
    }

    #[test]
    fn manifest_parses_without_error() {
        let map = parse_manifest(DB_MANIFEST).expect("parse MANIFEST.sha256");
        assert!(!map.is_empty(), "manifest must not be empty");
    }

    /// A manifest with no entries must not vacuously pass: `parse_manifest`
    /// rejects it so `verify_manifest("", &[])` (and any empty manifest) is an
    /// error rather than `Ok(())`.
    #[test]
    fn rejects_empty_manifest() {
        let err = verify_manifest("", &[]).unwrap_err();
        assert!(
            matches!(err, ManifestError::MalformedManifest(_)),
            "empty manifest must be MalformedManifest, got: {err}"
        );

        // Whitespace-only is also empty after line trimming.
        let err = verify_manifest("\n  \n", &[("a.sql", "x")]).unwrap_err();
        assert!(
            matches!(err, ManifestError::MalformedManifest(_)),
            "whitespace-only manifest must be MalformedManifest, got: {err}"
        );
    }

    /// Self-contained: manifest pins the hash of `"x"` for `a.sql`, but the
    /// source content is tampered — exactly one variant (`HashMismatch`) must
    /// fire, with no ambiguity from missing/unlisted files.
    #[test]
    fn detects_hash_mismatch() {
        let manifest = manifest_line("x", "a.sql");
        let files = [("a.sql", "tampered")];
        let err = verify_manifest(&manifest, &files).unwrap_err();
        assert!(
            matches!(err, ManifestError::HashMismatch { ref file, .. } if file == "a.sql"),
            "expected exactly HashMismatch for a.sql, got: {err}"
        );
    }

    /// Self-contained: the manifest lists only `a.sql` (matching content), but
    /// the source set also contains `b.sql` — exactly `UnlistedFile(b.sql)`
    /// must fire.
    #[test]
    fn detects_unlisted_file() {
        let manifest = manifest_line("x", "a.sql");
        let files = [("a.sql", "x"), ("b.sql", "y")];
        let err = verify_manifest(&manifest, &files).unwrap_err();
        assert!(
            matches!(err, ManifestError::UnlistedFile(ref name) if name == "b.sql"),
            "expected exactly UnlistedFile(b.sql), got: {err}"
        );
    }

    /// Self-contained: the manifest lists `a.sql` + `b.sql` (matching content
    /// for both), but the source set is missing `b.sql` — exactly
    /// `MissingFile(b.sql)` must fire.
    #[test]
    fn detects_missing_file() {
        let manifest = format!("{}\n{}", manifest_line("x", "a.sql"), manifest_line("y", "b.sql"));
        let files = [("a.sql", "x")];
        let err = verify_manifest(&manifest, &files).unwrap_err();
        assert!(
            matches!(err, ManifestError::MissingFile(ref name) if name == "b.sql"),
            "expected exactly MissingFile(b.sql), got: {err}"
        );
    }

    /// A correct manifest over a self-contained source set verifies cleanly.
    #[test]
    fn accepts_matching_manifest() {
        let manifest = format!("{}\n{}", manifest_line("x", "a.sql"), manifest_line("y", "b.sql"));
        let files = [("a.sql", "x"), ("b.sql", "y")];
        verify_manifest(&manifest, &files).expect("matching manifest must verify");
    }
}
