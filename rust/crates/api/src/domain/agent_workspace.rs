//! Agent workspace path contract.
//!
//! Container CLI agents mount a governed workspace root at `/workspace`.
//! The host path is workspace-scoped, not user-scoped or project-scoped, so
//! an agent can work across projects in the same shared workspace while
//! remaining inside the organization tenant boundary.

use std::path::{Component, Path as FsPath, PathBuf};

use agentforge_core::{AppResult, ErrorKind};
use uuid::Uuid;

pub const CONTAINER_WORKSPACE_ROOT: &str = "/workspace";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceMountScope {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkspacePaths {
    pub host_projects_root: PathBuf,
    pub container_cwd: String,
}

pub fn resolve_agent_workspace_paths(
    workspace_root: &str,
    scope: WorkspaceMountScope,
    requested_cwd: Option<&str>,
) -> AppResult<AgentWorkspacePaths> {
    let host_projects_root = workspace_projects_root(workspace_root, scope);
    let container_cwd = normalize_container_cwd(workspace_root, &host_projects_root, requested_cwd)?;
    Ok(AgentWorkspacePaths { host_projects_root, container_cwd })
}

pub fn host_path_for_container_cwd(host_projects_root: &FsPath, container_cwd: &str) -> AppResult<PathBuf> {
    if container_cwd == CONTAINER_WORKSPACE_ROOT {
        return Ok(host_projects_root.to_path_buf());
    }

    let Some(relative) = container_cwd.strip_prefix("/workspace/") else {
        return Err(ErrorKind::Validation("cwd must be under /workspace".into()).into());
    };
    safe_join_under(host_projects_root, relative)
}

fn workspace_projects_root(workspace_root: &str, scope: WorkspaceMountScope) -> PathBuf {
    FsPath::new(workspace_root.trim_end_matches('/'))
        .join("orgs")
        .join(scope.org_id.to_string())
        .join("workspaces")
        .join(scope.workspace_id.to_string())
        .join("projects")
}

fn normalize_container_cwd(
    workspace_root: &str,
    host_projects_root: &FsPath,
    requested_cwd: Option<&str>,
) -> AppResult<String> {
    let Some(raw) = requested_cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(CONTAINER_WORKSPACE_ROOT.to_string());
    };

    match raw {
        "." | "~" | "~/projects" | CONTAINER_WORKSPACE_ROOT => {
            return Ok(CONTAINER_WORKSPACE_ROOT.to_string());
        }
        _ => {}
    }

    if let Some(relative) = raw.strip_prefix("~/projects/").or_else(|| raw.strip_prefix("/workspace/")) {
        return container_cwd_from_relative(relative);
    }

    let requested = PathBuf::from(raw);
    if requested.is_absolute() {
        if contains_parent_or_prefix(&requested) {
            return Err(ErrorKind::Validation("cwd must not escape the managed workspace root".into()).into());
        }

        if let Ok(relative) = requested.strip_prefix(host_projects_root) {
            return container_cwd_from_relative_path(relative);
        }

        if let Some(relative) = legacy_user_projects_relative(workspace_root, &requested) {
            return container_cwd_from_relative_path(&relative);
        }

        return Err(ErrorKind::Validation(format!(
            "cwd must stay under managed workspace root {}",
            host_projects_root.display()
        ))
        .into());
    }

    if raw.starts_with("~/") {
        return Err(ErrorKind::Validation("cwd must be relative to ~/projects or /workspace".into()).into());
    }

    container_cwd_from_relative(raw)
}

fn container_cwd_from_relative(relative: &str) -> AppResult<String> {
    container_cwd_from_relative_path(FsPath::new(relative))
}

