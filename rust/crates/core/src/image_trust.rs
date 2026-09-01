//! Shared policy for verifying published container-image signatures.

pub const COSIGN_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
pub const DEFAULT_IMAGE_REPOSITORY: &str = "Wisdoverse/Wisdoverse-Forge";
pub const PUBLISH_IMAGES_WORKFLOW: &str = ".github/workflows/publish-images.yml";
pub const CLI_REBUILD_WORKFLOW: &str = ".github/workflows/watch-cli-versions.yml";
pub const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";
pub const TRUSTED_IMAGE_WORKFLOW_REF: &str = "refs/heads/main";

/// Only these public Container CLI overlays are published by the CLI rebuild
/// workflow. Every other image must be signed by `publish-images.yml` or by
/// the main-only release workflow that builds versioned runtime images.
pub fn is_public_cli_overlay(image: &str) -> bool {
    let leaf = image.rsplit('/').next().unwrap_or(image);
    let name = leaf.split(['@', ':']).next().unwrap_or(leaf);
    matches!(name, "agent-opencode" | "agent-codex" | "agent-gemini")
}

/// Build the anchored GitHub Actions certificate-identity allowlist for an
/// image. Repository and ref inputs are escaped before entering the regexp.
pub fn cosign_identity_regexp(repo: &str, image: &str, git_ref: Option<&str>) -> String {
    let workflows = if is_public_cli_overlay(image) {
        [PUBLISH_IMAGES_WORKFLOW, CLI_REBUILD_WORKFLOW, RELEASE_WORKFLOW]
            .into_iter()
            .map(escape_regex)
            .collect::<Vec<_>>()
            .join("|")
    } else {
        [PUBLISH_IMAGES_WORKFLOW, RELEASE_WORKFLOW].into_iter().map(escape_regex).collect::<Vec<_>>().join("|")
    };
    let git_ref = git_ref.unwrap_or(TRUSTED_IMAGE_WORKFLOW_REF);
    format!("^https://github\\.com/{}/({workflows})@{}$", escape_regex(repo), escape_regex(git_ref))
}

/// Exact `cosign verify` argv used by operator and runtime verification.
pub fn cosign_verify_args(image: &str, repo: &str, git_ref: Option<&str>) -> Vec<String> {
    vec![
        "verify".to_string(),
        "--certificate-identity-regexp".to_string(),
        cosign_identity_regexp(repo, image, git_ref),
        "--certificate-oidc-issuer".to_string(),
        COSIGN_OIDC_ISSUER.to_string(),
        image.to_string(),
    ]
}

pub fn expected_signer_description(image: &str) -> &'static str {
    if is_public_cli_overlay(image) {
        ".github/workflows/publish-images.yml, .github/workflows/watch-cli-versions.yml, or .github/workflows/release.yml on main"
    } else {
        ".github/workflows/publish-images.yml or .github/workflows/release.yml on main"
    }
}

fn escape_regex(value: &str) -> String {
    const SPECIAL: &[char] = &['\\', '.', '+', '*', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|'];
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if SPECIAL.contains(&character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_overlay_policy_allows_only_the_three_exact_workflows() {
        let image = "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex@sha256:abc";
        assert!(is_public_cli_overlay(image));
        assert!(!is_public_cli_overlay("ghcr.io/wisdoverse/wisdoverse-forge/agent-codex-helper:latest"));
        assert_eq!(
            cosign_identity_regexp(DEFAULT_IMAGE_REPOSITORY, image, None),
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/(\\.github/workflows/publish-images\\.yml|\\.github/workflows/watch-cli-versions\\.yml|\\.github/workflows/release\\.yml)@refs/heads/main$"
        );
        assert_eq!(
            cosign_identity_regexp(DEFAULT_IMAGE_REPOSITORY, "ghcr.io/wisdoverse/wisdoverse-forge/sidecar:main", None,),
            "^https://github\\.com/Wisdoverse/Wisdoverse-Forge/(\\.github/workflows/publish-images\\.yml|\\.github/workflows/release\\.yml)@refs/heads/main$"
        );
    }

    #[test]
    fn cosign_args_pin_repo_ref_issuer_and_exact_image() {
        let image = "ghcr.io/wisdoverse/wisdoverse-forge/agent-gemini@sha256:abc";
        let args = cosign_verify_args(image, "acme.co/Mirror", Some("refs/tags/v1.2.3"));
        assert_eq!(args[0], "verify");
        assert_eq!(args[1], "--certificate-identity-regexp");
        assert_eq!(
            args[2],
            "^https://github\\.com/acme\\.co/Mirror/(\\.github/workflows/publish-images\\.yml|\\.github/workflows/watch-cli-versions\\.yml|\\.github/workflows/release\\.yml)@refs/tags/v1\\.2\\.3$"
        );
        assert_eq!(args[3..5], ["--certificate-oidc-issuer", COSIGN_OIDC_ISSUER]);
        assert_eq!(args.last().map(String::as_str), Some(image));
    }
}
