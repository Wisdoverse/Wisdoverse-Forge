//! Dev environment lifecycle and container configuration policies.

use std::collections::BTreeMap;

use agentforge_core::{AppResult, ErrorKind};
use serde::Deserialize;

pub(crate) const MAX_NAME_LEN: usize = 100;
#[cfg(test)]
pub(crate) const VALID_STATUSES: &[&str] = &["stopped", "starting", "running", "error"];
pub(crate) const STARTING_STATUS: &str = "starting";
pub(crate) const RUNNING_STATUS: &str = "running";
pub(crate) const STOPPED_STATUS: &str = "stopped";
pub(crate) const ERROR_STATUS: &str = "error";
pub(crate) const DEFAULT_STOP_TIMEOUT_SECONDS: i64 = 30;

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
}
