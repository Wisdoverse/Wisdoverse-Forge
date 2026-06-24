//! Compile-time enforced tenant isolation.
//!
//! [`TenantScope`] can only be constructed by auth middleware (fields are private).
//! Repository methods accept `&TenantScope` to ensure every query is scoped to
//! the authenticated organization and user.

use uuid::Uuid;

use crate::types::{OrgId, ProjectId, TeamId, UserId, WorkspaceId};

/// Tenant isolation scope — constructed by auth middleware only.
///
/// Repository methods require `&TenantScope` to execute queries, enforcing
/// at compile time that unauthenticated code paths cannot access tenant data.
#[derive(Debug, Clone)]
pub struct TenantScope {
    org_id: OrgId,
    user_id: UserId,
    workspace_id: Option<WorkspaceId>,
    team_id: Option<TeamId>,
    project_id: Option<ProjectId>,
}

impl TenantScope {
    /// Create a new tenant scope.
    ///
    /// # Safety contract
    /// **Only call from verified auth context** (auth middleware after JWT validation).
    /// This constructor is `pub` because the `auth` crate (a separate crate that depends
    /// on `core`) needs to construct `TenantScope` from validated tokens.
    /// A sealed-token pattern will replace this once the auth crate is fully implemented.
    #[doc(hidden)]
    pub fn new(org_id: OrgId, user_id: UserId) -> Self {
        Self { org_id, user_id, workspace_id: None, team_id: None, project_id: None }
    }

    /// Create a tenant scope with optional governance axes from verified auth context.
    ///
    /// # Safety contract
    /// **Only call from verified auth context** after the workspace/team/project axes have
    /// been taken from trusted claims or validated against membership state.
    #[doc(hidden)]
    pub fn with_axes(
        org_id: OrgId,
        user_id: UserId,
        workspace_id: Option<WorkspaceId>,
        team_id: Option<TeamId>,
        project_id: Option<ProjectId>,
    ) -> Self {
        Self { org_id, user_id, workspace_id, team_id, project_id }
    }

    /// The organization this request is scoped to.
    pub fn org_id(&self) -> OrgId {
        self.org_id
    }

    /// The authenticated user making this request.
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// The active workspace execution boundary, when the auth context carries one.
    pub fn workspace_id(&self) -> Option<WorkspaceId> {
        self.workspace_id
    }

    /// The active team sharing axis, when the auth context carries one.
    pub fn team_id(&self) -> Option<TeamId> {
        self.team_id
    }

    /// The active project sharing axis, when the auth context carries one.
    pub fn project_id(&self) -> Option<ProjectId> {
        self.project_id
    }

    /// Build a read-scope proof from the active axes on this request scope.
    pub fn scoped_read(&self) -> ScopedRead {
        ScopedRead::from_scope(self)
    }
}

/// Human sharing axis selected for a governance mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    User,
    Team,
    Project,
}

impl ScopeKind {
    /// Stable label for logs and metrics.
    pub fn as_label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Project => "project",
        }
    }
}

/// Validated read proof for governance queries.
///
/// Read paths need the full membership set because governance resolution uses
/// union queries across user, team, and project visibility while still staying
/// inside the organization and workspace execution boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedRead {
    org_id: OrgId,
    user_id: UserId,
    workspace_ids: Vec<WorkspaceId>,
    team_ids: Vec<TeamId>,
    project_ids: Vec<ProjectId>,
}

impl ScopedRead {
    /// Build a read proof from the axes carried on a request's tenant scope.
    pub fn from_scope(scope: &TenantScope) -> Self {
        let mut read = Self {
            org_id: scope.org_id,
            user_id: scope.user_id,
            workspace_ids: Vec::new(),
            team_ids: Vec::new(),
            project_ids: Vec::new(),
        };

        if let Some(workspace_id) = scope.workspace_id {
            push_unique(&mut read.workspace_ids, workspace_id);
        }
        if let Some(team_id) = scope.team_id {
            push_unique(&mut read.team_ids, team_id);
        }
        if let Some(project_id) = scope.project_id {
            push_unique(&mut read.project_ids, project_id);
        }

        read
    }

    /// Build a read proof from a membership set already validated by the caller.
    pub fn from_validated_memberships(
        org_id: OrgId,
        user_id: UserId,
        workspace_ids: impl IntoIterator<Item = WorkspaceId>,
        team_ids: impl IntoIterator<Item = TeamId>,
        project_ids: impl IntoIterator<Item = ProjectId>,
    ) -> Self {
        Self {
            org_id,
            user_id,
            workspace_ids: unique_vec(workspace_ids),
            team_ids: unique_vec(team_ids),
            project_ids: unique_vec(project_ids),
        }
    }

