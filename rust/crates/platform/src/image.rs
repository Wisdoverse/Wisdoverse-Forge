//! Image operations: pull, digest inspection (local + remote), re-tag.
//!
//! Used by the deployment-side CLI agent-image auto-updater to detect when a
//! Container CLI overlay (`agent-<tool>:latest`) has a newer manifest on the
//! registry and to refresh the local image the runtime spawns from. All four
//! operations are image-level only — they NEVER create a container, build a
//! `HostConfig`, or touch `security.rs`, so the container-creation
//! defense-in-depth is unaffected.

use std::collections::HashSet;

use bollard::auth::DockerCredentials;
use bollard::models::ImageInspect;
use bollard::query_parameters::{
    CreateImageOptionsBuilder, ListContainersOptionsBuilder, ListImagesOptionsBuilder, RemoveImageOptionsBuilder,
    TagImageOptionsBuilder,
};
use futures_util::StreamExt;

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
}
