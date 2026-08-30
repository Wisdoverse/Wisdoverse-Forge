//! Deployment image refs shared by readiness, MCP creation, and Agent start.

use std::collections::HashMap;
use std::env;

use agentforge_core::{AppResult, CliToolKind, ErrorKind};
use agentforge_platform::{LocalImageIdentity, verify_image_signature};

use crate::domain::agent::{AgentContainerImageIdentity, AgentContainerImageTrust};

pub(crate) fn configured_cli_images() -> HashMap<String, String> {
    CliToolKind::ALL.into_iter().map(|tool| (tool.as_str().to_string(), configured_cli_image(tool))).collect()
}

pub(crate) fn configured_cli_image(tool: CliToolKind) -> String {
    let env_name = format!("CONTAINER_IMAGE_{}", tool.as_str().to_ascii_uppercase());
    resolve_cli_image(tool, env::var(env_name).ok().as_deref())
}

pub(crate) fn recorded_image_trust_is_acceptable(tool: CliToolKind, trust: Option<&str>) -> bool {
    matches!(trust, Some("verified-signature"))
        || matches!(tool, CliToolKind::Claude) && matches!(trust, Some("host-local"))
}

fn resolve_cli_image(tool: CliToolKind, configured: Option<&str>) -> String {
    configured
        .map(str::trim)
        .filter(|image| !image.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("agentforge-agent:{}", tool.as_str()))
}

pub(crate) async fn capture_container_image_identity(
    tool: CliToolKind,
    configured_source: &str,
    identity: &LocalImageIdentity,
) -> AppResult<AgentContainerImageIdentity> {
    let (source, trust) = match identity.registry_reference.as_deref() {
        Some(registry_reference) => {
            verify_image_signature(registry_reference).await.map_err(|err| {
                let code = err.image_verification_code().unwrap_or("image_signature_verification_failed");
                ErrorKind::Unavailable(format!("{code}: {}", image_verification_recovery(code)))
            })?;
            (registry_reference.to_string(), AgentContainerImageTrust::VerifiedSignature)
        }
        None if identity.manifest_digest.is_none() && matches!(tool, CliToolKind::Claude) => {
            (configured_source.to_string(), AgentContainerImageTrust::HostLocal)
        }
        None if identity.manifest_digest.is_none() => {
            return Err(ErrorKind::Unavailable(format!(
                "image_host_local_not_allowed: Only Claude may use an implicit host-local Container CLI image; configure and pull a signed public registry image for {}",
                tool.as_str()
            ))
            .into());
        }
        None => {
            return Err(ErrorKind::Unavailable(
                "image_registry_source_ambiguous: The image source is ambiguous; pull it again from the configured registry or, for Claude only, use a host-local build"
                    .to_string(),
            )
            .into());
        }
    };
    Ok(image_identity_evidence(source, identity, trust))
}

pub(crate) fn image_verification_failure(err: &agentforge_core::AppError) -> (&'static str, &'static str) {
    const CODES: &[&str] = &[
        "image_verifier_missing",
        "image_verifier_start_failed",
        "image_verification_timeout",
        "image_registry_auth_failed",
        "image_registry_unreachable",
        "image_signature_untrusted",
        "image_registry_source_ambiguous",
        "image_host_local_not_allowed",
        "image_signature_verification_failed",
    ];
    let error = err.to_string();
    let code = CODES.iter().copied().find(|code| error.contains(code)).unwrap_or("image_signature_verification_failed");
    (code, image_verification_recovery(code))
}

fn image_verification_recovery(code: &str) -> &'static str {
    match code {
        "image_verifier_missing" | "image_verifier_start_failed" => {
            "The server image verifier is unavailable; reinstall or update Forge, then check again"
        }
        "image_registry_auth_failed" => {
            "Stock Compose verifies public signed registries only; use a public image until explicit private-registry credential integration is available"
        }
        "image_registry_unreachable" | "image_verification_timeout" => {
            "The image registry could not be reached; check registry access, then check again"
        }
        "image_registry_source_ambiguous" => {
            "The image source is ambiguous; pull it again from the configured registry or, for Claude only, use a host-local build"
        }
        "image_host_local_not_allowed" => {
            "Only Claude supports the host-local build path; configure and pull a signed public registry image for this Container CLI"
        }
        _ => {
            "The image signature is missing or not from the configured trusted signer; publish or pull a trusted image, then check again"
        }
    }
}

