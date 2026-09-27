//! Image operations: pull, digest/label inspection (local + remote), re-tag,
//! and a Dockerfile-only `docker build`.
//!
//! Used by the deployment-side CLI agent-image auto-updater to detect when a
//! Container CLI overlay (`agent-<tool>:latest`) has a newer manifest on the
//! registry and to refresh the local image the runtime spawns from, and by the
//! claude local-build path (no public registry image) to build the overlay
//! server-side. All operations are image-level only — they NEVER create a
//! container, build a `HostConfig`, or touch `security.rs`, so the
//! container-creation defense-in-depth is unaffected.

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::time::Duration;

use agentforge_core::image_trust::{DEFAULT_IMAGE_REPOSITORY, cosign_verify_args, is_public_cli_overlay};
use bollard::auth::DockerCredentials;
use bollard::models::ImageInspect;
use bollard::query_parameters::{
    BuildImageOptionsBuilder, CreateImageOptionsBuilder, ListContainersOptionsBuilder, ListImagesOptionsBuilder,
    RemoveImageOptionsBuilder, TagImageOptionsBuilder,
};
use futures_util::StreamExt;
use tokio::process::Command;

use crate::container::PlatformError;
use crate::docker::DockerClient;

/// A local image reduced to the fields the prune scoping policy needs. The
/// policy itself (which images are superseded agent overlays) lives in the jobs
/// worker; this type + its lister stay I/O-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalImage {
    /// Content id, e.g. `sha256:...` — what `remove_image_if_unreferenced` takes
    /// and what `referenced_image_ids` compares against.
    pub id: String,
    /// `repo:tag` strings; empty for a dangling (untagged) image.
    pub repo_tags: Vec<String>,
    /// `repo@sha256:...` strings; identify the source repo of a dangling image.
    pub repo_digests: Vec<String>,
}

/// Immutable identity and operator-facing metadata for one local image ref.
///
/// Docker tags are mutable. Callers that create security-sensitive containers
/// resolve the configured tag once, then pass this content id to Docker so a
/// concurrent re-tag cannot swap the image between readiness and create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalImageIdentity {
    /// Docker content/config id (`sha256:...`), accepted as an immutable image
    /// reference by the daemon.
    pub id: String,
    /// Registry manifest digest when the image was pulled from a registry.
    /// Locally-built images legitimately have no manifest digest.
    pub manifest_digest: Option<String>,
    /// Immutable source registry reference (`repo@sha256:...`) corresponding
    /// to the inspected configured ref. Local builds have no registry source.
    pub registry_reference: Option<String>,
    pub labels: HashMap<String, String>,
}

const IMAGE_SIGNATURE_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify one immutable registry image against the repository's pinned
/// GitHub Actions signer allowlist. Mutable tags are rejected before `cosign`
/// starts; callers decide which runtime environments require this proof.
pub async fn verify_image_signature(registry_reference: &str) -> Result<(), PlatformError> {
    let signer_repository = configured_image_signer_repository();
    verify_image_signature_for_repository(registry_reference, &signer_repository).await
}

async fn verify_image_signature_for_repository(
    registry_reference: &str,
    signer_repository: &str,
) -> Result<(), PlatformError> {
    if !is_immutable_registry_reference(registry_reference) {
        return Err(PlatformError::ImageVerification(
            "expected an immutable repo@sha256:<64 hex> reference".to_string(),
        ));
    }

    let mut command = Command::new("cosign");
    command
        .args(cosign_verify_args(registry_reference, signer_repository, None))
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(IMAGE_SIGNATURE_VERIFICATION_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(PlatformError::ImageVerification("image_verifier_missing".to_string()));
        }
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "could not start cosign image verifier");
            return Err(PlatformError::ImageVerification("image_verifier_start_failed".to_string()));
        }
        Err(_) => {
            return Err(PlatformError::ImageVerification("image_verification_timeout".to_string()));
        }
    };

    if !output.status.success() {
        let diagnostic = bounded_diagnostic(&output.stderr);
        let code = classify_cosign_failure(&diagnostic);
        tracing::warn!(code, diagnostic, "cosign rejected container image");
        return Err(PlatformError::ImageVerification(code.to_string()));
    }
    Ok(())
}