fn container_cwd_from_relative_path(relative_path: &FsPath) -> AppResult<String> {
    if relative_path.as_os_str().is_empty() || relative_path == FsPath::new(".") {
        return Ok(CONTAINER_WORKSPACE_ROOT.to_string());
    }
    let unsafe_component = relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)));
    if unsafe_component {
        return Err(ErrorKind::Validation("cwd must not escape the managed workspace root".into()).into());
    }
    Ok(format!("{CONTAINER_WORKSPACE_ROOT}/{}", relative_path.to_string_lossy().trim_start_matches('/')))
}

fn safe_join_under(root: &FsPath, relative: &str) -> AppResult<PathBuf> {
    let relative_path = FsPath::new(relative);
    let unsafe_component = relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)));
    if unsafe_component {
        return Err(ErrorKind::Validation("cwd must not escape the managed workspace root".into()).into());
    }
    Ok(root.join(relative_path))
}

fn contains_parent_or_prefix(path: &FsPath) -> bool {
    path.components().any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}

fn legacy_user_projects_relative(workspace_root: &str, requested: &FsPath) -> Option<PathBuf> {
    let root = FsPath::new(workspace_root.trim_end_matches('/'));
    let relative = requested.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let user_id = match components.next()? {
        Component::Normal(value) => value.to_str()?,
        _ => return None,
    };
    Uuid::parse_str(user_id).ok()?;
    match components.next()? {
        Component::Normal(value) if value == "projects" => Some(components.as_path().to_path_buf()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> WorkspaceMountScope {
        WorkspaceMountScope {
            org_id: Uuid::parse_str("aaaaaaaa-1111-2222-3333-444444444444").unwrap(),
            workspace_id: Uuid::parse_str("bbbbbbbb-1111-2222-3333-444444444444").unwrap(),
        }
    }

    #[test]
    fn workspace_path_defaults_to_org_workspace_projects_root() {
        let paths = resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), None).unwrap();
        assert_eq!(
            paths.host_projects_root.to_string_lossy(),
            "/data/agentforge/workspaces/orgs/aaaaaaaa-1111-2222-3333-444444444444/workspaces/bbbbbbbb-1111-2222-3333-444444444444/projects"
        );
        assert_eq!(paths.container_cwd, "/workspace");
    }

    #[test]
    fn workspace_path_keeps_mount_root_while_mapping_container_cwd() {
        let paths =
            resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("~/projects/agentforge"))
                .unwrap();
        assert_eq!(
            paths.host_projects_root.to_string_lossy(),
            "/data/agentforge/workspaces/orgs/aaaaaaaa-1111-2222-3333-444444444444/workspaces/bbbbbbbb-1111-2222-3333-444444444444/projects"
        );
        assert_eq!(paths.container_cwd, "/workspace/agentforge");

        let paths =
            resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("analysis_app")).unwrap();
        assert_eq!(paths.container_cwd, "/workspace/analysis_app");
    }

    #[test]
    fn workspace_path_maps_legacy_user_projects_paths_to_container_cwd() {
        let paths = resolve_agent_workspace_paths(
            "/data/agentforge/workspaces",
            scope(),
            Some("/data/agentforge/workspaces/11111111-2222-3333-4444-555555555555/projects/agentforge"),
        )
        .unwrap();
        assert_eq!(paths.container_cwd, "/workspace/agentforge");
    }

    #[test]
    fn workspace_path_rejects_escape_attempts() {
        assert!(resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("../secrets")).is_err());
        assert!(resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("/tmp")).is_err());
        assert!(
            resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("~/projects/../.ssh")).is_err()
        );
    }

    #[test]
    fn host_path_for_container_cwd_stays_under_mount_root() {
        let paths =
            resolve_agent_workspace_paths("/data/agentforge/workspaces", scope(), Some("/workspace/app")).unwrap();
        let host_path = host_path_for_container_cwd(&paths.host_projects_root, &paths.container_cwd).unwrap();
        assert!(host_path.ends_with("projects/app"));
        assert!(host_path_for_container_cwd(&paths.host_projects_root, "/tmp").is_err());
    }
}
