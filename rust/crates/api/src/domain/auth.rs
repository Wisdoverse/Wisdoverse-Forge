//! Authentication and session-context domain rules.
//!
//! Owns context-switch policy: validated cross-axis selection (workspace,
//! team, project) and the public session response shape. Repository-level
//! membership checks live in `identity`, `workspace`, and `project` modules;
//! this module only encodes the rules that combine them.

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde::Serialize;
use uuid::Uuid;

/// Refresh-token lifetime issued by a context switch.
pub const SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Validated context axes for an auth context switch.
///
/// Encodes the cross-axis invariant that `project_id` cannot be set without a
/// `workspace_id`, because every project belongs to exactly one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchContextAxes {
    workspace_id: Option<Uuid>,
    team_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

impl SwitchContextAxes {
    pub fn new(workspace_id: Option<Uuid>, team_id: Option<Uuid>, project_id: Option<Uuid>) -> AppResult<Self> {
        if project_id.is_some() && workspace_id.is_none() {
            return Err(ErrorKind::Validation("workspaceId is required when projectId is selected".into()).into());
        }

        Ok(Self { workspace_id, team_id, project_id })
    }

    pub fn workspace_id(&self) -> Option<Uuid> {
        self.workspace_id
    }

    pub fn team_id(&self) -> Option<Uuid> {
        self.team_id
    }

    pub fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }

    pub fn project_workspace_pair(&self) -> Option<(Uuid, Uuid)> {
        match (self.project_id, self.workspace_id) {
            (Some(project_id), Some(workspace_id)) => Some((project_id, workspace_id)),
            _ => None,
        }
    }
}

/// Tokens minted by a successful context switch.
#[derive(Debug, Clone)]
pub struct SwitchContextResult {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_in: u64,
    pub refresh_expires_in: u64,
}

/// Frontend response body for `POST /api/v1/auth/switch-context`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchContextSuccessResponse {
    pub ok: bool,
    pub access_token: String,
    pub expires_in: u64,
}

impl SwitchContextSuccessResponse {
    pub fn ok(access_token: String, expires_in: u64) -> Self {
        Self { ok: true, access_token, expires_in }
    }
}

/// User-visible error and authorization policy for session context switches.
pub struct AuthContextSwitchPolicy;

impl AuthContextSwitchPolicy {
    pub fn missing_org_membership() -> AppError {
        ErrorKind::Forbidden.into()
    }

    pub fn ensure_workspace_in_org(exists_in_org: bool) -> AppResult<()> {
        Self::ensure_allowed(exists_in_org)
    }

    pub fn ensure_team_readable(can_read: bool) -> AppResult<()> {
        Self::ensure_allowed(can_read)
    }

    pub fn ensure_project_readable(can_read: bool) -> AppResult<()> {
        Self::ensure_allowed(can_read)
    }

    pub fn token_creation_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("context switch token creation failed: {err}")).into()
    }

    pub fn refresh_token_creation_failed(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("context switch refresh token creation failed: {err}")).into()
    }

    fn ensure_allowed(allowed: bool) -> AppResult<()> {
        if allowed { Ok(()) } else { Err(Self::missing_org_membership()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_workspace_team_and_project_selection() {
        let workspace_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let axes = SwitchContextAxes::new(Some(workspace_id), Some(team_id), Some(project_id)).unwrap();

        assert_eq!(axes.workspace_id(), Some(workspace_id));
        assert_eq!(axes.team_id(), Some(team_id));
        assert_eq!(axes.project_id(), Some(project_id));
        assert_eq!(axes.project_workspace_pair(), Some((project_id, workspace_id)));
    }

    #[test]
    fn require_workspace_for_project_selection() {
        let err = SwitchContextAxes::new(None, None, Some(Uuid::new_v4())).unwrap_err();

        assert!(
            matches!(err.kind, ErrorKind::Validation(message) if message == "workspaceId is required when projectId is selected")
        );
    }

    #[test]
    fn allow_org_only_and_workspace_only_selection() {
        let workspace_id = Uuid::new_v4();

        let org_only = SwitchContextAxes::new(None, None, None).unwrap();
        let workspace_only = SwitchContextAxes::new(Some(workspace_id), None, None).unwrap();

        assert_eq!(org_only.project_workspace_pair(), None);
        assert_eq!(workspace_only.workspace_id(), Some(workspace_id));
        assert_eq!(workspace_only.project_workspace_pair(), None);
    }

    #[test]
    fn switch_context_success_response_serialization() {
        let body = SwitchContextSuccessResponse::ok("new-access".into(), 900);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["accessToken"], "new-access");
        assert_eq!(json["expiresIn"], 900);
    }

    #[test]
    fn context_switch_policy_owns_authorization_and_token_errors() {
        assert!(AuthContextSwitchPolicy::ensure_workspace_in_org(true).is_ok());
        assert!(matches!(
            AuthContextSwitchPolicy::ensure_workspace_in_org(false).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
        assert!(matches!(AuthContextSwitchPolicy::ensure_team_readable(false).unwrap_err().kind, ErrorKind::Forbidden));
        assert!(matches!(
            AuthContextSwitchPolicy::ensure_project_readable(false).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
        assert!(matches!(
            AuthContextSwitchPolicy::token_creation_failed("bad").kind,
            ErrorKind::Internal(err) if err.to_string().contains("context switch token creation failed")
        ));
        assert!(matches!(
            AuthContextSwitchPolicy::refresh_token_creation_failed("bad").kind,
            ErrorKind::Internal(err) if err.to_string().contains("context switch refresh token creation failed")
        ));
    }
}