fn bounded_diagnostic(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .take(512)
        .collect()
}

fn classify_cosign_failure(diagnostic: &str) -> &'static str {
    let diagnostic = diagnostic.to_ascii_lowercase();
    if diagnostic.contains("unauthorized")
        || diagnostic.contains("authentication required")
        || diagnostic.contains("denied")
    {
        "image_registry_auth_failed"
    } else if diagnostic.contains("no such host")
        || diagnostic.contains("connection refused")
        || diagnostic.contains("connection reset")
        || diagnostic.contains("dial tcp")
    {
        "image_registry_unreachable"
    } else if diagnostic.contains("no matching signatures")
        || diagnostic.contains("no signatures")
        || diagnostic.contains("certificate identity")
        || diagnostic.contains("signature")
    {
        "image_signature_untrusted"
    } else {
        "image_signature_verification_failed"
    }
}

fn configured_image_signer_repository() -> String {
    std::env::var("AGENT_IMAGE_SIGNER_REPOSITORY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE_REPOSITORY.to_string())
}

/// Outcome of a single image-removal decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// The image was removed.
    Removed,
    /// A running or stopped container still references the image — left intact.
    SkippedInUse,
    /// Docker returned 409 (still referenced by another tag / child) — left intact.
    SkippedConflict,
    /// The image was already gone.
    NotFound,
}

