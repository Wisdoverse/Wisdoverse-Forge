//! `agentforge tuf` — TUF-style trusted metadata for offline release bundles.
//!
//! A self-hosted Forge needs to ship software to air-gapped hosts without a
//! phone-home trust hook. The offline bundle (`scripts/offline-bundle.sh`)
//! carries a TUF-style metadata chain that a host verifies against a **pinned
//! root** before loading a single byte:
//!
//! ```text
//! metadata/root.json       (signed by root keys; pins key ids + role thresholds)
//! metadata/targets.json    (signed; sha256 + size of every payload file)
//! metadata/snapshot.json   (signed; hash of targets.json)
//! metadata/timestamp.json  (signed; hash of snapshot.json)
//! ```
//!
//! ## Trust model (root pinning + rotation)
//!
//! - **Pinning:** the first bundle's `metadata/root.json` is copied ONCE to the
//!   host (`/etc/agentforge/tuf/root.json`). Every later verify compares the
//!   bundle root against that pin:
//!   - same version => must be byte-identical;
//!   - the next version => must meet both the pinned and candidate root
//!     thresholds, then becomes the new pin after the full bundle verifies;
//!   - lower version => rejected (rollback protection).
//! - **Key rotation:** `agentforge tuf rotate` signs a NEW root with the old and
//!   new private keys, keeps the old key id in the role list during a grace
//!   period, and re-signs the chain with the new key.
//! - **Verification order:** root -> signatures (threshold) -> timestamp ->
//!   snapshot hash -> targets hash -> on-disk file sha256+size. Any mismatch or
//!   missing target aborts with the failing step named.
//!
//! Keys are Ed25519 PKCS#8 PEM files, the same format `openssl genpkey
//! -algorithm ed25519` produces (and the bundle scripts already use for
//! `SHA256SUMS.sig`); signatures are 64-byte raw-Ed25519 over the canonical
//! JSON encoding of the `signed` object.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use clap::{Args, Subcommand};
use ed25519_dalek::{Signature as EdSignature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use pkcs8::DecodePrivateKey;
use serde::{Deserialize, Serialize};

use crate::error::{CliError, CliResult};

/// Directory (relative to the bundle dir) holding the metadata chain.
pub const METADATA_DIR: &str = "metadata";
const DEFAULT_EXPIRY_DAYS: u64 = 365;
const KEY_TYPE: &str = "ed25519";

#[derive(Args)]
pub struct TufArgs {
    #[command(subcommand)]
    pub command: TufSubcommand,
}

#[derive(Subcommand)]
pub enum TufSubcommand {
    /// Create the metadata chain for a bundle directory (from the signing key).
    Init(InitArgs),
    /// Re-sign the chain from the SHA256SUMS manifest in the bundle directory.
    Sign(SignArgs),
    /// Verify a bundle against a locally pinned root.json.
    Verify(VerifyArgs),
    /// Rotate the root key and re-sign the chain with the new key.
    Rotate(RotateArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Bundle directory (must contain the payload files + SHA256SUMS).
    #[arg(long)]
    pub dir: PathBuf,
    /// Ed25519 PKCS#8 private key PEM (same key as BUNDLE_SIGNING_KEY).
    #[arg(long)]
    pub key: PathBuf,
    /// Metadata validity in days (default 365).
    #[arg(long, default_value_t = DEFAULT_EXPIRY_DAYS)]
    pub expires_days: u64,
}

#[derive(Args)]
pub struct SignArgs {
    #[arg(long)]
    pub dir: PathBuf,
    #[arg(long)]
    pub key: PathBuf,
}

#[derive(Args)]
pub struct VerifyArgs {
    #[arg(long)]
    pub dir: PathBuf,
    /// Locally pinned root.json (copy of the first trusted bundle's root).
    #[arg(long)]
    pub pin: PathBuf,
}

#[derive(Args)]
pub struct RotateArgs {
    #[arg(long)]
    pub dir: PathBuf,
    /// New Ed25519 PKCS#8 private key PEM (becomes the primary root key).
    #[arg(long)]
    pub new_key: PathBuf,
    /// Existing root private key PEM (proves possession of the pinned key).
    #[arg(long)]
    pub old_key: PathBuf,
}

// ── metadata model (canonical JSON, BTreeMap => deterministic order) ────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct KeyVal {
    #[serde(rename = "public")]
    public_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Key {
    keytype: String,
    scheme: String,
    keyval: KeyVal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Role {
    keyids: Vec<String>,
    threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RootMetadata {
    #[serde(rename = "spec_version")]
    spec_version: String,
    version: u64,
    expires: String,
    keys: BTreeMap<String, Key>,
    roles: BTreeMap<String, Role>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TargetEntry {
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TargetsMetadata {
    version: u64,
    expires: String,
    targets: BTreeMap<String, TargetEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SnapshotMetadata {
    version: u64,
    expires: String,
    targets: TargetEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TimestampMetadata {
    version: u64,
    expires: String,
    snapshot: TargetEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Signature {
    keyid: String,
    sig: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SignedMeta {
    signed: serde_json::Value,
    signatures: Vec<Signature>,
}

impl RootMetadata {
    fn new(version: u64, expires: String, key: &SigningKey) -> Self {
        let keyid = keyid_for(&key.verifying_key());
        let new_role = || Role { keyids: vec![keyid.clone()], threshold: 1 };
        let entry = key_entry_for(key);
        Self {
            spec_version: "1.0.0".to_string(),
            version,
            expires,
            keys: BTreeMap::from([(keyid.clone(), entry)]),
            roles: BTreeMap::from([
                ("root".to_string(), new_role()),
                ("targets".to_string(), new_role()),
                ("snapshot".to_string(), new_role()),
                ("timestamp".to_string(), new_role()),
            ]),
        }
    }

    fn key_ids(&self) -> Vec<String> {
        self.roles.get("root").map(|r| r.keyids.clone()).unwrap_or_default()
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn key_entry_for(key: &SigningKey) -> Key {
    use base64::Engine as _;
    Key {
        keytype: KEY_TYPE.to_string(),
        scheme: "ed25519".to_string(),
        keyval: KeyVal { public_key: base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes()) },
    }
}

fn keyid_for(verifying: &VerifyingKey) -> String {
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, b"ed25519:");
    sha2::Digest::update(&mut hasher, verifying.as_bytes());
    hex::encode(sha2::Digest::finalize(hasher))
}

fn load_signing_key(path: &Path) -> CliResult<SigningKey> {
    let pem = fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("cannot read signing key {}: {e}", path.display())))?;
    SigningKey::from_pkcs8_pem(&pem)
        .map_err(|e| CliError::Other(format!("cannot parse Ed25519 PKCS#8 key {}: {e}", path.display())))
}

/// Canonical serialization of the `signed` object: via `serde_json::Value`
/// (sorted keys) so the signer and every verifier hash identical bytes.
fn canonical(signed: &impl Serialize) -> CliResult<Vec<u8>> {
    let value = serde_json::to_value(signed).map_err(|e| CliError::Other(format!("metadata serialization: {e}")))?;
    serde_json::to_vec(&value).map_err(|e| CliError::Other(format!("metadata serialization: {e}")))
}

fn sign_object(signed: &impl Serialize, keys: &[(String, SigningKey)]) -> CliResult<Vec<u8>> {
    let payload = canonical(signed)?;
    let mut signatures = Vec::new();
    for (keyid, key) in keys {
        let signature = key.sign(&payload);
        signatures.push(Signature { keyid: keyid.clone(), sig: hex::encode(signature.to_bytes()) });
    }
    let envelope = SignedMeta { signed: serde_json::to_value(signed).expect("round trip"), signatures };
    serde_json::to_vec(&envelope).map_err(|e| CliError::Other(format!("metadata serialization: {e}")))
}

fn write_metadata(dir: &Path, name: &str, bytes: &[u8]) -> CliResult<()> {
    let meta_dir = dir.join(METADATA_DIR);
    fs::create_dir_all(&meta_dir).map_err(|e| CliError::Other(format!("cannot create {}: {e}", meta_dir.display())))?;
    let path = meta_dir.join(format!("{name}.json"));
    fs::write(&path, bytes).map_err(|e| CliError::Other(format!("cannot write {}: {e}", path.display())))?;
    Ok(())
}

fn read_metadata(dir: &Path, name: &str) -> CliResult<Vec<u8>> {
    let path = dir.join(METADATA_DIR).join(format!("{name}.json"));
    fs::read(&path).map_err(|e| CliError::Other(format!("missing metadata {}: {e}", path.display())))
}

fn read_envelope(dir: &Path, name: &str) -> CliResult<SignedMeta> {
    let bytes = read_metadata(dir, name)?;
    serde_json::from_slice(&bytes).map_err(|e| CliError::Other(format!("cannot parse metadata {name}.json: {e}")))
}

fn expiry(expires_days: u64) -> String {
    (Utc::now() + Duration::days(expires_days as i64)).to_rfc3339()
}

fn check_expiry(expires: &str, name: &str) -> CliResult<()> {
    let expires = DateTime::parse_from_rfc3339(expires)
        .map_err(|e| CliError::Other(format!("invalid expires in {name}: {e}")))?;
    if expires < Utc::now() {
        return Err(CliError::Other(format!(
            "{name} expired at {expires}; refresh the bundle (resign) before loading"
        )));
    }
    Ok(())
}

fn verify_role(signed: &SignedMeta, root: &RootMetadata, role_name: &str) -> CliResult<()> {
    let role =
        root.roles.get(role_name).ok_or_else(|| CliError::Other(format!("root.json has no {role_name} role")))?;
    let authorized: BTreeSet<&str> = role.keyids.iter().map(String::as_str).collect();
    let threshold = role.threshold as usize;
    if threshold == 0 || threshold > authorized.len() {
        return Err(CliError::Other(format!(
            "invalid {role_name} threshold {} for {} unique authorized keys",
            role.threshold,
            authorized.len()
        )));
    }
    for keyid in &authorized {
        let key = root
            .keys
            .get(*keyid)
            .ok_or_else(|| CliError::Other(format!("{role_name} role references missing key {keyid}")))?;
        raw_public_key(key)?;
    }

    let payload = canonical_runtime(&signed.signed)?;
    let mut valid = BTreeSet::new();
    for signature_entry in &signed.signatures {
        if !authorized.contains(signature_entry.keyid.as_str()) || valid.contains(signature_entry.keyid.as_str()) {
            continue;
        }
        let key = &root.keys[&signature_entry.keyid];
        let Ok(raw) = hex::decode(&signature_entry.sig) else {
            continue;
        };
        let Ok(signature) = EdSignature::from_slice(&raw) else {
            continue;
        };
        let public = raw_public_key(key)?;
        if public.verify(&payload, &signature).is_ok() {
            valid.insert(signature_entry.keyid.as_str());
        }
    }
    if valid.len() < threshold {
        return Err(CliError::Other(format!(
            "{role_name} signature verification failed: {} of {threshold} required unique signatures valid",
            valid.len()
        )));
    }
    Ok(())
}

/// Serde JSON of the runtime `signed` Value — same canonical bytes as the signer.
fn canonical_runtime(value: &serde_json::Value) -> CliResult<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| CliError::Other(format!("metadata serialization: {e}")))
}

/// Extract the raw Ed25519 public bytes from a key entry (base64 in `keyval`).
fn raw_public_key(key: &Key) -> CliResult<VerifyingKey> {
    use base64::Engine as _;
    if key.keytype != KEY_TYPE || key.scheme != KEY_TYPE {
        return Err(CliError::Other(format!(
            "unsupported key type/scheme: {}/{} (expected ed25519/ed25519)",
            key.keytype, key.scheme
        )));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&key.keyval.public_key)
        .map_err(|e| CliError::Other(format!("keyval.public base64 decode: {e}")))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CliError::Other("keyval.public must be a 32-byte Ed25519 public key".to_string()))?;
    VerifyingKey::from_bytes(&arr).map_err(|e| CliError::Other(format!("invalid Ed25519 public key: {e}")))
}

/// Parse a sha256sum manifest: `<hexsum>  <path>` per line (paths may contain spaces).
fn parse_manifest(manifest: &str) -> CliResult<Vec<(String, String)>> {
    let mut entries = Vec::new();
    for (index, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let sum = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().trim().to_string();
        if sum.is_empty() || path.is_empty() {
            return Err(CliError::Other(format!("malformed manifest line {}: {line}", index + 1)));
        }
        entries.push((sum, path));
    }
    Ok(entries)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, data);
    hex::encode(sha2::Digest::finalize(hasher))
}

fn file_target(dir: &Path, path: &str) -> CliResult<TargetEntry> {
    let file = dir.join(path);
    let bytes = fs::read(&file).map_err(|e| CliError::Other(format!("cannot read target {path}: {e}")))?;
    Ok(TargetEntry { sha256: sha256_hex(&bytes), size: bytes.len() as u64 })
}

fn load_manifest_targets(dir: &Path) -> CliResult<BTreeMap<String, TargetEntry>> {
    let manifest_path = dir.join("SHA256SUMS");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|e| CliError::Other(format!("cannot read manifest {}: {e}", manifest_path.display())))?;
    let mut targets = BTreeMap::new();
    for (_sum, path) in parse_manifest(&manifest)? {
        targets.insert(path.clone(), file_target(dir, &path)?);
    }
    Ok(targets)
}

fn read_root_file(path: &Path) -> CliResult<(Vec<u8>, SignedMeta, RootMetadata)> {
    let bytes = fs::read(path).map_err(|e| CliError::Other(format!("cannot read {}: {e}", path.display())))?;
    let envelope: SignedMeta =
        serde_json::from_slice(&bytes).map_err(|e| CliError::Other(format!("cannot parse root.json: {e}")))?;
    let root: RootMetadata = serde_json::from_value(envelope.signed.clone())
        .map_err(|e| CliError::Other(format!("root.json is not a TUF root: {e}")))?;
    Ok((bytes, envelope, root))
}

fn metadata_dir(dir: &Path) -> CliResult<PathBuf> {
    let path = dir.join(METADATA_DIR);
    if !path.is_dir() {
        return Err(CliError::Other(format!(
            "no metadata directory {} — run `agentforge tuf init` first",
            path.display()
        )));
    }
    Ok(path)
}

fn entry_for(bytes: &[u8]) -> TargetEntry {
    TargetEntry { sha256: sha256_hex(bytes), size: bytes.len() as u64 }
}

fn versions_from(dir: &Path) -> CliResult<(u64, u64, u64)> {
    let read_version = |name: &str| -> CliResult<u64> {
        read_envelope(dir, name)?
            .signed
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| CliError::Other(format!("{name}.json has no valid version")))
    };
    Ok((read_version("targets")?, read_version("snapshot")?, read_version("timestamp")?))
}

// ── commands ────────────────────────────────────────────────────────────────

pub fn run(opts: TufArgs, _stdout: &mut dyn Write) -> CliResult<()> {
    match opts.command {
        TufSubcommand::Init(args) => init(args),
        TufSubcommand::Sign(args) => sign(args),
        TufSubcommand::Verify(args) => verify(args),
        TufSubcommand::Rotate(args) => rotate(args),
    }
}

fn init(args: InitArgs) -> CliResult<()> {
    let key = load_signing_key(&args.key)?;
    let expires = expiry(args.expires_days);
    let root = RootMetadata::new(1, expires.clone(), &key);
    let targets = load_manifest_targets(&args.dir)?;
    let keyid = keyid_for(&key.verifying_key());
    write_chain(&args.dir, &root, &[(keyid.clone(), key)], &targets, &expires, (0, 0, 0))?;
    println!(
        "TUF metadata initialized (root v1, {} targets).\nPin this root ONCE on every host before loading bundles:\n  cp {}/metadata/root.json /etc/agentforge/tuf/root.json\nRoot key id: {keyid}",
        targets.len(),
        args.dir.display()
    );
    Ok(())
}

fn sign(args: SignArgs) -> CliResult<()> {
    let key = load_signing_key(&args.key)?;
    let (_, root_env, root) = read_root_file(&args.dir.join(METADATA_DIR).join("root.json")).map_err(|_| {
        CliError::Other("no existing metadata — run `agentforge tuf init --dir <dir> --key <key>` first".to_string())
    })?;
    verify_role(&root_env, &root, "root")?;
    let signer_keyid = keyid_for(&key.verifying_key());
    if !root.key_ids().contains(&signer_keyid) {
        return Err(CliError::Other(format!(
            "signing key {signer_keyid} is not a trusted root key; rotation requires `tuf rotate`"
        )));
    }
    let versions = versions_from(&args.dir)?;
    let targets = load_manifest_targets(&args.dir)?;
    let expires = expiry(DEFAULT_EXPIRY_DAYS);
    write_child_chain(&args.dir, &root, &[(signer_keyid, key)], &targets, &expires, versions)?;
    println!("TUF metadata refreshed: root v{}, {} targets, expiry {expires}", root.version, targets.len());
    Ok(())
}

fn verify(args: VerifyArgs) -> CliResult<()> {
    let meta_dir = metadata_dir(&args.dir)?;
    let (pin_bytes, pin_env, pin) = read_root_file(&args.pin)?;
    verify_role(&pin_env, &pin, "root")?;

    // 1. Root metadata vs PIN (pinning + rotation + rollback).
    let root_path = meta_dir.join("root.json");
    let root_bytes =
        fs::read(&root_path).map_err(|e| CliError::Other(format!("cannot read {}: {e}", root_path.display())))?;
    let root_env: SignedMeta = serde_json::from_slice(&root_bytes)
        .map_err(|e| CliError::Other(format!("cannot parse {}: {e}", root_path.display())))?;
    let root: RootMetadata = serde_json::from_value(root_env.signed.clone())
        .map_err(|e| CliError::Other(format!("cannot parse {}: {e}", root_path.display())))?;
    check_expiry(&root.expires, "root.json")?;

    if root.version < pin.version {
        return Err(CliError::Other(format!(
            "root rollback rejected: bundle root v{} < pinned root v{}",
            root.version, pin.version
        )));
    }
    if root.version == pin.version {
        if root_bytes != pin_bytes {
            return Err(CliError::Other(
                "root mismatch: bundle root v{version} differs from the pinned root".to_string(),
            ));
        }
    } else {
        if root.version != pin.version + 1 {
            return Err(CliError::Other(format!(
                "root rotation rejected: bundle root v{} must advance exactly one version from pinned root v{}",
                root.version, pin.version
            )));
        }
        verify_role(&root_env, &pin, "root")?;
    }

    // 2. The candidate root must authorize itself; each child uses its own role.
    verify_role(&root_env, &root, "root")?;
    for name in ["targets", "snapshot", "timestamp"] {
        let envelope = read_envelope(&args.dir, name)?;
        let expires = envelope.signed.get("expires").and_then(|v| v.as_str()).unwrap_or_default();
        check_expiry(expires, &format!("{name}.json"))?;
        verify_role(&envelope, &root, name)?;
    }

    // 3. Hash chain: timestamp -> snapshot -> targets.
    let timestamp = read_envelope(&args.dir, "timestamp")?;
    let snapshot_bytes = read_metadata(&args.dir, "snapshot")?;
    let snapshot = read_envelope(&args.dir, "snapshot")?;
    let targets_bytes = read_metadata(&args.dir, "targets")?;

    let snapshot_entry: TargetEntry = serde_json::from_value(
        timestamp
            .signed
            .get("snapshot")
            .cloned()
            .ok_or_else(|| CliError::Other("timestamp.json missing snapshot hash".into()))?,
    )
    .map_err(|e| CliError::Other(format!("parse timestamp snapshot: {e}")))?;
    let actual = entry_for(&snapshot_bytes);
    if actual != snapshot_entry {
        return Err(CliError::Other(format!(
            "timestamp -> snapshot hash mismatch: expected sha256={} size={}, got sha256={} size={}",
            snapshot_entry.sha256, snapshot_entry.size, actual.sha256, actual.size
        )));
    }

    let targets_entry: TargetEntry = serde_json::from_value(
        snapshot
            .signed
            .get("targets")
            .cloned()
            .ok_or_else(|| CliError::Other("snapshot.json missing targets hash".into()))?,
    )
    .map_err(|e| CliError::Other(format!("parse snapshot targets: {e}")))?;
    let actual = entry_for(&targets_bytes);
    if actual != targets_entry {
        return Err(CliError::Other(format!(
            "snapshot -> targets hash mismatch: expected sha256={} size={}, got sha256={} size={}",
            targets_entry.sha256, targets_entry.size, actual.sha256, actual.size
        )));
    }

    let targets_meta: TargetsMetadata = serde_json::from_value(read_envelope(&args.dir, "targets")?.signed)
        .map_err(|e| CliError::Other(format!("parse targets.json: {e}")))?;
    if targets_meta.targets.is_empty() {
        return Err(CliError::Other("targets.json has no target entries".into()));
    }

    // 4. Every payload file on disk matches its signed hash + size.
    for (name, expected) in &targets_meta.targets {
        let actual = file_target(&args.dir, name)?;
        if *expected != actual {
            return Err(CliError::Other(format!(
                "target hash mismatch for {name}: expected sha256={} size={}, got sha256={} size={}",
                expected.sha256, expected.size, actual.sha256, actual.size
            )));
        }
    }

    if root.version > pin.version {
        persist_pin(&args.pin, &root_bytes)?;
    }

    println!(
        "TUF chain verified: root v{} (pinned), {} targets, signatures + hash chain OK.",
        root.version,
        targets_meta.targets.len()
    );
    Ok(())
}

fn rotate(args: RotateArgs) -> CliResult<()> {
    let old_key = load_signing_key(&args.old_key)?;
    let new_key = load_signing_key(&args.new_key)?;
    let (_, root_env, root) = read_root_file(&args.dir.join(METADATA_DIR).join("root.json"))?;
    verify_role(&root_env, &root, "root")?;
    let old_keyid = keyid_for(&old_key.verifying_key());
    if !root.key_ids().contains(&old_keyid) {
        return Err(CliError::Other(format!("old key {old_keyid} is not a trusted root key; rotation refused")));
    }
    let new_keyid = keyid_for(&new_key.verifying_key());
    let expires = expiry(DEFAULT_EXPIRY_DAYS);

    // New root v+1: both keys trusted (grace), threshold stays 1, signed by BOTH.
    let mut root_meta = RootMetadata::new(root.version + 1, expires.clone(), &new_key);
    for role in root_meta.roles.values_mut() {
        role.keyids = vec![new_keyid.clone(), old_keyid.clone()];
    }
    root_meta.keys.insert(old_keyid.clone(), key_entry_for(&old_key));

    let versions = versions_from(&args.dir)?;
    let targets_env = read_envelope(&args.dir, "targets")?;
    verify_role(&targets_env, &root, "targets")?;
    let targets: TargetsMetadata =
        serde_json::from_value(targets_env.signed).map_err(|e| CliError::Other(format!("parse targets.json: {e}")))?;
    let chain_keys = vec![(new_keyid.clone(), new_key), (old_keyid.clone(), old_key)];
    write_chain(&args.dir, &root_meta, &chain_keys, &targets.targets, &expires, versions)?;
    println!(
        "Root rotated to v{} (keys: {} + {}). Hosts advance their pin after the complete rotated bundle verifies.",
        root_meta.version, old_keyid, new_keyid
    );
    Ok(())
}

/// Write all four metadata files; child versions bump monotonically from any
/// prior chain so a resign is never a downgrade.
fn write_chain(
    dir: &Path,
    root: &RootMetadata,
    root_keys: &[(String, SigningKey)],
    targets: &BTreeMap<String, TargetEntry>,
    expires: &str,
    versions: (u64, u64, u64),
) -> CliResult<()> {
    let root_bytes = sign_object(root, root_keys)?;
    verify_role(&serde_json::from_slice(&root_bytes).expect("signed root round trip"), root, "root")?;
    write_metadata(dir, "root", &root_bytes)?;
    write_child_chain(dir, root, root_keys, targets, expires, versions)
}

fn write_child_chain(
    dir: &Path,
    root: &RootMetadata,
    signing_keys: &[(String, SigningKey)],
    targets: &BTreeMap<String, TargetEntry>,
    expires: &str,
    (targets_version, snapshot_version, timestamp_version): (u64, u64, u64),
) -> CliResult<()> {
    let targets_meta =
        TargetsMetadata { version: targets_version + 1, expires: expires.to_string(), targets: targets.clone() };
    let targets_bytes = sign_object(&targets_meta, signing_keys)?;
    verify_role(&serde_json::from_slice(&targets_bytes).expect("signed targets round trip"), root, "targets")?;

    let snapshot_meta = SnapshotMetadata {
        version: snapshot_version + 1,
        expires: expires.to_string(),
        targets: entry_for(&targets_bytes),
    };
    let snapshot_bytes = sign_object(&snapshot_meta, signing_keys)?;
    verify_role(&serde_json::from_slice(&snapshot_bytes).expect("signed snapshot round trip"), root, "snapshot")?;

    let timestamp_meta = TimestampMetadata {
        version: timestamp_version + 1,
        expires: expires.to_string(),
        snapshot: entry_for(&snapshot_bytes),
    };
    let timestamp_bytes = sign_object(&timestamp_meta, signing_keys)?;
    verify_role(&serde_json::from_slice(&timestamp_bytes).expect("signed timestamp round trip"), root, "timestamp")?;

    write_metadata(dir, "targets", &targets_bytes)?;
    write_metadata(dir, "snapshot", &snapshot_bytes)?;
    write_metadata(dir, "timestamp", &timestamp_bytes)?;
    Ok(())
}

fn persist_pin(path: &Path, bytes: &[u8]) -> CliResult<()> {
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&temporary, bytes)
        .map_err(|e| CliError::Other(format!("cannot write updated root pin {}: {e}", temporary.display())))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(CliError::Other(format!("cannot replace root pin {}: {error}", path.display())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pkcs8::EncodePrivateKey;

    fn test_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn write_file(dir: &Path, name: &str, content: &[u8]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    fn setup_bundle(dir: &Path) {
        write_file(dir, "images.tar", b"image-bytes-123");
        write_file(dir, "images.txt", b"agentforge-server:0.1.0\n");
        write_file(dir, "README.txt", b"bundle readme");
        let manifest = ["images.tar", "images.txt", "README.txt"]
            .iter()
            .map(|name| format!("{}  {name}", sha256_hex(&fs::read(dir.join(name)).unwrap())))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(dir.join("SHA256SUMS"), manifest).unwrap();
    }

    fn key_pem_path(dir: &Path, key: &SigningKey) -> PathBuf {
        let pem = key.to_pkcs8_pem(pkcs8::LineEnding::LF).expect("pkcs8 pem");
        let path = dir.join("signing-key.pem");
        fs::write(&path, pem.as_bytes()).unwrap();
        path
    }

    #[test]
    fn init_sign_verify_round_trip_accepts_pinned_root() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let key = test_key(7);
        init(InitArgs { dir: dir.to_path_buf(), key: key_pem_path(dir, &key), expires_days: 365 }).unwrap();
        let pin = dir.join(METADATA_DIR).join("root.json");
        assert!(pin.exists());
        verify(VerifyArgs { dir: dir.to_path_buf(), pin: pin.clone() }).unwrap();

        // Tamper a payload file: verification must fail naming the target.
        fs::write(dir.join("images.txt"), b"different content").unwrap();
        let err = verify(VerifyArgs { dir: dir.to_path_buf(), pin }).unwrap_err();
        assert!(err.to_string().contains("hash mismatch"), "got: {err}");
    }

    #[test]
    fn verify_rejects_tampered_timestamp_hash_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let key = test_key(9);
        init(InitArgs { dir: dir.to_path_buf(), key: key_pem_path(dir, &key), expires_days: 365 }).unwrap();

        let mut ts: SignedMeta =
            serde_json::from_slice(&fs::read(dir.join(METADATA_DIR).join("timestamp.json")).unwrap()).unwrap();
        ts.signed["snapshot"]["sha256"] = serde_json::json!("deadbeef");
        fs::write(dir.join(METADATA_DIR).join("timestamp.json"), serde_json::to_vec(&ts).unwrap()).unwrap();

        let err =
            verify(VerifyArgs { dir: dir.to_path_buf(), pin: dir.join(METADATA_DIR).join("root.json") }).unwrap_err();
        // Tampering invalidates the timestamp signature first; the hash-chain
        // check would be the next failure if the attacker re-signed too.
        assert!(err.to_string().contains("signature verification failed"), "got: {err}");
    }

    #[test]
    fn verify_rejects_root_signed_by_an_unpinned_key() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let key_a = test_key(11);
        let key_b = SigningKey::from_bytes(&[12; 32]);
        init(InitArgs { dir: dir.to_path_buf(), key: key_pem_path(dir, &key_a), expires_days: 365 }).unwrap();

        // Save the FIRST root as the host pin (one-time TOFU), then replace the
        // bundle root with a v2 forged by an unrelated key.
        let pin = dir.join("host-pin-root.json");
        fs::copy(dir.join(METADATA_DIR).join("root.json"), &pin).unwrap();

        let forged = RootMetadata::new(2, expiry(30), &key_b);
        let bytes = sign_object(&forged, &[(keyid_for(&key_b.verifying_key()), key_b)]).unwrap();
        fs::write(dir.join(METADATA_DIR).join("root.json"), bytes).unwrap();

        let err = verify(VerifyArgs { dir: dir.to_path_buf(), pin }).unwrap_err();
        assert!(err.to_string().contains("root signature verification failed"), "got: {err}");
    }

    #[test]
    fn verify_rejects_forged_pinned_key_label() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let trusted = test_key(13);
        let attacker = test_key(14);
        init(InitArgs { dir: dir.to_path_buf(), key: key_pem_path(dir, &trusted), expires_days: 365 }).unwrap();
        let pin = dir.join("host-pin-root.json");
        fs::copy(dir.join(METADATA_DIR).join("root.json"), &pin).unwrap();

        let trusted_id = keyid_for(&trusted.verifying_key());
        let mut forged = RootMetadata::new(2, expiry(30), &attacker);
        forged.keys = BTreeMap::from([(trusted_id.clone(), key_entry_for(&attacker))]);
        for role in forged.roles.values_mut() {
            role.keyids = vec![trusted_id.clone()];
        }
        let bytes = sign_object(&forged, &[(trusted_id, attacker)]).unwrap();
        fs::write(dir.join(METADATA_DIR).join("root.json"), bytes).unwrap();

        let err = verify(VerifyArgs { dir: dir.to_path_buf(), pin }).unwrap_err();
        assert!(err.to_string().contains("root signature verification failed"), "got: {err}");
    }

    #[test]
    fn role_threshold_counts_unique_authorized_keys() {
        let trusted = test_key(15);
        let second = test_key(16);
        let trusted_id = keyid_for(&trusted.verifying_key());
        let second_id = keyid_for(&second.verifying_key());
        let mut root = RootMetadata::new(1, expiry(30), &trusted);
        root.keys.insert(second_id.clone(), key_entry_for(&second));
        root.roles.get_mut("root").unwrap().keyids.push(second_id);
        root.roles.get_mut("root").unwrap().threshold = 2;
        let bytes = sign_object(&root, &[(trusted_id, trusted)]).unwrap();
        let mut envelope: SignedMeta = serde_json::from_slice(&bytes).unwrap();
        envelope.signatures.push(envelope.signatures[0].clone());

        let err = verify_role(&envelope, &root, "root").unwrap_err();
        assert!(err.to_string().contains("1 of 2 required unique signatures"), "got: {err}");
    }

    #[test]
    fn sign_preserves_root_bytes_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let key = test_key(17);
        let key_path = key_pem_path(dir, &key);
        init(InitArgs { dir: dir.to_path_buf(), key: key_path.clone(), expires_days: 365 }).unwrap();
        let before = read_metadata(dir, "root").unwrap();

        sign(SignArgs { dir: dir.to_path_buf(), key: key_path }).unwrap();

        assert_eq!(read_metadata(dir, "root").unwrap(), before);
    }

    #[test]
    fn rotate_accepts_new_root_signed_by_pinned_old_key() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        setup_bundle(dir);
        let key_a = test_key(21);
        let key_b = test_key(22);
        init(InitArgs { dir: dir.to_path_buf(), key: key_pem_path(dir, &key_a), expires_days: 365 }).unwrap();
        let pin = dir.join("host-pin-root.json");
        fs::copy(dir.join(METADATA_DIR).join("root.json"), &pin).unwrap();

        rotate(RotateArgs {
            dir: dir.to_path_buf(),
            new_key: key_pem_path(dir, &key_b),
            old_key: key_pem_path(dir, &key_a),
        })
        .unwrap();

        // Old pin accepts v2 only after both root thresholds verify, then the
        // verifier persists v2 so the next load uses the new trust root.
        verify(VerifyArgs { dir: dir.to_path_buf(), pin: pin.clone() }).unwrap();
        assert_eq!(fs::read(&pin).unwrap(), read_metadata(dir, "root").unwrap());
        verify(VerifyArgs { dir: dir.to_path_buf(), pin }).unwrap();
    }
}