    /// Organization all memberships belong to.
    pub fn org_id(&self) -> OrgId {
        self.org_id
    }

    /// Authenticated user the proof was built for.
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Workspace IDs covered by this proof.
    pub fn workspace_ids(&self) -> &[WorkspaceId] {
        &self.workspace_ids
    }

    /// Team IDs covered by this proof.
    pub fn team_ids(&self) -> &[TeamId] {
        &self.team_ids
    }

    /// Project IDs covered by this proof.
    pub fn project_ids(&self) -> &[ProjectId] {
        &self.project_ids
    }

    /// Whether this proof covers a workspace execution boundary.
    pub fn contains_workspace(&self, workspace_id: WorkspaceId) -> bool {
        self.workspace_ids.contains(&workspace_id)
    }

    /// Whether this proof covers a team sharing axis.
    pub fn contains_team(&self, team_id: TeamId) -> bool {
        self.team_ids.contains(&team_id)
    }

    /// Whether this proof covers a project sharing axis.
    pub fn contains_project(&self, project_id: ProjectId) -> bool {
        self.project_ids.contains(&project_id)
    }
}

/// Write-scope construction failed because the selected mutation target was
/// not covered by the validated read proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScopedWriteError {
    #[error("user scope does not match authenticated user")]
    UserMismatch,
    #[error("team scope is not in authenticated membership set")]
    TeamNotInScope,
    #[error("project scope is not in authenticated membership set")]
    ProjectNotInScope,
}

impl ScopedWriteError {
    fn reason_label(self) -> &'static str {
        match self {
            Self::UserMismatch => "user_mismatch",
            Self::TeamNotInScope => "team_not_in_scope",
            Self::ProjectNotInScope => "project_not_in_scope",
        }
    }
}

/// Validated write proof for a governance mutation.
///
/// Mutation paths commit to exactly one sharing axis at construction time. The
/// constructor verifies that the selected `(kind, id)` is present in the caller's
/// `ScopedRead` proof, preventing silent fallback to a wider org-only scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedWrite {
    kind: ScopeKind,
    id: Uuid,
    scope_proof: ScopedRead,
}

impl ScopedWrite {
    /// Validate and create a committed write scope.
    pub fn try_new(kind: ScopeKind, id: Uuid, scope_proof: ScopedRead) -> Result<Self, ScopedWriteError> {
        let err = match kind {
            ScopeKind::User if scope_proof.user_id == UserId::from(id) => return Ok(Self { kind, id, scope_proof }),
            ScopeKind::User => ScopedWriteError::UserMismatch,
            ScopeKind::Team if scope_proof.contains_team(TeamId::from(id)) => {
                return Ok(Self { kind, id, scope_proof });
            }
            ScopeKind::Team => ScopedWriteError::TeamNotInScope,
            ScopeKind::Project if scope_proof.contains_project(ProjectId::from(id)) => {
                return Ok(Self { kind, id, scope_proof });
            }
            ScopeKind::Project => ScopedWriteError::ProjectNotInScope,
        };

        metrics::counter!(
            "agentforge_governance_scope_mismatch_total",
            "kind" => kind.as_label(),
            "reason" => err.reason_label()
        )
        .increment(1);
        Err(err)
    }

    /// Validate and create a user-scoped mutation proof.
    pub fn for_user(user_id: UserId, scope_proof: ScopedRead) -> Result<Self, ScopedWriteError> {
        Self::try_new(ScopeKind::User, user_id.as_uuid(), scope_proof)
    }

    /// Validate and create a team-scoped mutation proof.
    pub fn for_team(team_id: TeamId, scope_proof: ScopedRead) -> Result<Self, ScopedWriteError> {
        Self::try_new(ScopeKind::Team, team_id.as_uuid(), scope_proof)
    }

    /// Validate and create a project-scoped mutation proof.
    pub fn for_project(project_id: ProjectId, scope_proof: ScopedRead) -> Result<Self, ScopedWriteError> {
        Self::try_new(ScopeKind::Project, project_id.as_uuid(), scope_proof)
    }

    /// Sharing axis for the mutation.
    pub fn kind(&self) -> ScopeKind {
        self.kind
    }

    /// UUID for the committed sharing axis.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Read proof used to validate this mutation.
    pub fn scope_proof(&self) -> &ScopedRead {
        &self.scope_proof
    }
}

