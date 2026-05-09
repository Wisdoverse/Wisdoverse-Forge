//! Security policy validation for container configurations.
//!
//! Enforces hard denials for privileged containers, dangerous capabilities,
//! forbidden mounts, and missing resource limits.

use crate::types::ContainerConfig;

/// A security policy violation detected during container config validation.
#[derive(Debug, thiserror::Error)]
pub enum SecurityViolation {
    #[error("privileged containers are not allowed")]
    Privileged,

    #[error("host PID namespace is not allowed")]
    HostPid,

    #[error("host network is not allowed")]
    HostNetwork,

    #[error("dangerous capability: {0}")]
    DangerousCapability(String),

    #[error("mount path not allowed: {0}")]
    ForbiddenMount(String),

    #[error("no resource limits configured")]
    NoResourceLimits,
}

/// Capabilities that are never allowed on agent containers.
const FORBIDDEN_CAPS: &[&str] = &["ALL", "SYS_ADMIN", "SYS_PTRACE", "NET_RAW"];

/// Host paths that must never be bind-mounted into containers.
const FORBIDDEN_MOUNT_PREFIXES: &[&str] = &["/var/run/docker.sock", "/etc/shadow", "/etc/passwd"];

/// Validate a container config against the security policy.
///
/// Returns `Ok(())` if the config is safe, or `Err(violations)` listing every
/// violation found (all violations are collected, not just the first).
pub fn validate_security(config: &ContainerConfig) -> Result<(), Vec<SecurityViolation>> {
    let mut violations = Vec::new();

    // Check for privileged mode.
    if config.privileged {
        violations.push(SecurityViolation::Privileged);
    }

    // Check for host PID namespace.
    if config.host_pid {
        violations.push(SecurityViolation::HostPid);
    }

    // Require at least one resource limit to prevent unbounded containers.
    if config.resources.memory_bytes.is_none() && config.resources.cpu_quota.is_none() {
        violations.push(SecurityViolation::NoResourceLimits);
    }

    // Check for host network mode.
    if let Some(ref net) = config.network
        && net == "host"
    {
        violations.push(SecurityViolation::HostNetwork);
    }

    // Check forbidden mounts.
    for mount in &config.mounts {
        for prefix in FORBIDDEN_MOUNT_PREFIXES {
            if mount.source.starts_with(prefix) {
                violations.push(SecurityViolation::ForbiddenMount(mount.source.clone()));
            }
        }
    }

    if violations.is_empty() { Ok(()) } else { Err(violations) }
}

/// Check whether a Linux capability string is forbidden.
pub fn is_forbidden_capability(cap: &str) -> bool {
    FORBIDDEN_CAPS.contains(&cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Mount, ResourceLimits};
    use std::collections::HashMap;

    fn valid_config() -> ContainerConfig {
        ContainerConfig {
            image: "agentforge/agent:latest".to_string(),
            name: Some("test-agent".to_string()),
            working_dir: None,
            env: vec!["FOO=bar".to_string()],
            labels: HashMap::new(),
            resources: ResourceLimits::default(),
            network: None,
            mounts: vec![],
            privileged: false,
            host_pid: false,
            tty: false,
            open_stdin: false,
            attach_stdin: false,
            attach_stdout: false,
            attach_stderr: false,
        }
    }

    #[test]
    fn accepts_valid_config() {
        assert!(validate_security(&valid_config()).is_ok());
    }

    #[test]
    fn rejects_no_resource_limits() {
        let mut cfg = valid_config();
        cfg.resources =
            ResourceLimits { cpu_quota: None, memory_bytes: None, memory_swap_bytes: None, pids_limit: None };
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::NoResourceLimits)));
    }

    #[test]
    fn allows_cpu_only_limits() {
        let mut cfg = valid_config();
        cfg.resources =
            ResourceLimits { cpu_quota: Some(100_000), memory_bytes: None, memory_swap_bytes: None, pids_limit: None };
        assert!(validate_security(&cfg).is_ok());
    }

    #[test]
    fn allows_memory_only_limits() {
        let mut cfg = valid_config();
        cfg.resources = ResourceLimits {
            cpu_quota: None,
            memory_bytes: Some(512 * 1024 * 1024),
            memory_swap_bytes: None,
            pids_limit: None,
        };
        assert!(validate_security(&cfg).is_ok());
    }

    #[test]
    fn rejects_forbidden_mounts() {
        let mut cfg = valid_config();
        cfg.mounts = vec![Mount {
            source: "/var/run/docker.sock".to_string(),
            target: "/var/run/docker.sock".to_string(),
            read_only: true,
        }];
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::ForbiddenMount(_))));
    }

    #[test]
    fn rejects_etc_shadow_mount() {
        let mut cfg = valid_config();
        cfg.mounts =
            vec![Mount { source: "/etc/shadow".to_string(), target: "/tmp/shadow".to_string(), read_only: true }];
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::ForbiddenMount(p) if p == "/etc/shadow")));
    }

    #[test]
    fn rejects_host_network() {
        let mut cfg = valid_config();
        cfg.network = Some("host".to_string());
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::HostNetwork)));
    }

    #[test]
    fn rejects_privileged_container() {
        let mut cfg = valid_config();
        cfg.privileged = true;
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::Privileged)));
    }

    #[test]
    fn rejects_host_pid() {
        let mut cfg = valid_config();
        cfg.host_pid = true;
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::HostPid)));
    }

    #[test]
    fn collects_multiple_violations() {
        let mut cfg = valid_config();
        cfg.resources =
            ResourceLimits { cpu_quota: None, memory_bytes: None, memory_swap_bytes: None, pids_limit: None };
        cfg.network = Some("host".to_string());
        cfg.mounts =
            vec![Mount { source: "/etc/passwd".to_string(), target: "/tmp/passwd".to_string(), read_only: true }];
        let err = validate_security(&cfg).unwrap_err();
        assert!(err.len() >= 3, "expected at least 3 violations, got {}", err.len());
    }

    #[test]
    fn forbidden_capabilities_detected() {
        assert!(is_forbidden_capability("ALL"));
        assert!(is_forbidden_capability("SYS_ADMIN"));
        assert!(is_forbidden_capability("SYS_PTRACE"));
        assert!(is_forbidden_capability("NET_RAW"));
        assert!(!is_forbidden_capability("NET_BIND_SERVICE"));
        assert!(!is_forbidden_capability("CHOWN"));
    }
}