impl DockerClient {
    /// Resolve a local tag/ref to the immutable identity Docker will run.
    /// Missing images return `Ok(None)`; daemon failures remain errors.
    pub async fn local_image_identity(&self, image_ref: &str) -> Result<Option<LocalImageIdentity>, PlatformError> {
        match self.inner().inspect_image(image_ref).await {
            Ok(info) => Ok(local_image_identity(info, image_ref)),
            Err(err) => {
                let platform_err = PlatformError::Docker(err);
                if platform_err.is_missing_image() || platform_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(platform_err)
                }
            }
        }
    }

    /// Inspect an immutable local image id while resolving registry provenance
    /// against its configured source repository. This keeps legacy/custom
    /// running containers attributable after the mutable tag has moved.
    pub async fn local_image_identity_for_source(
        &self,
        image_id: &str,
        configured_source: &str,
    ) -> Result<Option<LocalImageIdentity>, PlatformError> {
        match self.inner().inspect_image(image_id).await {
            Ok(info) => Ok(local_image_identity(info, configured_source)),
            Err(err) => {
                let platform_err = PlatformError::Docker(err);
                if platform_err.is_missing_image() || platform_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(platform_err)
                }
            }
        }
    }

    /// Pull `image_ref` (e.g. `ghcr.io/org/agent-codex:latest`) from its
    /// registry. The progress stream is drained fully so the pull has completed
    /// before this returns. `credentials` is `None` for public images (the
    /// daemon's ambient registry auth is used).
    pub async fn pull_image(
        &self,
        image_ref: &str,
        credentials: Option<DockerCredentials>,
    ) -> Result<(), PlatformError> {
        let (repo, tag) = split_image_ref(image_ref);
        let options = CreateImageOptionsBuilder::default().from_image(&repo).tag(&tag).build();
        let mut stream = self.inner().create_image(Some(options), None, credentials);
        while let Some(item) = stream.next().await {
            item.map_err(|err| PlatformError::Pull(err.to_string()))?;
        }
        Ok(())
    }

    /// Local content (manifest) digest of an already-pulled image tag, as a
    /// `sha256:...` string. `Ok(None)` when the image is not present locally —
    /// the caller treats that as drift (a pull is needed).
    pub async fn local_image_digest(&self, image_ref: &str) -> Result<Option<String>, PlatformError> {
        match self.inner().inspect_image(image_ref).await {
            Ok(info) => Ok(extract_local_digest(&info)),
            Err(err) => {
                let platform_err = PlatformError::Docker(err);
                if platform_err.is_missing_image() || platform_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(platform_err)
                }
            }
        }
    }

    /// Remote manifest digest for `image_ref` from the registry WITHOUT pulling
    /// (daemon-side `GET /distribution/<image>/json`, reusing the daemon's
    /// registry auth). Comparable to [`local_image_digest`] for the same ref.
    pub async fn remote_image_digest(
        &self,
        image_ref: &str,
        credentials: Option<DockerCredentials>,
    ) -> Result<String, PlatformError> {
        let info = self
            .inner()
            .inspect_registry_image(image_ref, credentials)
            .await
            .map_err(|err| PlatformError::Registry(err.to_string()))?;
        info.descriptor
            .digest
            .filter(|digest| !digest.is_empty())
            .ok_or_else(|| PlatformError::Registry(format!("registry returned no digest for {image_ref}")))
    }

    /// Re-tag `source_ref` as `target_repo:target_tag`. Used to point the
    /// runtime's image ref (`agentforge-agent:<tool>`) at the freshly pulled
    /// registry content so the NEXT spawned agent uses the new CLI.
    pub async fn tag_image(&self, source_ref: &str, target_repo: &str, target_tag: &str) -> Result<(), PlatformError> {
        let options = TagImageOptionsBuilder::default().repo(target_repo).tag(target_tag).build();
        self.inner().tag_image(source_ref, Some(options)).await.map_err(PlatformError::Docker)
    }

    /// The `Config.Labels` map of a locally-present image, or `Ok(None)` when
    /// the image does not exist on this host. Used by the claude local-build
    /// detector to read the baked `org.agentforge.cli-version` label without
    /// the caller hand-rolling missing-image error classification.
    pub async fn image_labels(&self, image_ref: &str) -> Result<Option<HashMap<String, String>>, PlatformError> {
        match self.inner().inspect_image(image_ref).await {
            Ok(info) => Ok(Some(info.config.and_then(|config| config.labels).unwrap_or_default())),
            Err(err) => {
                let platform_err = PlatformError::Docker(err);
                if platform_err.is_missing_image() || platform_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(platform_err)
                }
            }
        }
    }

    /// Whether `image_ref` exists locally. Same not-found classification as
    /// [`Self::image_labels`]; any other daemon error propagates.
    pub async fn image_exists(&self, image_ref: &str) -> Result<bool, PlatformError> {
        Ok(self.image_labels(image_ref).await?.is_some())
    }

    /// Build an image from an in-memory, Dockerfile-only build context and tag
    /// it `tag`. The context tar contains a single `Dockerfile` entry — no host
    /// directory is ever sent to the daemon — and `build_args` map onto the
    /// Dockerfile's `ARG`s. The progress stream is drained fully and any
    /// in-stream daemon error (`errorDetail` frame) is surfaced as
    /// [`PlatformError::Build`], so `Ok(())` means the image really landed.
    pub async fn build_image_from_dockerfile(
        &self,
        dockerfile: &str,
        tag: &str,
        build_args: &HashMap<String, String>,
    ) -> Result<(), PlatformError> {
        let context = dockerfile_tar_context(dockerfile)?;
        let options = BuildImageOptionsBuilder::default()
            .dockerfile("Dockerfile")
            .t(tag)
            .rm(true)
            .forcerm(true)
            .buildargs(build_args)
            .build();
        let mut stream = self.inner().build_image(options, None, Some(bollard::body_full(context.into())));
        while let Some(item) = stream.next().await {
            let info = item.map_err(|err| PlatformError::Build(err.to_string()))?;
            // The classic builder reports failures as in-stream `errorDetail`
            // JSON frames, not transport errors — check that surface explicitly.
            if let Some(detail) = info.error_detail.and_then(|d| d.message).filter(|m| !m.is_empty()) {
                return Err(PlatformError::Build(detail));
            }
        }
        Ok(())
    }

    /// The set of image content-ids (`sha256:...`) referenced by ANY container,
    /// running OR stopped (`all=true`). The prune path subtracts this set so an
    /// image a stopped container could restart from is never removed.
    pub async fn referenced_image_ids(&self) -> Result<HashSet<String>, PlatformError> {
        let options = ListContainersOptionsBuilder::default().all(true).build();
        let containers = self.inner().list_containers(Some(options)).await.map_err(PlatformError::Docker)?;
        Ok(containers.into_iter().filter_map(|c| c.image_id).collect())
    }

    /// All top-level local images (`all=false` hides intermediate layers;
    /// dangling/untagged leaf images are still returned). Scoping to agent
    /// overlays is the caller's policy, kept out of this I/O wrapper.
    pub async fn list_local_images(&self) -> Result<Vec<LocalImage>, PlatformError> {
        let options = ListImagesOptionsBuilder::default().all(false).build();
        let images = self.inner().list_images(Some(options)).await.map_err(PlatformError::Docker)?;
        Ok(images
            .into_iter()
            .map(|i| LocalImage { id: i.id, repo_tags: i.repo_tags, repo_digests: i.repo_digests })
            .collect())
    }

    /// Remove image `id` ONLY if no container references it (`referenced` from
    /// [`referenced_image_ids`]). `force=false` + `noprune=true` so a shared
    /// parent layer is never cascade-deleted. A 409 conflict (still tagged /
    /// has a child) is reported as [`RemoveOutcome::SkippedConflict`], not an
    /// error — defense-in-depth on top of the unreferenced check.
    pub async fn remove_image_if_unreferenced(
        &self,
        id: &str,
        referenced: &HashSet<String>,
    ) -> Result<RemoveOutcome, PlatformError> {
        if referenced.contains(id) {
            return Ok(RemoveOutcome::SkippedInUse);
        }
        let options = RemoveImageOptionsBuilder::default().force(false).noprune(true).build();
        match self.inner().remove_image(id, Some(options), None).await {
            Ok(_) => Ok(RemoveOutcome::Removed),
            Err(err) => {
                let platform_err = PlatformError::Docker(err);
                if platform_err.is_not_found() {
                    Ok(RemoveOutcome::NotFound)
                } else if platform_err.is_conflict() {
                    Ok(RemoveOutcome::SkippedConflict)
                } else {
                    Err(platform_err)
                }
            }
        }
    }
}

