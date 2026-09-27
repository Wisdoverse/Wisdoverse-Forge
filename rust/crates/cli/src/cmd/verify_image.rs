//! `agentforge verify-image` — verify a published container image against its
//! Sigstore cosign signature produced by the official image workflows.
//!
//! ## Why this exists
//!
//! The ghcr.io container images are the PRIMARY shipped artifacts for the
//! container runtime (sidecar, server, orchestrator, agent images). They are
//! signed with Sigstore keyless cosign BY DIGEST. Mainline images are signed in
//! `publish-images.yml`; versioned frontend, server, sidecar, orchestrator, and
//! public CLI images built for a release are signed in `release.yml`. Public CLI
//! overlays may also be signed by the CLI-only `watch-cli-versions.yml` rebuild
//! workflow.
//! This command lets an operator confirm an image was built and signed by the
//! official image workflows before running it.
//!
//! ## Prerequisite
//!
//! The `cosign` binary must be on PATH. One-time install:
//!   <https://docs.sigstore.dev/cosign/installation/>
//!
//! ## Usage
//!
//! ```text
//! # By digest (recommended — immutable):
//! agentforge verify-image ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:<digest>
//!
//! # By tag (cosign resolves the tag to a digest, then verifies):
//! agentforge verify-image ghcr.io/wisdoverse/wisdoverse-forge/sidecar:main
//! ```
//!
//! ## What is checked (fails closed)
//!
//! `cosign verify` confirms that:
//!   1. The image is signed by a GitHub Actions OIDC token.
//!   2. The token was issued for an allowed workflow in the official
//!      repository. `watch-cli-versions.yml` is allowed only for the three
//!      public CLI overlay image names.
//!   3. The token issuer is `https://token.actions.githubusercontent.com`.
//!
//! An image signed by a fork, a different workflow, or a different repository
//! FAILS — we never trust "any Sigstore signature". The default identity
//! regexp accepts only an allowed workflow running from `refs/heads/main`;
//! pass `--ref` to deliberately verify another exact ref.

use crate::error::{CliError, CliResult};
#[cfg(test)]
use agentforge_core::image_trust::cosign_identity_regexp as identity_regexp;
use agentforge_core::image_trust::{
    DEFAULT_IMAGE_REPOSITORY as DEFAULT_REPO, cosign_verify_args, expected_signer_description,
};
use clap::Args;
use std::io::Write;
use std::process::Command;

/// Verify a published container image against its Sigstore cosign signature.
///
/// Requires the `cosign` binary on PATH.
#[derive(Args)]
pub struct VerifyImageArgs {
    /// Image reference to verify. Prefer an immutable digest reference, e.g.
    /// `ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:<digest>`.
    /// A tag (`...:main`) is also accepted; cosign resolves it to a digest.
    pub image: String,

    /// GitHub repository that published the image.
    /// Override only if you host a private mirror of the platform.
    #[arg(long, default_value = DEFAULT_REPO)]
    pub repo: String,

    /// Pin the signing identity to a single git ref, e.g. `refs/tags/v1.2.3`
    /// or `refs/heads/main`. When omitted, only `refs/heads/main` is accepted.
    #[arg(long)]
    pub r#ref: Option<String>,
}

