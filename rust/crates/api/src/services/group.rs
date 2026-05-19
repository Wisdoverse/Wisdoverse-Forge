//! Group service — business logic and validation for groups.

use agentforge_core::{AppResult, GroupId, ProjectId, TenantScope};
use agentforge_db::entities::{Group, GroupMember};
use uuid::Uuid;

use crate::domain::resource::{GroupMemberRole, ResourceListPage, ResourceName};
use crate::repositories::identity::group::GroupRepository;

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
        let page = ResourceListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
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
        let name = ResourceName::parse(name)?;
        self.repo.create(scope, name.value(), description, project_id).await
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
        let name = name.map(ResourceName::parse).transpose()?.map(ResourceName::value);
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
        let role = GroupMemberRole::parse(role)?;
        self.repo.add_member(scope, group_id, user_id, role.as_str()).await
    }

    /// Remove a member from a group.
    pub async fn remove_member(&self, scope: &TenantScope, group_id: GroupId, user_id: Uuid) -> AppResult<()> {
        self.repo.remove_member(scope, group_id, user_id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::{GroupMemberRole, ResourceListPage, ResourceName};

    #[test]
    fn valid_names() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse("Backend Team").is_ok());
        assert!(ResourceName::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(ResourceName::parse("").is_err());
        assert!(ResourceName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn valid_roles() {
        assert!(GroupMemberRole::parse("member").is_ok());
        assert!(GroupMemberRole::parse("admin").is_ok());
    }

    #[test]
    fn invalid_roles() {
        assert!(GroupMemberRole::parse("").is_err());
        assert!(GroupMemberRole::parse("owner").is_err());
        assert!(GroupMemberRole::parse("superadmin").is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(ResourceListPage::new(0, 0).limit(), 1);
        assert_eq!(ResourceListPage::new(200, 0).limit(), 100);
        assert_eq!(ResourceListPage::new(50, 0).limit(), 50);
    }

    #[test]
    fn offset_floor() {
        assert_eq!(ResourceListPage::new(10, -10).offset(), 0);
        assert_eq!(ResourceListPage::new(10, 50).offset(), 50);
    }
}