/// Serialize a single-entry (`Dockerfile`) uncompressed tar archive in memory,
/// the minimal build context `POST /build` accepts. Identity encoding — the
/// daemon also accepts gzip/bzip2/xz, but compressing a <2 KB Dockerfile buys
/// nothing.
fn dockerfile_tar_context(dockerfile: &str) -> Result<Vec<u8>, PlatformError> {
    let bytes = dockerfile.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();

    let mut builder = tar::Builder::new(Vec::new());
    builder
        .append_data(&mut header, "Dockerfile", bytes)
        .map_err(|err| PlatformError::Build(format!("tar context: {err}")))?;
    builder.into_inner().map_err(|err| PlatformError::Build(format!("tar context: {err}")))
}

/// Split a `repo[:tag]` reference into `(repo, tag)`, defaulting the tag to
/// `latest`. A `:` that appears before a `/` is a registry `host:port`, not a
/// tag, so only a `:` in the final path segment counts as the tag separator.
fn split_image_ref(image_ref: &str) -> (String, String) {
    match image_ref.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo.to_string(), tag.to_string()),
        _ => (image_ref.to_string(), "latest".to_string()),
    }
}

/// The image's `RepoDigests` manifest digest (`repo@sha256:...`), which is the
/// value comparable to a registry's manifest digest. Returns `None` when no
/// repo digest is recorded (a locally-built, never-pushed image) — the caller
/// treats `None` as "unknown → drift, pull once". We deliberately do NOT fall
/// back to the image `Id`: that is the *config* digest, which can never equal a
/// remote *manifest* digest, so using it would make every comparison report
/// drift and re-pull on every tick.
fn extract_local_digest(info: &ImageInspect) -> Option<String> {
    info.repo_digests.as_ref()?.iter().find_map(|rd| rd.rsplit_once('@').map(|(_, dig)| dig.to_string()))
}

fn image_repository(image_ref: &str) -> &str {
    let without_digest = image_ref.split_once('@').map_or(image_ref, |(repository, _)| repository);
    match without_digest.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => repository,
        _ => without_digest,
    }
}