pub fn run(opts: VerifyImageArgs, stdout: &mut dyn Write) -> CliResult<()> {
    if opts.image.trim().is_empty() {
        return Err(CliError::Other("image reference must not be empty".into()));
    }

    let args = cosign_verify_args(&opts.image, &opts.repo, opts.r#ref.as_deref());

    let status = Command::new("cosign").args(&args).status().map_err(|e| {
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
            "cosign verify failed — image is NOT verified\n\
             The image at `{}` was not signed by the official publish workflow \
             ({}) in repo {}. Pull only images you can verify, and report \
             unexpected signers as a security issue.",
            opts.image,
            expected_signer_description(&opts.image),
            opts.repo
        )));
    }

    let _ = writeln!(stdout, "verified: {} (signed by an official image workflow in {})", opts.image, opts.repo);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_regexp_pins_workflow_and_main_by_default() {
        let id = identity_regexp(DEFAULT_REPO, "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex:latest", None);
        // Dots are escaped (regex metacharacter); hyphens are NOT (literal in
        // RE2 outside a character class), keeping the pattern readable.
        assert_eq!(
            id,
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/(\\.github/workflows/publish-images\\.yml|\\.github/workflows/watch-cli-versions\\.yml|\\.github/workflows/release\\.yml)@refs/heads/main$"
        );
    }

    #[test]
    fn identity_regexp_pins_exact_ref_when_supplied() {
        let id = identity_regexp(
            DEFAULT_REPO,
            "ghcr.io/wisdoverse/wisdoverse-forge/agent-gemini@sha256:abc",
            Some("refs/tags/v1.2.3"),
        );
        assert_eq!(
            id,
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/(\\.github/workflows/publish-images\\.yml|\\.github/workflows/watch-cli-versions\\.yml|\\.github/workflows/release\\.yml)@refs/tags/v1\\.2\\.3$"
        );
    }

    #[test]
    fn identity_regexp_is_anchored_at_both_ends() {
        let id = identity_regexp(DEFAULT_REPO, "ghcr.io/wisdoverse/wisdoverse-forge/agent-opencode:latest", None);
        assert!(id.starts_with('^'), "identity must be anchored at start: {id}");
        assert!(id.ends_with('$'), "identity must be anchored at end: {id}");
    }

    #[test]
    fn identity_regexp_escapes_repo_metacharacters() {
        // A repo (e.g. a private mirror) containing a `.` must not let that dot
        // act as a regex wildcard.
        let id = identity_regexp("acme.co/Mirror", "registry.example/agent-codex:latest", None);
        assert!(id.contains("acme\\.co/Mirror"), "repo dot must be escaped: {id}");
    }

    #[test]
    fn identity_regexp_rejects_fork_and_evil_repo() {
        // Apply the COMPILED regex to attack URLs — guards against a future
        // edit that accidentally unanchors or broadens the pattern (asserting
        // the string shape alone would not catch that).
        let id = identity_regexp(DEFAULT_REPO, "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex:latest", None);
        let re = regex::Regex::new(&id).expect("identity regexp compiles");

        // Legitimate signer on main must match.
        assert!(re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/publish-images.yml@refs/heads/main"
        ));
        assert!(re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/watch-cli-versions.yml@refs/heads/main"
        ));
        assert!(
            re.is_match("https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/release.yml@refs/heads/main")
        );
        assert!(!re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/publish-images.yml@refs/tags/v1.2.3"
        ));
        // Different org/repo — must NOT match.
        assert!(!re.is_match("https://github.com/EVIL/repo/.github/workflows/publish-images.yml@refs/heads/main"));
        // Same org, attacker repo — must NOT match.
        assert!(
            !re.is_match("https://github.com/Wisdoverse/attacker/.github/workflows/publish-images.yml@refs/heads/main")
        );
        // Repo-name extension (Wisdoverse-ForgeEvil) — must NOT match.
        assert!(!re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-ForgeEvil/.github/workflows/publish-images.yml@refs/heads/main"
        ));
        // Empty ref segment — must NOT match.
        assert!(!re.is_match("https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/publish-images.yml@"));
        // A different workflow in the official repo — must NOT match.
        assert!(
            !re.is_match("https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/evil.yml@refs/heads/main")
        );
    }

    #[test]
    fn non_cli_images_trust_only_the_two_main_image_workflows() {
        let id = identity_regexp(DEFAULT_REPO, "ghcr.io/wisdoverse/wisdoverse-forge/sidecar:main", None);
        let re = regex::Regex::new(&id).unwrap();
        assert!(re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/publish-images.yml@refs/heads/main"
        ));
        assert!(
            re.is_match("https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/release.yml@refs/heads/main")
        );
        assert!(!re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/watch-cli-versions.yml@refs/heads/main"
        ));
    }

    #[test]
    fn cosign_args_pin_identity_and_official_oidc_issuer() {
        let image = "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex@sha256:abc";
        let identity = identity_regexp(DEFAULT_REPO, image, None);
        let args = cosign_verify_args(image, DEFAULT_REPO, None);

        // First positional arg is the cosign subcommand.
        assert_eq!(args[0], "verify");

        // Identity regexp is passed (pinned signer), not "any signature".
        let idx = args.iter().position(|a| a == "--certificate-identity-regexp").expect("identity flag present");
        assert_eq!(args[idx + 1], identity);
        assert!(args[idx + 1].contains("publish-images\\.yml"), "must pin the publish workflow");
        assert!(args[idx + 1].contains("watch-cli-versions\\.yml"), "must pin the CLI rebuild workflow");
        assert!(args[idx + 1].contains("release\\.yml"), "must pin the release workflow");

        // OIDC issuer is pinned to GitHub Actions.
        let issuer_idx = args.iter().position(|a| a == "--certificate-oidc-issuer").expect("oidc issuer flag present");
        assert_eq!(args[issuer_idx + 1], agentforge_core::image_trust::COSIGN_OIDC_ISSUER);

        // The image reference is the final positional arg.
        assert_eq!(args.last().unwrap(), image);

        // Defense against a regression to a permissive verify: an identity
        // (regexp or exact) MUST always be present so cosign cannot accept an
        // arbitrary Sigstore signature.
        let has_identity = args.iter().any(|a| a == "--certificate-identity-regexp" || a == "--certificate-identity");
        assert!(has_identity, "a pinned certificate identity must always be passed: {args:?}");
    }

    #[test]
    fn run_rejects_empty_image_reference() {
        let args = VerifyImageArgs { image: "   ".into(), repo: DEFAULT_REPO.into(), r#ref: None };
        let mut out = Vec::new();
        let result = run(args, &mut out);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }
}
