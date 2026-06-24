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

/// Host directories that must never be bind-mounted into agent containers.
/// Checked against a lexically-normalized source (see [`normalize_mount_source`]),
/// so `.`/`..`/parent-dir tricks cannot bypass it. Both `/run` and `/var/run` are
/// listed because they are the same directory on systemd hosts via a symlink the
/// validator cannot resolve for a not-yet-created path.
const FORBIDDEN_MOUNT_ROOTS: &[&str] =
    &["/etc", "/proc", "/sys", "/dev", "/run", "/var/run", "/var/lib/docker", "/boot", "/root"];

/// Lexically normalize an absolute path: collapse `.`, `..`, and redundant
/// separators WITHOUT touching the filesystem (mount sources may not exist yet at
/// validation time). Returns `None` for a non-absolute source — those are Docker
/// named volumes, not host-path binds, and cannot traverse the host filesystem.
fn normalize_mount_source(source: &str) -> Option<String> {
    if !source.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for segment in source.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

/// Whether a fully-normalized absolute path is the docker socket or a sensitive host root.
fn is_forbidden_normalized(path: &str) -> bool {
    // The docker socket by basename, wherever it is mounted from.
    if path.rsplit('/').next() == Some("docker.sock") {
        return true;
    }
    // The whole host root.
    if path == "/" {
        return true;
    }
    // A sensitive host root, exactly or as a parent of the source.
    FORBIDDEN_MOUNT_ROOTS.iter().any(|root| path == *root || path.starts_with(&format!("{root}/")))
}

/// Whether a bind-mount source targets the docker socket or a sensitive host root.
///
/// Checks the lexically-normalized path first (handles `.`/`..` and not-yet-created
/// sources), then — because Docker resolves host symlinks at mount time — resolves
/// symlinks via `canonicalize` when the source exists and re-checks the real target,
/// so a benign-looking path that symlinks to a forbidden root cannot slip through.
fn is_forbidden_mount(source: &str) -> bool {
    let Some(norm) = normalize_mount_source(source) else {
        return false;
    };
    if is_forbidden_normalized(&norm) {
        return true;
    }
    if let Ok(canonical) = std::fs::canonicalize(source)
        && let Some(canon) = canonical.to_str()
        && is_forbidden_normalized(canon)
    {
        return true;
    }
    false
}

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

    // Check forbidden mounts (lexically canonicalized so `.`/`..`/parent-dir and
    // symlinked socket paths cannot bypass the denial of the docker socket and
    // sensitive host roots).
    for mount in &config.mounts {
        if is_forbidden_mount(&mount.source) {
            violations.push(SecurityViolation::ForbiddenMount(mount.source.clone()));
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

    fn mount_cfg(source: &str) -> ContainerConfig {
        let mut cfg = valid_config();
        cfg.mounts = vec![Mount { source: source.to_string(), target: "/mnt".to_string(), read_only: true }];
        cfg
    }

    #[test]
    fn rejects_docker_socket_via_noncanonical_path() {
        // The naive prefix check missed these; a canonicalizing check must not.
        for src in ["/var/run/./docker.sock", "/var/run/../run/docker.sock", "/run/docker.sock", "/var/run"] {
            let err = validate_security(&mount_cfg(src)).unwrap_err();
            assert!(
                err.iter().any(|v| matches!(v, SecurityViolation::ForbiddenMount(_))),
                "socket-bearing mount `{src}` must be rejected"
            );
        }
    }

    #[test]
    fn rejects_sensitive_host_roots() {
        for src in ["/", "/etc/cron.d", "/root/.ssh", "/proc/1/root", "/sys", "/dev/mem", "/var/lib/docker/x"] {
            let err = validate_security(&mount_cfg(src)).unwrap_err();
            assert!(
                err.iter().any(|v| matches!(v, SecurityViolation::ForbiddenMount(_))),
                "sensitive host mount `{src}` must be rejected"
            );
        }
    }

    #[test]
    fn allows_benign_scratch_and_workspace_mounts() {
        for src in ["/tmp/work", "/data/agentforge/workspaces/x", "/home/agent/project", "/srv/scratch"] {
            assert!(validate_security(&mount_cfg(src)).is_ok(), "benign mount `{src}` should be allowed");
        }
    }

    #[test]
    fn rejects_symlink_to_forbidden_root() {
        // Docker resolves host symlinks at mount time, so a benign-looking source that
        // is actually a symlink to a forbidden root must be rejected (codex review P1).
        use std::os::unix::fs::symlink;
        let link = std::env::temp_dir().join(format!("af-sec-symlink-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        symlink("/etc", &link).expect("create symlink to /etc");
        let result = validate_security(&mount_cfg(link.to_str().unwrap()));
        let _ = std::fs::remove_file(&link);
        let err = result.expect_err("symlink resolving to /etc must be rejected");
        assert!(err.iter().any(|v| matches!(v, SecurityViolation::ForbiddenMount(_))));
    }
}
