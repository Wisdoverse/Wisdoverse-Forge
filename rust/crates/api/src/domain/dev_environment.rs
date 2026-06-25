//! Dev environment lifecycle and container configuration policies.

use std::collections::BTreeMap;

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};

pub(crate) const MAX_NAME_LEN: usize = 100;
#[cfg(test)]
pub(crate) const VALID_STATUSES: &[&str] = &["stopped", "starting", "running", "error"];
pub(crate) const STARTING_STATUS: &str = "starting";
pub(crate) const RUNNING_STATUS: &str = "running";
pub(crate) const STOPPED_STATUS: &str = "stopped";
pub(crate) const ERROR_STATUS: &str = "error";
pub(crate) const DEFAULT_STOP_TIMEOUT_SECONDS: i64 = 30;

pub(crate) fn dev_environment_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn dev_environment_message_response<T: Serialize>(data: T, message: &'static str) -> Value {
    json!({ "ok": true, "data": data, "message": message })
}

pub(crate) fn dev_environment_delete_response() -> Value {
    json!({ "ok": true })
}

pub(crate) struct DevEnvironmentRepositoryPolicy;

impl DevEnvironmentRepositoryPolicy {
    pub(crate) fn dev_environment_not_found(id: uuid::Uuid) -> AppError {
        ErrorKind::NotFound(format!("dev_environment {id}")).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DevEnvironmentName<'a> {
    value: &'a str,
}

impl<'a> DevEnvironmentName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if value.is_empty() || value.len() > MAX_NAME_LEN {
            return Err(ErrorKind::Validation(format!("name must be 1-{MAX_NAME_LEN} characters")).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopPlan<'a> {
    MarkStopped,
    StopContainer(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DevEnvironmentRuntimeState {
    Running,
    Stopped,
    Dead,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DevEnvironmentStatusUpdate {
    Running,
    Stopped,
}

pub(crate) struct DevEnvironmentLifecyclePolicy;

impl DevEnvironmentLifecyclePolicy {
    pub(crate) fn ensure_can_start(status: &str, container_id: Option<&str>) -> AppResult<()> {
        if status == RUNNING_STATUS || status == STARTING_STATUS {
            return Err(ErrorKind::Validation(format!("environment is already {status}")).into());
        }
        if let Some(existing_container_id) = container_id {
            return Err(ErrorKind::Validation(format!(
                "environment already has container {existing_container_id}; stop it before starting"
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn stop_plan<'a>(status: &str, container_id: Option<&'a str>) -> AppResult<StopPlan<'a>> {
        if status == STOPPED_STATUS && container_id.is_none() {
            return Err(ErrorKind::Validation("environment is already stopped".into()).into());
        }

        Ok(match container_id {
            Some(container_id) => StopPlan::StopContainer(container_id),
            None => StopPlan::MarkStopped,
        })
    }

    pub(crate) fn ensure_can_delete(status: &str) -> AppResult<()> {
        if status == RUNNING_STATUS || status == STARTING_STATUS {
            return Err(ErrorKind::Validation("stop the environment before deleting".into()).into());
        }
        Ok(())
    }

    pub(crate) fn reconcile_runtime_status(
        current_status: &str,
        runtime_state: DevEnvironmentRuntimeState,
    ) -> Option<DevEnvironmentStatusUpdate> {
        match runtime_state {
            DevEnvironmentRuntimeState::Running if current_status != RUNNING_STATUS => {
                Some(DevEnvironmentStatusUpdate::Running)
            }
            DevEnvironmentRuntimeState::Stopped | DevEnvironmentRuntimeState::Dead => {
                Some(DevEnvironmentStatusUpdate::Stopped)
            }
            DevEnvironmentRuntimeState::Running | DevEnvironmentRuntimeState::Other => None,
        }
    }
}

pub(crate) struct DevEnvironmentRuntimePolicy;

impl DevEnvironmentRuntimePolicy {
    pub(crate) fn docker_unavailable() -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("Docker runtime not available for dev environments")).into()
    }

    pub(crate) fn create_container_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to create dev environment container: {err}")).into()
    }

    pub(crate) fn start_container_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to start dev environment container: {err}")).into()
    }

    pub(crate) fn stop_container_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to stop dev environment container: {err}")).into()
    }

    pub(crate) fn remove_container_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to remove dev environment container: {err}")).into()
    }

    pub(crate) fn inspect_container_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to inspect dev environment container: {err}")).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DevEnvironmentRuntimeSpec {
    pub(crate) image: String,
    pub(crate) env: Vec<String>,
    pub(crate) mounts: Vec<DevEnvironmentMountSpec>,
    pub(crate) network: Option<String>,
    pub(crate) resources: DevEnvironmentResourceSpec,
}

impl DevEnvironmentRuntimeSpec {
    pub(crate) fn parse(config: &serde_json::Value) -> AppResult<Self> {
        let raw: RawDevEnvironmentContainerConfig = serde_json::from_value(config.clone())
            .map_err(|err| ErrorKind::Validation(format!("invalid dev environment config: {err}")))?;
        let image = raw
            .image
            .map(|image| image.trim().to_string())
            .filter(|image| !image.is_empty())
            .ok_or_else(|| ErrorKind::Validation("config.image is required to start a dev environment".into()))?;

        Ok(Self {
            image,
            env: raw.env.map(env_config_to_vec).transpose()?.unwrap_or_default(),
            mounts: raw
                .mounts
                .into_iter()
                .map(|mount| DevEnvironmentMountSpec {
                    source: mount.source,
                    target: mount.target,
                    read_only: mount.read_only,
                })
                .collect(),
            network: raw.network,
            resources: raw.resources.unwrap_or_default(),
        })
    }
}

/// Image-source allowlist policy for dev-environment containers (F018).
///
/// The dev-environment runs an operator-supplied image on the host Docker
/// daemon, so an unrestricted reference is a supply-chain / arbitrary-image RCE
/// vector (and the pull can reach internal registries, SSRF-style). The policy
/// is lenient-by-default but closed: official Docker Hub **library** images
/// (e.g. `ubuntu:22.04`, `library/alpine`) and the managed `agentforge-agent`
/// images are always allowed; operators widen it with
/// `DEV_ENV_ALLOWED_IMAGE_REGISTRIES` (a list of reference prefixes such as
/// `ghcr.io/myorg/` or `docker.io/`). Anything else — a namespaced Docker Hub
/// image, or any other registry host — is rejected, so a tenant cannot pull
/// `evil.example/malware` or reach `internal-registry:5000/...`.
pub(crate) struct DevEnvironmentImagePolicy;

impl DevEnvironmentImagePolicy {
    const MANAGED_IMAGE_REPO: &'static str = "agentforge-agent";

