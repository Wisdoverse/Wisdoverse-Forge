//! Verifies every committed migration file has a matching MANIFEST.sha256 entry
//! and that the entry matches the file's actual sha256.
//!
//! Run with:
//!
//! ```text
//! cargo test -p agentforge-db --test manifest_integrity_test
//! ```
//!
//! If this test fails, regenerate the manifest:
//!
//! ```text
//! cd rust/crates/db/migrations
//! sha256sum *.sql > MANIFEST.sha256
//! ```

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

#[test]
fn manifest_matches_migration_files() {
    let dir = migrations_dir();
    let manifest_path = dir.join("MANIFEST.sha256");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path.display()));

    // Parse manifest into filename -> expected_hash.
    let mut expected: HashMap<String, String> = HashMap::new();
    for line in manifest.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, "  ");
        let hash = parts.next().expect("hash token").trim().to_string();
        let name = parts.next().expect("filename token").trim().to_string();
        expected.insert(name, hash);
    }

    // Hash every .sql file in the migrations directory.
    let mut actual: HashMap<String, String> = HashMap::new();
    for entry in fs::read_dir(&dir).expect("read migrations dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let mut h = Sha256::new();
        h.update(&bytes);
        actual.insert(name, format!("{:x}", h.finalize()));
    }

    // Every file in the manifest must exist and hash correctly.
    for (name, exp) in &expected {
        match actual.get(name) {
            None => panic!("MANIFEST.sha256 lists '{name}' but the file does not exist in migrations/"),
            Some(act) if act != exp => panic!("SHA-256 mismatch for '{name}':\n  manifest : {exp}\n  on-disk  : {act}"),
            _ => {}
        }
    }

    // Every .sql file must have a manifest entry.
    for name in actual.keys() {
        if !expected.contains_key(name) {
            panic!(
                "'{name}' exists in migrations/ but is not listed in MANIFEST.sha256 — run: cd rust/crates/db/migrations && sha256sum *.sql > MANIFEST.sha256"
            );
        }
    }
}
