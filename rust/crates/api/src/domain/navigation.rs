//! Frontend navigation response contract.
//!
//! The route surface is historical, but these projections are the active
//! tree-pane contract. Keep serialization here so routes do not construct
//! response shapes directly.

use serde::Serialize;
use uuid::Uuid;

use crate::domain::project_clone::CloneSummary;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyOrg {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) plan: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyTeam {
    pub(crate) id: Uuid,
    pub(crate) org_id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) visibility: String,
    pub(crate) description: String,
    pub(crate) can_manage: bool,
    pub(crate) can_delete: bool,
    pub(crate) can_create_project: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyProject {
    pub(crate) id: Uuid,
    pub(crate) team_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) color: String,
    pub(crate) description: String,
    pub(crate) can_manage: bool,
    pub(crate) can_delete: bool,
    /// Denormalized clone lifecycle marker mirrored from `projects.clone_status`
    /// (`none`/`queued`/`cloning`/`ready`/`failed`), for fast badge rendering in
    /// the tree pane without a per-project attempt read.
    pub(crate) clone_status: String,
    /// The latest clone attempt's detail (M6) — `None` when the project has no
    /// attempt (`clone_status='none'`). The service attaches this after listing;
    /// the `From<LegacyProjectRow>` adapter defaults it to `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clone: Option<CloneSummary>,
}

pub(crate) fn legacy_orgs_response(orgs: Vec<LegacyOrg>) -> serde_json::Value {
    serde_json::json!({ "ok": true, "orgs": orgs })
}

pub(crate) fn legacy_org_response(org: LegacyOrg) -> serde_json::Value {
    serde_json::json!({ "ok": true, "org": org })
}

pub(crate) fn legacy_teams_response(teams: Vec<LegacyTeam>) -> serde_json::Value {
    serde_json::json!({ "ok": true, "teams": teams })
}

pub(crate) fn legacy_team_response(team: LegacyTeam) -> serde_json::Value {
    serde_json::json!({ "ok": true, "team": team })
}

pub(crate) fn legacy_projects_response(projects: Vec<LegacyProject>) -> serde_json::Value {
    serde_json::json!({ "ok": true, "projects": projects })
}

pub(crate) fn legacy_project_response(project: LegacyProject) -> serde_json::Value {
    serde_json::json!({ "ok": true, "project": project })
}

pub(crate) fn legacy_delete_response() -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_org_serializes_old_frontend_shape() {
        let value = serde_json::to_value(LegacyOrg {
            id: Uuid::nil(),
            name: "Test Org".to_string(),
            slug: "test-org".to_string(),
            plan: "pro".to_string(),
            role: "owner".to_string(),
        })
        .unwrap();

        assert_eq!(value["name"], "Test Org");
        assert_eq!(value["plan"], "pro");
        assert_eq!(value["role"], "owner");
    }

    #[test]
    fn legacy_team_uses_camel_case_org_id() {
        let value = serde_json::to_value(LegacyTeam {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            name: "Engineering".to_string(),
            slug: "engineering".to_string(),
            visibility: "private".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
            can_create_project: true,
        })
        .unwrap();

        assert!(value.get("orgId").is_some());
        assert_eq!(value["slug"], "engineering");
        assert_eq!(value["canManage"], true);
        assert_eq!(value["canDelete"], true);
        assert_eq!(value["canCreateProject"], true);
    }

    #[test]
    fn legacy_project_uses_camel_case_team_id() {
        let value = serde_json::to_value(LegacyProject {
            id: Uuid::nil(),
            team_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            name: "Wisdoverse Forge".to_string(),
            slug: "agentforge".to_string(),
            color: "#007AFF".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
            clone_status: "none".to_string(),
            clone: None,
        })
        .unwrap();

        assert!(value.get("teamId").is_some());
        assert!(value.get("workspaceId").is_some());
        assert_eq!(value["color"], "#007AFF");
        assert_eq!(value["canManage"], true);
        assert_eq!(value["canDelete"], true);
        assert_eq!(value["cloneStatus"], "none");
        // `clone` is absent (skip_serializing_if) when there is no attempt.
        assert!(value.get("clone").is_none());
    }

    #[test]
    fn legacy_project_serializes_clone_summary_when_present() {
        let value = serde_json::to_value(LegacyProject {
            id: Uuid::nil(),
            team_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            name: "Cloned".to_string(),
            slug: "cloned".to_string(),
            color: "#007AFF".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
            clone_status: "ready".to_string(),
            clone: Some(CloneSummary {
                status: "ready".to_string(),
                error_class: None,
                error_message: None,
                resolved_branch: Some("main".to_string()),
                head_sha: Some("abc123".to_string()),
                attempt: 1,
                updated_at: chrono::Utc::now(),
            }),
        })
        .unwrap();

        assert_eq!(value["cloneStatus"], "ready");
        assert_eq!(value["clone"]["status"], "ready");
        assert_eq!(value["clone"]["resolvedBranch"], "main");
        assert_eq!(value["clone"]["headSha"], "abc123");
    }

    #[test]
    fn legacy_response_helpers_preserve_top_level_keys() {
        assert_eq!(legacy_orgs_response(Vec::new())["orgs"], serde_json::json!([]));
        assert_eq!(legacy_teams_response(Vec::new())["teams"], serde_json::json!([]));
        assert_eq!(legacy_projects_response(Vec::new())["projects"], serde_json::json!([]));
        assert_eq!(legacy_delete_response()["ok"], true);
    }
}