    pub(crate) fn ensure_image_allowed(image: &str, configured_prefixes: &[String]) -> AppResult<()> {
        let image = image.trim();
        if image.is_empty() {
            return Err(ErrorKind::Validation("config.image is required to start a dev environment".into()).into());
        }
        // Reject whitespace / control chars: the reference is passed to the
        // daemon's image-pull, and this also forecloses argv/cmdline injection.
        if image.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ErrorKind::Validation(
                "image reference must not contain whitespace or control characters".into(),
            )
            .into());
        }
        // Explicit operator opt-in: a configured reference prefix, matched at a
        // component boundary so `ghcr.io` does not also admit `ghcr.io.evil/...`
        // and `ghcr.io/myorg` does not admit `ghcr.io/myorg-malware/...`.
        if configured_prefixes.iter().any(|prefix| Self::prefix_matches(image, prefix)) {
            return Ok(());
        }
        // Split off the registry component per Docker's rule (the first path
        // segment is a registry IFF it contains `.`/`:` or is `localhost`). The
        // managed and official-library cases require NO registry — otherwise a ref
        // like `agentforge-agent:5000/malware` (registry host `agentforge-agent:5000`)
        // would masquerade as the managed image and pull from an arbitrary host.
        if let Some(path) = Self::docker_hub_path(image) {
            let repo = Self::repository_of(path);
            // Managed local agent image (`agentforge-agent[:tag]`).
            if repo == Self::MANAGED_IMAGE_REPO {
                return Ok(());
            }
            // Official Docker Hub library image: bare `name` (-> library/name) or
            // an explicit single-level `library/name`. The library namespace is
            // curated by Docker, so it cannot host a tenant's malicious image.
            if !repo.contains('/') || repo.strip_prefix("library/").is_some_and(|rest| !rest.contains('/')) {
                return Ok(());
            }
        }
        Err(ErrorKind::Validation(format!(
            "image '{image}' is not from an allowed source; use an official Docker Hub library image \
             (e.g. ubuntu:22.04), a managed agentforge-agent image, or a registry prefix configured in \
             DEV_ENV_ALLOWED_IMAGE_REGISTRIES"
        ))
        .into())
    }

    /// Return the Docker Hub repository path of a reference, or `None` if it
    /// targets a non-Hub registry. A canonical Docker Hub registry host
    /// (`docker.io`, `index.docker.io`, `registry-1.docker.io`) is stripped so
    /// `docker.io/library/ubuntu` is recognized as the same official image as the
    /// bare `ubuntu`. Otherwise Docker treats the first `/`-segment as a registry
    /// when it contains `.`/`:` or equals `localhost`.
    fn docker_hub_path(image: &str) -> Option<&str> {
        match image.split_once('/') {
            Some(("docker.io" | "index.docker.io" | "registry-1.docker.io", rest)) => Some(rest),
            Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => None,
            _ => Some(image),
        }
    }

    /// Strip an optional `@digest` then `:tag` from a Docker Hub reference path,
    /// leaving the bare repository (e.g. `library/ubuntu`, `agentforge-agent`).
    fn repository_of(path: &str) -> &str {
        let without_digest = path.split('@').next().unwrap_or(path);
        // A `:` only follows the repository as a tag separator here (the registry,
        // which is the only other `:` source, was already excluded).
        match without_digest.rsplit_once(':') {
            Some((repo, _tag)) => repo,
            None => without_digest,
        }
    }

    /// Does an operator-configured `prefix` match `image` at a component boundary?
    /// A bare prefix (no trailing `/`) matches only when the next character in the
    /// image is a path/tag boundary (`/` or `:`) or the image ends there — so
    /// `ghcr.io` matches `ghcr.io/x` but not `ghcr.io.evil/x`, and `ghcr.io/myorg`
    /// matches `ghcr.io/myorg/x` but not `ghcr.io/myorg-malware/x`. A prefix that
    /// already ends with `/` is a plain path-prefix and is taken as-is.
    fn prefix_matches(image: &str, prefix: &str) -> bool {
        if prefix.is_empty() {
            return false;
        }
        let Some(rest) = image.strip_prefix(prefix) else {
            return false;
        };
        prefix.ends_with('/') || rest.is_empty() || rest.starts_with('/') || rest.starts_with(':')
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DevEnvironmentMountSpec {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) read_only: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct DevEnvironmentResourceSpec {
    pub(crate) cpu_quota: Option<i64>,
    pub(crate) memory_bytes: Option<i64>,
    pub(crate) memory_swap_bytes: Option<i64>,
    pub(crate) pids_limit: Option<i64>,
}

#[derive(Deserialize)]
struct RawDevEnvironmentContainerConfig {
    image: Option<String>,
    #[serde(default)]
    env: Option<EnvConfig>,
    #[serde(default)]
    mounts: Vec<MountConfig>,
    network: Option<String>,
    resources: Option<DevEnvironmentResourceSpec>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EnvConfig {
    Map(BTreeMap<String, String>),
    List(Vec<String>),
}

#[derive(Deserialize)]
struct MountConfig {
    source: String,
    target: String,
    #[serde(default)]
    read_only: bool,
}

fn env_config_to_vec(config: EnvConfig) -> AppResult<Vec<String>> {
    match config {
        EnvConfig::Map(values) => values
            .into_iter()
            .map(|(key, value)| {
                validate_env_key(&key)?;
                Ok(format!("{key}={value}"))
            })
            .collect(),
        EnvConfig::List(values) => {
            for entry in &values {
                let key = entry.split_once('=').map(|(key, _)| key).unwrap_or(entry);
                validate_env_key(key)?;
                if !entry.contains('=') {
                    return Err(ErrorKind::Validation(format!(
                        "environment entry `{entry}` must use KEY=VALUE format"
                    ))
                    .into());
                }
            }
            Ok(values)
        }
    }
}

fn validate_env_key(key: &str) -> AppResult<()> {
    if key.is_empty() || key.contains('=') {
        return Err(ErrorKind::Validation(format!("invalid environment variable name `{key}`")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn image_policy_allows_official_library_and_managed_images() {
        let none: &[String] = &[];
        // Official Docker Hub library images (curated namespace), including the
        // canonical registry-qualified forms.
        for ok in [
            "ubuntu:22.04",
            "alpine:3.19",
            "python:3.12-slim",
            "library/ubuntu:22.04",
            "debian",
            "docker.io/library/ubuntu:22.04",
            "index.docker.io/library/alpine",
            "docker.io/ubuntu",
        ] {
            assert!(DevEnvironmentImagePolicy::ensure_image_allowed(ok, none).is_ok(), "{ok} should be allowed");
        }
        // A canonical Docker Hub *namespaced* (non-library) image is still rejected.
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("docker.io/someuser/img", none).is_err());
        // Managed local agent images.
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("agentforge-agent", none).is_ok());
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("agentforge-agent:codex", none).is_ok());
    }

    #[test]
    fn image_policy_blocks_untrusted_registries_and_namespaces_by_default() {
        let none: &[String] = &[];
        for bad in [
            "evil.example/malware:latest",   // arbitrary external registry
            "ghcr.io/someorg/img:1",         // other registry host
            "registry.internal:5000/x",      // internal registry (SSRF-style)
            "someuser/customimage:tag",      // non-library docker.io namespace
            "127.0.0.1:5000/x",              // host-local registry
            "agentforge-agent:5000/malware", // registry `agentforge-agent:5000`, NOT the managed image
            "library/ns/extra",              // multi-level under library
            "",                              // empty
            "ubuntu 22.04",                  // whitespace
        ] {
            assert!(
                matches!(
                    DevEnvironmentImagePolicy::ensure_image_allowed(bad, none).unwrap_err().kind,
                    ErrorKind::Validation(_)
                ),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn image_policy_honors_configured_prefixes() {
        let allowed = vec!["ghcr.io/myorg/".to_string(), "docker.io/".to_string()];
        // Now permitted because they match a configured prefix.
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io/myorg/runner:1", &allowed).is_ok());
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("docker.io/library/ubuntu", &allowed).is_ok());
        // A registry NOT in the allowlist is still rejected.
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io/otherorg/x", &allowed).is_err());
    }

    #[test]
    fn image_policy_prefix_match_respects_component_boundaries() {
        // Bare-host and bare-namespace prefixes (no trailing `/`) must only match
        // at a `/` or `:` boundary, so look-alike registries/namespaces are blocked.
        let host = vec!["ghcr.io".to_string()];
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io/org/img:1", &host).is_ok());
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io.evil/img", &host).is_err());

        let ns = vec!["ghcr.io/myorg".to_string()];
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io/myorg/img:1", &ns).is_ok());
        assert!(DevEnvironmentImagePolicy::ensure_image_allowed("ghcr.io/myorg-malware/img", &ns).is_err());
    }

    #[test]
    fn valid_status_list_matches_persisted_contract() {
        assert_eq!(VALID_STATUSES, ["stopped", "starting", "running", "error"]);
    }

    #[test]
    fn name_validation_preserves_current_length_contract() {
        assert_eq!(DevEnvironmentName::parse("dev-env").unwrap().value(), "dev-env");
        assert!(DevEnvironmentName::parse("").is_err());
        assert!(DevEnvironmentName::parse(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn repository_policy_owns_lookup_error() {
        let id = uuid::Uuid::new_v4();

        assert!(matches!(
            DevEnvironmentRepositoryPolicy::dev_environment_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("dev_environment {id}")
        ));
    }

    #[test]
    fn start_policy_rejects_active_or_leaked_container_state() {
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_start("stopped", None).is_ok());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_start("running", None).is_err());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_start("starting", None).is_err());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_start("error", Some("ctr-old")).is_err());
    }

    #[test]
    fn stop_policy_distinguishes_marker_update_from_runtime_teardown() {
        assert_eq!(DevEnvironmentLifecyclePolicy::stop_plan("error", None).unwrap(), StopPlan::MarkStopped);
        assert_eq!(
            DevEnvironmentLifecyclePolicy::stop_plan("running", Some("ctr-dev")).unwrap(),
            StopPlan::StopContainer("ctr-dev")
        );
        assert!(DevEnvironmentLifecyclePolicy::stop_plan("stopped", None).is_err());
    }

    #[test]
    fn delete_policy_rejects_active_states() {
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_delete("stopped").is_ok());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_delete("error").is_ok());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_delete("running").is_err());
        assert!(DevEnvironmentLifecyclePolicy::ensure_can_delete("starting").is_err());
    }

    #[test]
    fn runtime_reconciliation_maps_container_state_to_persisted_status() {
        assert_eq!(
            DevEnvironmentLifecyclePolicy::reconcile_runtime_status("starting", DevEnvironmentRuntimeState::Running),
            Some(DevEnvironmentStatusUpdate::Running)
        );
        assert_eq!(
            DevEnvironmentLifecyclePolicy::reconcile_runtime_status("running", DevEnvironmentRuntimeState::Dead),
            Some(DevEnvironmentStatusUpdate::Stopped)
        );
        assert_eq!(
            DevEnvironmentLifecyclePolicy::reconcile_runtime_status("running", DevEnvironmentRuntimeState::Running),
            None
        );
    }

    #[test]
    fn runtime_config_accepts_structured_env_mounts_and_resources() {
        let spec = DevEnvironmentRuntimeSpec::parse(&json!({
            "image": " ubuntu:22.04 ",
            "env": {"A": "one", "B": "two"},
            "mounts": [{"source": "/tmp/work", "target": "/workspace", "read_only": true}],
            "network": "agentforge-dev",
            "resources": {"memory_bytes": 268435456}
        }))
        .unwrap();

        assert_eq!(spec.image, "ubuntu:22.04");
        assert!(spec.env.contains(&"A=one".to_string()));
        assert_eq!(spec.mounts[0].target, "/workspace");
        assert!(spec.mounts[0].read_only);
        assert_eq!(spec.network.as_deref(), Some("agentforge-dev"));
        assert_eq!(spec.resources.memory_bytes, Some(268435456));
    }

    #[test]
    fn runtime_config_requires_image() {
        let err = DevEnvironmentRuntimeSpec::parse(&json!({"env": ["A=one"]})).unwrap_err();

        match err.kind {
            ErrorKind::Validation(message) => assert!(message.contains("config.image is required")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn runtime_config_rejects_invalid_env_entries() {
        let err = DevEnvironmentRuntimeSpec::parse(&json!({"image": "ubuntu:22.04", "env": ["BROKEN"]})).unwrap_err();

        match err.kind {
            ErrorKind::Validation(message) => assert!(message.contains("KEY=VALUE")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn runtime_policy_owns_docker_error_contracts() {
        for (err, expected) in [
            (DevEnvironmentRuntimePolicy::docker_unavailable(), "Docker runtime not available"),
            (DevEnvironmentRuntimePolicy::create_container_failed("bad"), "failed to create dev environment container"),
            (DevEnvironmentRuntimePolicy::start_container_failed("bad"), "failed to start dev environment container"),
            (DevEnvironmentRuntimePolicy::stop_container_failed("bad"), "failed to stop dev environment container"),
            (DevEnvironmentRuntimePolicy::remove_container_failed("bad"), "failed to remove dev environment container"),
            (
                DevEnvironmentRuntimePolicy::inspect_container_failed("bad"),
                "failed to inspect dev environment container",
            ),
        ] {
            match err.kind {
                ErrorKind::Internal(message) => assert!(message.to_string().contains(expected)),
                other => panic!("expected internal error, got {other:?}"),
            }
        }
    }
}
