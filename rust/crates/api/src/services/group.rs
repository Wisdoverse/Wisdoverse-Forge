//! Group service — business logic and validation for groups.

use agentforge_core::{AppResult, ErrorKind, GroupId, ProjectId, TenantScope};
use agentforge_db::entities::{Group, GroupMember};
use uuid::Uuid;

use crate::repositories::group::GroupRepository;

/// Business logic layer for group operations.
pub struct GroupService {
    repo: GroupRepository,
}

impl GroupService {
    pub fn new(repo: GroupRepository) -> Self {
        Self { repo }
    }

    /// List groups with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Group>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Get a single group by ID.
    pub async fn get(&self, scope: &TenantScope, id: GroupId) -> AppResult<Group> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new group with validated name.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        description: Option<&str>,
        project_id: Option<ProjectId>,
    ) -> AppResult<Group> {
        Self::validate_name(name)?;
        self.repo.create(scope, name, description, project_id).await
    }

    /// Return the project default group, creating it when the project has none.
    pub async fn find_or_create_default_for_project(
        &self,
        scope: &TenantScope,
        project_id: ProjectId,
    ) -> AppResult<Group> {
        self.repo.find_or_create_default_for_project(scope, project_id).await
    }

    /// Update a group's name and/or description.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: GroupId,
        name: Option<&str>,
        description: Option<&str>,
    ) -> AppResult<Group> {
        if let Some(n) = name {
            Self::validate_name(n)?;
        }
        self.repo.update(scope, id, name, description).await
    }

    /// Soft-delete a group.
    pub async fn delete(&self, scope: &TenantScope, id: GroupId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// List members of a group.
    pub async fn list_members(&self, scope: &TenantScope, group_id: GroupId) -> AppResult<Vec<GroupMember>> {
        self.repo.list_members(scope, group_id).await
    }

    /// Add a member to a group with validated role.
    pub async fn add_member(
        &self,
        scope: &TenantScope,
        group_id: GroupId,
        user_id: Uuid,
        role: &str,
    ) -> AppResult<GroupMember> {
        Self::validate_role(role)?;
        self.repo.add_member(scope, group_id, user_id, role).await
    }

    /// Remove a member from a group.
    pub async fn remove_member(&self, scope: &TenantScope, group_id: GroupId, user_id: Uuid) -> AppResult<()> {
        self.repo.remove_member(scope, group_id, user_id).await
    }

    /// Validate group name: 1-255 characters.
    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }

    /// Validate member role: must be "member" or "admin".
    fn validate_role(role: &str) -> AppResult<()> {
        match role {
            "member" | "admin" => Ok(()),
            _ => Err(ErrorKind::Validation("role must be 'member' or 'admin'".into()).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(GroupService::validate_name("A").is_ok());
        assert!(GroupService::validate_name("Backend Team").is_ok());
        assert!(GroupService::validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(GroupService::validate_name("").is_err());
        assert!(GroupService::validate_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn valid_roles() {
        assert!(GroupService::validate_role("member").is_ok());
        assert!(GroupService::validate_role("admin").is_ok());
    }

    #[test]
    fn invalid_roles() {
        assert!(GroupService::validate_role("").is_err());
        assert!(GroupService::validate_role("owner").is_err());
        assert!(GroupService::validate_role("superadmin").is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(0_i64.clamp(1, 100), 1);
        assert_eq!(200_i64.clamp(1, 100), 100);
        assert_eq!(50_i64.clamp(1, 100), 50);
    }

    #[test]
    fn offset_floor() {
        let negative_offset = -10_i64;
        let positive_offset = 50_i64;
        assert_eq!(negative_offset.max(0), 0);
        assert_eq!(positive_offset.max(0), 50);
    }
}