fn image_identity_evidence(
    source: String,
    identity: &LocalImageIdentity,
    trust: AgentContainerImageTrust,
) -> AgentContainerImageIdentity {
    let version = identity
        .labels
        .get("org.agentforge.cli-version")
        .or_else(|| identity.labels.get("org.wisdoverse.cli-version"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "<no value>")
        .map(str::to_string);
    AgentContainerImageIdentity {
        source,
        image_id: identity.id.clone(),
        manifest_digest: identity.manifest_digest.clone(),
        version_source: if version.is_some() { "docker-label" } else { "not-reported" }.to_string(),
        version,
        trust,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_override_and_canonical_fallback_are_exact() {
        assert_eq!(
            resolve_cli_image(CliToolKind::Codex, Some(" registry.example/agent@sha256:abc ")),
            "registry.example/agent@sha256:abc"
        );
        assert_eq!(resolve_cli_image(CliToolKind::Gemini, Some("  ")), "agentforge-agent:gemini");
        assert_eq!(resolve_cli_image(CliToolKind::Claude, None), "agentforge-agent:claude");
    }

    #[test]
    fn persisted_host_local_evidence_is_reusable_only_for_claude() {
        assert!(recorded_image_trust_is_acceptable(CliToolKind::Claude, Some("host-local")));
        assert!(!recorded_image_trust_is_acceptable(CliToolKind::Codex, Some("host-local")));
        assert!(recorded_image_trust_is_acceptable(CliToolKind::Codex, Some("verified-signature")));
        assert!(!recorded_image_trust_is_acceptable(CliToolKind::Gemini, None));
    }

    #[test]
    fn local_build_evidence_never_claims_signature_verification() {
        let identity = LocalImageIdentity {
            id: "sha256:image".into(),
            manifest_digest: None,
            registry_reference: None,
            labels: HashMap::from([("org.agentforge.cli-version".into(), "1.2.3".into())]),
        };

        let evidence =
            image_identity_evidence("agentforge-agent:claude".into(), &identity, AgentContainerImageTrust::HostLocal);

        assert_eq!(evidence.image_id, "sha256:image");
        assert_eq!(evidence.version.as_deref(), Some("1.2.3"));
        assert_eq!(evidence.trust, AgentContainerImageTrust::HostLocal);
    }

    #[tokio::test]
    async fn local_claude_build_is_reported_honestly() {
        let identity = LocalImageIdentity {
            id: "sha256:image".into(),
            manifest_digest: None,
            registry_reference: None,
            labels: HashMap::new(),
        };

        let evidence = capture_container_image_identity(CliToolKind::Claude, "agentforge-agent:claude", &identity)
            .await
            .expect("an explicit local build has immutable Docker identity");

        assert_eq!(evidence.trust, AgentContainerImageTrust::HostLocal);
    }

    #[tokio::test]
    async fn local_non_claude_image_fails_closed() {
        let identity = LocalImageIdentity {
            id: "sha256:image".into(),
            manifest_digest: None,
            registry_reference: None,
            labels: HashMap::new(),
        };

        let error = capture_container_image_identity(CliToolKind::Codex, "agentforge-agent:codex", &identity)
            .await
            .expect_err("non-Claude local images must not bypass registry signature verification");

        assert!(error.to_string().contains("image_host_local_not_allowed"));
        assert!(error.to_string().contains("Only Claude"));
    }

    #[tokio::test]
    async fn ambiguous_registry_provenance_fails_closed() {
        let identity = LocalImageIdentity {
            id: "sha256:image".into(),
            manifest_digest: Some(format!("sha256:{}", "a".repeat(64))),
            registry_reference: None,
            labels: HashMap::new(),
        };

        let error = capture_container_image_identity(CliToolKind::Codex, "agentforge-agent:codex", &identity)
            .await
            .expect_err("registry content without an attributable source must fail closed");

        assert!(error.to_string().contains("image_registry_source_ambiguous"));
    }
}