fn unique_vec<T: Copy + PartialEq>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut unique = Vec::new();
    for item in items {
        push_unique(&mut unique, item);
    }
    unique
}

fn push_unique<T: Copy + PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_accessors() {
        let org = OrgId::new();
        let user = UserId::new();
        let scope = TenantScope::new(org, user);

        assert_eq!(scope.org_id(), org);
        assert_eq!(scope.user_id(), user);
        assert_eq!(scope.workspace_id(), None);
        assert_eq!(scope.team_id(), None);
        assert_eq!(scope.project_id(), None);
    }

    #[test]
    fn clone_preserves_values() {
        let scope = TenantScope::new(OrgId::new(), UserId::new());
        let cloned = scope.clone();

        assert_eq!(scope.org_id(), cloned.org_id());
        assert_eq!(scope.user_id(), cloned.user_id());
    }

    #[test]
    fn construction_with_axes_sets_optional_scope_values() {
        let org = OrgId::new();
        let user = UserId::new();
        let workspace = WorkspaceId::new();
        let team = TeamId::new();
        let project = ProjectId::new();

        let scope = TenantScope::with_axes(org, user, Some(workspace), Some(team), Some(project));

        assert_eq!(scope.org_id(), org);
        assert_eq!(scope.user_id(), user);
        assert_eq!(scope.workspace_id(), Some(workspace));
        assert_eq!(scope.team_id(), Some(team));
        assert_eq!(scope.project_id(), Some(project));
    }

    #[test]
    fn scoped_read_from_scope_carries_present_axes_only() {
        let org = OrgId::new();
        let user = UserId::new();
        let workspace = WorkspaceId::new();
        let project = ProjectId::new();
        let scope = TenantScope::with_axes(org, user, Some(workspace), None, Some(project));

        let read = scope.scoped_read();

        assert_eq!(read.org_id(), org);
        assert_eq!(read.user_id(), user);
        assert_eq!(read.workspace_ids(), &[workspace]);
        assert!(read.team_ids().is_empty());
        assert_eq!(read.project_ids(), &[project]);
        assert!(read.contains_workspace(workspace));
        assert!(read.contains_project(project));
    }

    #[test]
    fn scoped_read_from_validated_memberships_deduplicates_axes() {
        let workspace = WorkspaceId::new();
        let team = TeamId::new();
        let project = ProjectId::new();

        let read = ScopedRead::from_validated_memberships(
            OrgId::new(),
            UserId::new(),
            [workspace, workspace],
            [team, team],
            [project, project],
        );

        assert_eq!(read.workspace_ids(), &[workspace]);
        assert_eq!(read.team_ids(), &[team]);
        assert_eq!(read.project_ids(), &[project]);
    }

    #[test]
    fn scoped_write_validates_selected_axis_against_read_proof() {
        let user = UserId::new();
        let team = TeamId::new();
        let project = ProjectId::new();
        let read = ScopedRead::from_validated_memberships(OrgId::new(), user, [WorkspaceId::new()], [team], [project]);

        let user_write = ScopedWrite::for_user(user, read.clone()).unwrap();
        assert_eq!(user_write.kind(), ScopeKind::User);
        assert_eq!(user_write.id(), user.as_uuid());

        let team_write = ScopedWrite::for_team(team, read.clone()).unwrap();
        assert_eq!(team_write.kind(), ScopeKind::Team);
        assert_eq!(team_write.id(), team.as_uuid());

        let project_write = ScopedWrite::for_project(project, read).unwrap();
        assert_eq!(project_write.kind(), ScopeKind::Project);
        assert_eq!(project_write.id(), project.as_uuid());
    }

    #[test]
    fn scoped_write_rejects_missing_or_mismatched_axes() {
        let read = ScopedRead::from_validated_memberships(
            OrgId::new(),
            UserId::new(),
            [WorkspaceId::new()],
            [TeamId::new()],
            [ProjectId::new()],
        );

        let user_err = ScopedWrite::for_user(UserId::new(), read.clone()).unwrap_err();
        assert_eq!(user_err, ScopedWriteError::UserMismatch);

        let team_err = ScopedWrite::for_team(TeamId::new(), read.clone()).unwrap_err();
        assert_eq!(team_err, ScopedWriteError::TeamNotInScope);

        let project_err = ScopedWrite::for_project(ProjectId::new(), read).unwrap_err();
        assert_eq!(project_err, ScopedWriteError::ProjectNotInScope);
    }
}
