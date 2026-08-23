//! Agent workspace persistence helpers.

use std::io;
use std::path::{Component, Path};

use agentforge_core::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::agent_workspace::{
    AgentWorkspacePaths, CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, host_path_for_container_cwd,
    resolve_agent_workspace_paths,
};
use crate::repositories::agent::AgentWorkspaceRepository;

const DEFAULT_WORKSPACE_ROOT: &str = "/data/agentforge/workspaces";

pub fn workspace_root_from_env() -> String {
    std::env::var("AGENTFORGE_WORKSPACE_ROOT").unwrap_or_else(|_| DEFAULT_WORKSPACE_ROOT.to_string())
}

#[cfg(unix)]
const SHARED_WORKSPACE_DIRECTORY_MODE: u32 = 0o2775;

/// Create a directory below the configured workspace root without following
/// symlinked path components. Server-managed routing and final directories are
/// normalized to the root's gid and an exact setgid, group-writable mode.
pub(crate) fn ensure_shared_workspace_directory(workspace_root: &Path, target: &Path) -> io::Result<()> {
    ensure_workspace_directory(workspace_root, target, false)
}

/// Prepare an agent working directory while preserving an existing project
/// directory, which may be owned by the agent container rather than the API.
pub(crate) fn ensure_agent_working_directory(workspace_root: &Path, target: &Path) -> io::Result<()> {
    ensure_workspace_directory(workspace_root, target, true)
}

#[cfg(unix)]
fn ensure_workspace_directory(workspace_root: &Path, target: &Path, preserve_existing: bool) -> io::Result<()> {
    use rustix::fs::{CWD, Gid, Mode, OFlags, fchmod, fchown, fstat, mkdirat, openat};

    let relative = target.strip_prefix(workspace_root).map_err(|_| {
        io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("workspace directory {} is outside {}", target.display(), workspace_root.display()),
        )
    })?;
    let mut parent =
        openat(CWD, workspace_root, OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?;
    let shared_gid = Gid::from_raw(fstat(&parent)?.st_gid);
    let mode = Mode::from_bits_truncate(SHARED_WORKSPACE_DIRECTORY_MODE);
    let mut components = relative.components().peekable();

    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("workspace directory contains an invalid component: {}", target.display()),
            ));
        };
        let created = match mkdirat(&parent, name, mode) {
            Ok(()) => true,
            Err(rustix::io::Errno::EXIST) => false,
            Err(err) => return Err(err.into()),
        };
        let child = openat(&parent, name, OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?;
        let is_final_component = components.peek().is_none();
        let is_preserved_target = preserve_existing && is_final_component && !created;
        match fchown(&child, None, Some(shared_gid)) {
            Ok(()) => {
                // fchown may clear setgid, so restore the exact mode afterwards.
                fchmod(&child, mode)?;
            }
            Err(rustix::io::Errno::PERM) if !created => {
                let stat = fstat(&child)?;
                let current_gid = Gid::from_raw(stat.st_gid);
                let safe_existing = if is_preserved_target {
                    can_preserve_agent_owned_directory(stat.st_mode, current_gid, shared_gid)
                } else if preserve_existing {
                    can_traverse_existing_agent_directory(stat.st_mode, current_gid, shared_gid)
                } else {
                    can_preserve_existing_shared_directory(stat.st_mode, current_gid, shared_gid)
                };
                if !safe_existing {
                    return Err(rustix::io::Errno::PERM.into());
                }
            }
            Err(err) => return Err(err.into()),
        }
        parent = child;
    }
    Ok(())
}

#[cfg(unix)]
fn can_preserve_agent_owned_directory(
    mode: rustix::fs::RawMode,
    gid: rustix::fs::Gid,
    shared_gid: rustix::fs::Gid,
) -> bool {
    gid == shared_gid && mode & 0o300 == 0o300 && mode & 0o002 == 0
}

#[cfg(unix)]
fn can_traverse_existing_agent_directory(
    mode: rustix::fs::RawMode,
    gid: rustix::fs::Gid,
    shared_gid: rustix::fs::Gid,
) -> bool {
    gid == shared_gid && mode & 0o010 == 0o010 && mode & 0o002 == 0
}

#[cfg(unix)]
fn can_preserve_existing_shared_directory(
    mode: rustix::fs::RawMode,
    gid: rustix::fs::Gid,
    shared_gid: rustix::fs::Gid,
) -> bool {
    gid == shared_gid && mode & 0o2030 == 0o2030 && mode & 0o002 == 0
}

#[cfg(not(unix))]
fn ensure_workspace_directory(workspace_root: &Path, target: &Path, preserve_existing: bool) -> io::Result<()> {
    target.strip_prefix(workspace_root).map_err(|_| {
        io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("workspace directory {} is outside {}", target.display(), workspace_root.display()),
        )
    })?;
    if preserve_existing && target.try_exists()? {
        return Ok(());
    }
    std::fs::create_dir_all(target)
}

#[derive(Clone)]
pub(crate) struct AgentWorkspaceService {
    repo: AgentWorkspaceRepository,
}

impl AgentWorkspaceService {
    pub(crate) fn new(repo: AgentWorkspaceRepository) -> Self {
        Self { repo }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(AgentWorkspaceRepository::new(pool))
    }

