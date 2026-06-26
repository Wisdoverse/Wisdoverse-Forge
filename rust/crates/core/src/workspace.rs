//! Workspace filesystem path resolution shared across crates.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// The host-side projects root that is bind-mounted as `/workspace` for an
/// agent: `<workspace_root>/orgs/<org_id>/workspaces/<workspace_id>/projects`.
///
/// Lives in `core` so the api workspace resolver (which WRITES into this tree,
/// e.g. materialized task images) and the jobs image-cleanup sweeper (which
/// REMOVES from it) compute the exact same path. If the two ever diverged, the
/// sweeper would target the wrong directory and silently clean nothing.
pub fn workspace_projects_root(workspace_root: &str, org_id: Uuid, workspace_id: Uuid) -> PathBuf {
    Path::new(workspace_root.trim_end_matches('/'))
        .join("orgs")
        .join(org_id.to_string())
        .join("workspaces")
        .join(workspace_id.to_string())
        .join("projects")
}

#[cfg(test)]
mod tests {
    use super::workspace_projects_root;
    use uuid::Uuid;

    #[test]
    fn joins_org_and_workspace_under_projects() {
        let org = Uuid::from_u128(1);
        let ws = Uuid::from_u128(2);
        let path = workspace_projects_root("/data/agentforge/workspaces", org, ws);
        assert_eq!(
            path,
            std::path::Path::new("/data/agentforge/workspaces")
                .join("orgs")
                .join(org.to_string())
                .join("workspaces")
                .join(ws.to_string())
                .join("projects")
        );
    }

    #[test]
    fn trims_a_trailing_slash_on_the_root() {
        let org = Uuid::from_u128(3);
        let ws = Uuid::from_u128(4);
        assert_eq!(
            workspace_projects_root("/root/", org, ws),
            workspace_projects_root("/root", org, ws),
            "a trailing slash on the workspace root must not change the result"
        );
    }
}