fn extract_registry_reference(info: &ImageInspect, image_ref: &str) -> Option<String> {
    let repository = image_repository(image_ref);
    let repo_digests = info.repo_digests.as_ref()?;
    let exact = repo_digests
        .iter()
        .filter(|repo_digest| {
            repo_digest.split_once('@').is_some_and(|(candidate, digest)| candidate == repository && !digest.is_empty())
        })
        .collect::<Vec<_>>();
    match exact.as_slice() {
        [only] => return Some((**only).clone()),
        [] => {}
        _ => return None,
    }

    if image_ref.starts_with("sha256:") {
        let public_sources = repo_digests
            .iter()
            .filter(|repo_digest| {
                repo_digest.split_once('@').is_some_and(|(candidate, digest)| {
                    candidate.rsplit('/').next().is_some_and(is_public_cli_overlay) && !digest.is_empty()
                })
            })
            .collect::<Vec<_>>();
        return match public_sources.as_slice() {
            [only] => Some((**only).clone()),
            _ => None,
        };
    }

    let tool = image_ref.strip_prefix("agentforge-agent:")?;
    if tool.contains(['/', ':', '@']) {
        return None;
    }
    let overlay = format!("agent-{tool}");
    if !is_public_cli_overlay(&overlay) {
        return None;
    }
    let aliases = repo_digests
        .iter()
        .filter(|repo_digest| {
            repo_digest.split_once('@').is_some_and(|(candidate, digest)| {
                candidate.rsplit('/').next() == Some(overlay.as_str()) && !digest.is_empty()
            })
        })
        .collect::<Vec<_>>();
    match aliases.as_slice() {
        [only] => Some((**only).clone()),
        _ => None,
    }
}

