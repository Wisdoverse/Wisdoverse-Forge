//! `agentforge verify-image` — verify a published container image against its
//! Sigstore cosign signature produced by the `publish-images.yml` workflow.
//!
//! ## Why this exists
//!
//! The ghcr.io container images are the PRIMARY shipped artifacts for the
//! container runtime (sidecar, server, orchestrator, agent images). They are
//! signed with Sigstore keyless cosign BY DIGEST in `publish-images.yml`.
//! This command lets an operator confirm an image was built and signed by the
//! official publish workflow before running it.
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
//!   2. The token was issued for the `publish-images.yml` workflow in the
//!      official Wisdoverse Forge repository (identity PINNED via regexp).
//!   3. The token issuer is `https://token.actions.githubusercontent.com`.
//!
//! An image signed by a fork, a different workflow, or a different repository
//! FAILS — we never trust "any Sigstore signature". The default identity
//! regexp accepts the workflow at any ref (`@refs/heads/main`, `@refs/tags/v*`,
//! or a `workflow_dispatch` ref) of the official repo; pass `--ref` to pin a
//! single ref such as `refs/tags/v1.2.3`.

use crate::error::{CliError, CliResult};
use clap::Args;
use std::io::Write;
use std::process::Command;

const OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
const DEFAULT_REPO: &str = "Wisdoverse/Wisdoverse-Forge";
/// The workflow file that signs published images. Pinned in the identity so a
/// signature from any other workflow (or repo) fails closed.
const PUBLISH_WORKFLOW: &str = ".github/workflows/publish-images.yml";

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
    /// or `refs/heads/main`. When omitted, any ref of the official publish
    /// workflow is accepted (the workflow file + repo are always pinned).
    #[arg(long)]
    pub r#ref: Option<String>,
}

/// Build the `--certificate-identity-regexp` value pinning the signer to the
/// official `publish-images.yml` workflow.
///
/// The repo and workflow path are always pinned. The ref portion is either the
/// caller-supplied exact ref or a wildcard accepting any ref of that workflow.
/// Regex metacharacters in the repo/ref are escaped so a `.` in a repo name
/// (or a crafted ref) cannot widen the match.
fn identity_regexp(repo: &str, git_ref: Option<&str>) -> String {
    let repo_esc = escape_regex(repo);
    let workflow_esc = escape_regex(PUBLISH_WORKFLOW);
    let prefix = format!("^https://github\\.com/{repo_esc}/{workflow_esc}@");
    match git_ref {
        Some(r) => format!("{prefix}{}$", escape_regex(r)),
        // `.+` (not `.*`) requires a non-empty ref segment so a bare
        // `...@` cannot match.
        None => format!("{prefix}.+$"),
    }
}

/// Escape RE2 (cosign/Go regexp) metacharacters so user-supplied repo/ref
/// values are matched literally.
fn escape_regex(s: &str) -> String {
    const SPECIAL: &[char] = &['\\', '.', '+', '*', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|'];
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Assemble the `cosign verify` argument vector. Factored out so the exact
/// flags + pinned identity can be unit-tested without a live registry.
fn cosign_args(image: &str, identity_regexp: &str) -> Vec<String> {
    vec![
        "verify".to_string(),
        "--certificate-identity-regexp".to_string(),
        identity_regexp.to_string(),
        "--certificate-oidc-issuer".to_string(),
        OIDC_ISSUER.to_string(),
        image.to_string(),
    ]
}

pub fn run(opts: VerifyImageArgs, stdout: &mut dyn Write) -> CliResult<()> {
    if opts.image.trim().is_empty() {
        return Err(CliError::Other("image reference must not be empty".into()));
    }

    let identity = identity_regexp(&opts.repo, opts.r#ref.as_deref());
    let args = cosign_args(&opts.image, &identity);

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
            opts.image, PUBLISH_WORKFLOW, opts.repo
        )));
    }

    let _ = writeln!(stdout, "verified: {} (signed by {} in {})", opts.image, PUBLISH_WORKFLOW, opts.repo);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_regexp_pins_workflow_and_accepts_any_ref_by_default() {
        let id = identity_regexp(DEFAULT_REPO, None);
        // Dots are escaped (regex metacharacter); hyphens are NOT (literal in
        // RE2 outside a character class), keeping the pattern readable.
        assert_eq!(
            id,
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/\\.github/workflows/publish-images\\.yml@.+$"
        );
    }

    #[test]
    fn identity_regexp_pins_exact_ref_when_supplied() {
        let id = identity_regexp(DEFAULT_REPO, Some("refs/tags/v1.2.3"));
        assert_eq!(
            id,
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/\\.github/workflows/publish-images\\.yml@refs/tags/v1\\.2\\.3$"
        );
    }

    #[test]
    fn identity_regexp_is_anchored_at_both_ends() {
        let id = identity_regexp(DEFAULT_REPO, None);
        assert!(id.starts_with('^'), "identity must be anchored at start: {id}");
        assert!(id.ends_with('$'), "identity must be anchored at end: {id}");
    }

    #[test]
    fn identity_regexp_escapes_repo_metacharacters() {
        // A repo (e.g. a private mirror) containing a `.` must not let that dot
        // act as a regex wildcard.
        let id = identity_regexp("acme.co/Mirror", None);
        assert!(id.contains("acme\\.co/Mirror"), "repo dot must be escaped: {id}");
    }

    #[test]
    fn identity_regexp_rejects_fork_and_evil_repo() {
        // Apply the COMPILED regex to attack URLs — guards against a future
        // edit that accidentally unanchors or broadens the pattern (asserting
        // the string shape alone would not catch that).
        let id = identity_regexp(DEFAULT_REPO, None);
        let re = regex::Regex::new(&id).expect("identity regexp compiles");

        // Legitimate signer (any ref) must match.
        assert!(re.is_match(
            "https://github.com/Wisdoverse/Wisdoverse-Forge/.github/workflows/publish-images.yml@refs/heads/main"
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
    fn cosign_args_pin_identity_and_official_oidc_issuer() {
        let identity = identity_regexp(DEFAULT_REPO, None);
        let args = cosign_args("ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:abc", &identity);

        // First positional arg is the cosign subcommand.
        assert_eq!(args[0], "verify");

        // Identity regexp is passed (pinned signer), not "any signature".
        let idx = args.iter().position(|a| a == "--certificate-identity-regexp").expect("identity flag present");
        assert_eq!(args[idx + 1], identity);
        assert!(args[idx + 1].contains("publish-images\\.yml"), "must pin the publish workflow");

        // OIDC issuer is pinned to GitHub Actions.
        let issuer_idx = args.iter().position(|a| a == "--certificate-oidc-issuer").expect("oidc issuer flag present");
        assert_eq!(args[issuer_idx + 1], OIDC_ISSUER);

        // The image reference is the final positional arg.
        assert_eq!(args.last().unwrap(), "ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:abc");

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