    pub(crate) async fn resolve_workspace_mount_scope(
        &self,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        project_id: Option<Uuid>,
    ) -> AppResult<WorkspaceMountScope> {
        if let Some(project_id) = project_id {
            let row = self.repo.project_workspace(org_id, project_id).await?;
            return WorkspaceMountScope::for_project(row.organization_id, workspace_id, row.workspace_id);
        }

        if let Some(workspace_id) = workspace_id {
            self.ensure_workspace_belongs_to_org(org_id, workspace_id).await?;
            return Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id));
        }

        let workspace_id = self.default_workspace_for_org(org_id).await?;
        Ok(WorkspaceMountScope::for_workspace(org_id, workspace_id))
    }

    pub(crate) async fn ensure_workspace_belongs_to_org(&self, org_id: Uuid, workspace_id: Uuid) -> AppResult<()> {
        self.repo.ensure_workspace_belongs_to_org(org_id, workspace_id).await
    }

    pub(crate) async fn default_workspace_for_org(&self, org_id: Uuid) -> AppResult<Uuid> {
        self.repo.default_workspace_for_org(org_id).await
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use super::*;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("agentforge-workspace-mode-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create temp workspace root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn shared_workspace_directories_are_setgid_group_writable_and_not_world_writable() {
        let root = TempRoot::new();
        let shared_gid = std::fs::metadata(root.path()).expect("stat workspace root").gid();
        let existing_route = root.path().join("orgs");
        std::fs::create_dir(&existing_route).expect("create legacy routing directory");
        std::fs::set_permissions(&existing_route, std::fs::Permissions::from_mode(0o750))
            .expect("set legacy routing mode");
        let target = root.path().join("orgs/org/workspaces/workspace/projects");

        ensure_shared_workspace_directory(root.path(), &target).expect("prepare shared workspace directory");

        for path in target.ancestors().take(5) {
            let metadata = std::fs::metadata(path).expect("stat created directory");
            let mode = metadata.permissions().mode();
            assert_eq!(metadata.gid(), shared_gid, "{} must use the workspace gid", path.display());
            assert_eq!(mode & 0o2770, 0o2770, "{} must be setgid and group writable", path.display());
            assert_eq!(mode & 0o002, 0, "{} must not be world writable", path.display());
        }
    }

    #[test]
    fn existing_server_owned_working_directory_is_normalized() {
        let root = TempRoot::new();
        let shared_gid = std::fs::metadata(root.path()).expect("stat workspace root").gid();
        let projects_root = root.path().join("orgs/org/workspaces/workspace/projects");
        let target = projects_root.join("existing-project");
        std::fs::create_dir_all(&target).expect("create existing project");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).expect("set legacy server mode");

        ensure_agent_working_directory(root.path(), &target).expect("normalize server-owned working directory");

        let metadata = std::fs::metadata(&target).expect("stat existing project");
        assert_eq!(metadata.gid(), shared_gid);
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o2775);
        for path in projects_root.ancestors().take(5) {
            let metadata = std::fs::metadata(path).expect("stat routing directory");
            assert_eq!(metadata.gid(), shared_gid, "{} must use the workspace gid", path.display());
            assert_eq!(metadata.permissions().mode() & 0o2770, 0o2770);
        }
    }

    #[test]
    fn only_agent_writable_non_world_writable_targets_may_survive_eperm() {
        use rustix::fs::Gid;

        let gid = Gid::from_raw(1012);

        assert!(can_preserve_agent_owned_directory(0o40750, gid, gid));
        assert!(!can_preserve_agent_owned_directory(0o40550, gid, gid));
        assert!(!can_preserve_agent_owned_directory(0o40752, gid, gid));
        assert!(!can_preserve_agent_owned_directory(0o40750, Gid::from_raw(101), gid));
    }

    #[test]
    fn safe_existing_shared_and_nested_agent_directories_may_survive_eperm() {
        use rustix::fs::Gid;

        let gid = Gid::from_raw(1012);

        assert!(can_preserve_existing_shared_directory(0o42775, gid, gid));
        assert!(!can_preserve_existing_shared_directory(0o40775, gid, gid));
        assert!(!can_preserve_existing_shared_directory(0o42755, gid, gid));
        assert!(!can_preserve_existing_shared_directory(0o42777, gid, gid));
        assert!(can_traverse_existing_agent_directory(0o40750, gid, gid));
        assert!(!can_traverse_existing_agent_directory(0o40740, gid, gid));
        assert!(!can_traverse_existing_agent_directory(0o40752, gid, gid));
    }

    #[test]
    fn workspace_directory_preparation_rejects_symlinked_components() {
        let root = TempRoot::new();
        let outside = TempRoot::new();
        symlink(outside.path(), root.path().join("orgs")).expect("plant symlinked component");
        let target = root.path().join("orgs/org/workspaces/workspace/projects");

        let error = ensure_shared_workspace_directory(root.path(), &target).expect_err("symlink must be rejected");

        assert!(error.raw_os_error().is_some());
        assert!(!outside.path().join("org").exists(), "must not create through the symlink");
    }
}