fn is_immutable_registry_reference(reference: &str) -> bool {
    if reference.trim() != reference {
        return false;
    }
    let Some((repository, digest)) = reference.split_once('@') else {
        return false;
    };
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    !repository.is_empty()
        && !repository.contains('@')
        && hex.len() == 64
        && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn local_image_identity(info: ImageInspect, image_ref: &str) -> Option<LocalImageIdentity> {
    let registry_reference = extract_registry_reference(&info, image_ref);
    let manifest_digest = registry_reference
        .as_deref()
        .and_then(|reference| reference.rsplit_once('@').map(|(_, digest)| digest.to_string()))
        .or_else(|| extract_local_digest(&info));
    let id = info.id.filter(|id| !id.is_empty())?;
    let labels = info.config.and_then(|config| config.labels).unwrap_or_default();
    Some(LocalImageIdentity { id, manifest_digest, registry_reference, labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_image_ref_handles_tag_registry_and_default() {
        assert_eq!(split_image_ref("agent-codex:latest"), ("agent-codex".into(), "latest".into()));
        assert_eq!(
            split_image_ref("ghcr.io/org/agent-codex:1.2.3"),
            ("ghcr.io/org/agent-codex".into(), "1.2.3".into())
        );
        // host:port present, tag absent -> the port is NOT mistaken for a tag.
        assert_eq!(split_image_ref("registry:5000/agent-codex"), ("registry:5000/agent-codex".into(), "latest".into()));
        assert_eq!(split_image_ref("agent-codex"), ("agent-codex".into(), "latest".into()));
    }

    #[test]
    fn dockerfile_tar_context_contains_single_dockerfile_entry() {
        let content = "FROM scratch\nLABEL x=y\n";
        let archive = dockerfile_tar_context(content).expect("tar build");

        let mut entries = tar::Archive::new(archive.as_slice());
        let mut found = Vec::new();
        for entry in entries.entries().expect("entries") {
            let mut entry = entry.expect("entry");
            let path = entry.path().expect("path").to_string_lossy().to_string();
            let mut body = String::new();
            use std::io::Read;
            entry.read_to_string(&mut body).expect("read entry");
            found.push((path, body));
        }
        assert_eq!(found.len(), 1, "exactly one entry — no host context is ever sent");
        assert_eq!(found[0].0, "Dockerfile");
        assert_eq!(found[0].1, content);
    }

    #[test]
    fn extract_local_digest_uses_repo_digest_only() {
        let with_repo = ImageInspect {
            id: Some("sha256:configdigest".into()),
            repo_digests: Some(vec!["ghcr.io/org/agent-codex@sha256:manifestdigest".into()]),
            ..Default::default()
        };
        assert_eq!(extract_local_digest(&with_repo).as_deref(), Some("sha256:manifestdigest"));

        // No repo digest -> None (NOT the config Id, which would force a false
        // drift + perpetual re-pull). Covers a locally-built, never-pushed image.
        let id_only = ImageInspect { id: Some("sha256:configonly".into()), repo_digests: None, ..Default::default() };
        assert_eq!(extract_local_digest(&id_only), None);

        let empty = ImageInspect { id: None, repo_digests: Some(vec![]), ..Default::default() };
        assert_eq!(extract_local_digest(&empty), None);
    }

    #[test]
    fn local_identity_requires_content_id_and_keeps_manifest_and_labels() {
        let info = ImageInspect {
            id: Some("sha256:configdigest".into()),
            repo_digests: Some(vec![
                "ghcr.io/other/agent-codex@sha256:wrongdigest".into(),
                "ghcr.io/org/agent-codex@sha256:manifestdigest".into(),
            ]),
            config: Some(bollard::models::ImageConfig {
                labels: Some(HashMap::from([("org.agentforge.cli-version".into(), "1.2.3".into())])),
                ..Default::default()
            }),
            ..Default::default()
        };
        let identity = local_image_identity(info, "ghcr.io/org/agent-codex:latest").expect("identity");
        assert_eq!(identity.id, "sha256:configdigest");
        assert_eq!(identity.manifest_digest.as_deref(), Some("sha256:manifestdigest"));
        assert_eq!(identity.registry_reference.as_deref(), Some("ghcr.io/org/agent-codex@sha256:manifestdigest"));
        assert_eq!(identity.labels.get("org.agentforge.cli-version").map(String::as_str), Some("1.2.3"));

        assert!(local_image_identity(ImageInspect::default(), "agent-codex:latest").is_none());
    }

    #[test]
    fn signature_verification_accepts_only_immutable_sha256_registry_references() {
        let digest = "a".repeat(64);
        assert!(is_immutable_registry_reference(&format!("ghcr.io/org/agent-codex@sha256:{digest}")));
        assert!(!is_immutable_registry_reference("ghcr.io/org/agent-codex:latest"));
        assert!(!is_immutable_registry_reference("ghcr.io/org/agent-codex@sha256:abc"));
        assert!(!is_immutable_registry_reference(&format!(" ghcr.io/org/agent-codex@sha256:{digest}")));
    }

    #[test]
    fn cosign_failures_map_to_stable_safe_codes() {
        assert_eq!(classify_cosign_failure("UNAUTHORIZED: authentication required"), "image_registry_auth_failed");
        assert_eq!(classify_cosign_failure("dial tcp: connection refused"), "image_registry_unreachable");
        assert_eq!(classify_cosign_failure("no matching signatures"), "image_signature_untrusted");
        assert_eq!(classify_cosign_failure("opaque failure"), "image_signature_verification_failed");
    }

    #[test]
    fn canonical_runtime_alias_resolves_one_exact_public_overlay_repo_digest() {
        let info = ImageInspect {
            repo_digests: Some(vec![
                "ghcr.io/org/agent-codextra@sha256:sibling".into(),
                "ghcr.io/org/agent-codex@sha256:verified".into(),
            ]),
            ..Default::default()
        };
        assert_eq!(
            extract_registry_reference(&info, "agentforge-agent:codex").as_deref(),
            Some("ghcr.io/org/agent-codex@sha256:verified")
        );
    }

    #[test]
    fn canonical_runtime_alias_rejects_ambiguous_repo_digests() {
        let info = ImageInspect {
            repo_digests: Some(vec![
                "ghcr.io/one/agent-gemini@sha256:first".into(),
                "ghcr.io/two/agent-gemini@sha256:second".into(),
            ]),
            ..Default::default()
        };
        assert_eq!(extract_registry_reference(&info, "agentforge-agent:gemini"), None);
    }

    #[test]
    fn immutable_local_id_recovers_one_public_registry_source() {
        let info = ImageInspect {
            repo_digests: Some(vec!["ghcr.io/org/agent-codex@sha256:verified".into()]),
            ..Default::default()
        };
        assert_eq!(
            extract_registry_reference(&info, "sha256:local-config-id").as_deref(),
            Some("ghcr.io/org/agent-codex@sha256:verified")
        );
    }
}
