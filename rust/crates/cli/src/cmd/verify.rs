//! `agentforge verify` — verify a downloaded sidecar/CLI binary against its
//! Sigstore cosign bundle published with each GitHub release.
//!
//! ## Prerequisite
//!
//! The `cosign` binary must be on PATH. One-time install:
//!   <https://docs.sigstore.dev/cosign/installation/>
//!
//! ## Usage
//!
//! ```text
//! agentforge verify --tag v1.2.3 ./agentforge-sidecar-linux-amd64
//! ```
//!
//! The bundle file defaults to `<artifact>.sig.bundle` in the same directory.
//! Download it from the same GitHub release before running verify.
//!
//! ## What is checked
//!
//! cosign verify-blob confirms that:
//!   1. The bundle was signed by a GitHub Actions OIDC token.
//!   2. The token was issued for the `release-supply-chain.yml` workflow in
//!      the official Wisdoverse Forge repository at the specified release tag.
//!   3. The artifact bytes match the digest recorded in the bundle.
//!
//! A bundle signed by a fork, a different workflow, or a different tag will
//! fail, even if the bytes are otherwise identical.

use crate::error::{CliError, CliResult};
use clap::Args;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

const OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
const DEFAULT_REPO: &str = "Wisdoverse/Wisdoverse-Forge";

/// Verify a release artifact against its Sigstore cosign bundle.
///
/// Requires the `cosign` binary on PATH.
/// Download the artifact and its `.sig.bundle` from the same GitHub release
/// before running this command.
#[derive(Args)]
pub struct VerifyArgs {
    /// Path to the artifact to verify (e.g. `./agentforge-sidecar-linux-amd64`).
    pub artifact: PathBuf,

    /// Path to the `.sig.bundle` Sigstore bundle.
    /// Defaults to `<artifact>.sig.bundle` in the same directory.
    #[arg(long)]
    pub bundle: Option<PathBuf>,

    /// Release tag the artifact was published with (e.g. `v1.2.3`).
    #[arg(long)]
    pub tag: String,

    /// GitHub repository that published the release.
    /// Override only if you host a private mirror of the platform.
    #[arg(long, default_value = DEFAULT_REPO)]
    pub repo: String,
}

pub fn run(opts: VerifyArgs, stdout: &mut dyn Write) -> CliResult<()> {
    let bundle = opts.bundle.unwrap_or_else(|| {
        let mut p = opts.artifact.clone();
        // Append `.sig.bundle` suffix to the existing filename.
        let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".sig.bundle");
        p.set_file_name(name);
        p
    });

    // Validate paths exist before shelling out so errors are clear.
    if !opts.artifact.exists() {
        return Err(CliError::Other(format!("artifact not found: {}", opts.artifact.display())));
    }
    if !bundle.exists() {
        return Err(CliError::Other(format!(
            "bundle not found: {}\n\
             Download it from the GitHub release page alongside the artifact, \
             or pass --bundle <path>.",
            bundle.display()
        )));
    }

    let identity =
        format!("https://github.com/{}/.github/workflows/release-supply-chain.yml@refs/tags/{}", opts.repo, opts.tag);

    let artifact_str =
        opts.artifact.to_str().ok_or_else(|| CliError::Other("artifact path contains invalid UTF-8".into()))?;
    let bundle_str = bundle.to_str().ok_or_else(|| CliError::Other("bundle path contains invalid UTF-8".into()))?;

    let status = Command::new("cosign")
        .args(["verify-blob"])
        .args(["--bundle", bundle_str])
        .args(["--certificate-identity", &identity])
        .args(["--certificate-oidc-issuer", OIDC_ISSUER])
        .arg(artifact_str)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CliError::Other(
                    "cosign not found on PATH.\n\
                     Install: https://docs.sigstore.dev/cosign/installation/"
                        .into(),
                )
            } else {
                CliError::Other(format!("failed to invoke cosign: {e}"))
            }
        })?;

    if !status.success() {
        return Err(CliError::Other(format!(
            "cosign verify-blob failed — artifact is NOT verified\n\
             Re-download the artifact and bundle from the official release \
             at https://github.com/{}/releases/tag/{} and retry.",
            opts.repo, opts.tag
        )));
    }

    let _ = writeln!(stdout, "verified: {} (tag {}, repo {})", opts.artifact.display(), opts.tag, opts.repo,);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_path_defaults_to_artifact_plus_sig_bundle_suffix() {
        let args = VerifyArgs {
            artifact: PathBuf::from("/tmp/agentforge-sidecar-linux-amd64"),
            bundle: None,
            tag: "v1.2.3".into(),
            repo: DEFAULT_REPO.into(),
        };
        // Replicate the default-bundle logic without the full run().
        let bundle = args.bundle.clone().unwrap_or_else(|| {
            let mut p = args.artifact.clone();
            let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            name.push(".sig.bundle");
            p.set_file_name(name);
            p
        });
        assert_eq!(bundle, PathBuf::from("/tmp/agentforge-sidecar-linux-amd64.sig.bundle"));
    }

    #[test]
    fn run_returns_error_when_artifact_missing() {
        let args = VerifyArgs {
            artifact: PathBuf::from("/nonexistent/artifact"),
            bundle: None,
            tag: "v1.2.3".into(),
            repo: DEFAULT_REPO.into(),
        };
        let mut out = Vec::new();
        let result = run(args, &mut out);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("artifact not found"), "unexpected msg: {msg}");
    }
}
