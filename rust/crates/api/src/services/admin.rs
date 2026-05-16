//! Admin service — business logic for admin-only operations.

use agentforge_core::{AppError, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{ImpersonationLog, Organization, User};
use uuid::Uuid;

use crate::domain::admin::{AdminImpersonationPolicy, AdminListPage, AdminRolePolicy};
use crate::repositories::admin::{AdminAgentEventRow, AdminAgentFilters, AdminAgentRow, AdminRepository, AdminStats};

/// Business logic layer for admin operations.
pub struct AdminService {
    repo: AdminRepository,
}

impl AdminService {
    pub fn new(repo: AdminRepository) -> Self {
        Self { repo }
    }

    /// List all users (admin only). Limit capped at 100.
    pub async fn list_all_users(&self, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let page = AdminListPage::new(limit, offset);
        self.repo.list_all_users(page.limit(), page.offset()).await
    }

    /// List all organizations (admin only). Limit capped at 100.
    pub async fn list_all_organizations(&self, limit: i64, offset: i64) -> AppResult<Vec<Organization>> {
        let page = AdminListPage::new(limit, offset);
        self.repo.list_all_organizations(page.limit(), page.offset()).await
    }

    /// Start impersonation of a target user.
    pub async fn start_impersonation(
        &self,
        scope: &TenantScope,
        target_user_id: Uuid,
        reason: Option<&str>,
    ) -> AppResult<ImpersonationLog> {
        AdminImpersonationPolicy::ensure_not_self(scope.user_id().as_uuid(), target_user_id)?;
        self.repo.start_impersonation(scope, target_user_id, reason).await
    }

    /// End the current impersonation session.
    pub async fn end_impersonation(&self, scope: &TenantScope) -> AppResult<ImpersonationLog> {
        self.repo.end_impersonation(scope).await
    }

    /// List impersonation log (admin only). Limit capped at 100.
    pub async fn list_impersonation_log(
        &self,
        scope: &TenantScope,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ImpersonationLog>> {
        let page = AdminListPage::new(limit, offset);
        self.repo.list_impersonation_log(scope, page.limit(), page.offset()).await
    }

    /// Get system-wide statistics.
    pub async fn stats(&self) -> AppResult<AdminStats> {
        self.repo.stats().await
    }

    /// List agents across every organization for the admin dashboard. Applies
    /// the same limit clamping as other admin list endpoints.
    pub async fn list_agents(&self, mut filters: AdminAgentFilters) -> AppResult<(Vec<AdminAgentRow>, i64)> {
        let page = AdminListPage::new(filters.limit, filters.offset);
        filters.limit = page.limit();
        filters.offset = page.offset();
        self.repo.list_agents(&filters).await
    }

    /// Fetch a single agent by ID (admin only) along with its most recent events.
    /// Returns `(row, recent_events)`; callers assemble the final JSON response.
    pub async fn get_agent(&self, agent_id: Uuid) -> AppResult<(AdminAgentRow, Vec<AdminAgentEventRow>)> {
        let row = self.repo.find_agent_by_id(agent_id).await?;
        let events = self.repo.recent_events_for_agent(agent_id, 20).await?;
        Ok((row, events))
    }

    /// Hard-delete a single agent (admin only).
    pub async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.repo.delete_agent(agent_id).await
    }

    /// Delete multiple agents, collecting per-ID success/failure results so the
    /// frontend can show which IDs were handled. Error messages are derived
    /// from `ErrorKind` (which implements `Display`) to avoid leaking internal
    /// error details in the response.
    pub async fn bulk_delete_agents(&self, agent_ids: &[Uuid]) -> Vec<BulkDeleteResult> {
        let mut results = Vec::with_capacity(agent_ids.len());
        for id in agent_ids {
            match self.repo.delete_agent(*id).await {
                Ok(()) => results.push(BulkDeleteResult { id: *id, ok: true, error: None }),
                Err(err) => {
                    results.push(BulkDeleteResult { id: *id, ok: false, error: Some(bulk_delete_error_message(&err)) })
                }
            }
        }
        results
    }

    /// Check if the user has admin privileges. Returns an error if not.
    pub fn require_admin(auth_role: &str) -> AppResult<()> {
        AdminRolePolicy::require_admin(auth_role)
    }
}

/// Per-ID outcome from a bulk-delete call, serialised in admin API responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BulkDeleteResult {
    pub id: Uuid,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Turn an `AppError` into a safe, client-facing message for bulk delete.
/// Internal errors collapse to a generic "delete failed" string so database
/// / infra details never leak into the HTTP response.
fn bulk_delete_error_message(err: &AppError) -> String {
    match &err.kind {
        ErrorKind::NotFound(_) => "agent not found".to_string(),
        ErrorKind::Validation(msg) => format!("validation error: {msg}"),
        ErrorKind::Unprocessable(msg) => format!("unprocessable entity: {msg}"),
        ErrorKind::Conflict(msg) => format!("conflict: {msg}"),
        ErrorKind::Unauthorized => "unauthorized".to_string(),
        ErrorKind::Forbidden => "forbidden".to_string(),
        ErrorKind::Unavailable(msg) => format!("service unavailable: {msg}"),
        ErrorKind::Internal(_) => "delete failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_role_check_owner() {
        assert!(AdminService::require_admin("owner").is_ok());
    }

    #[test]
    fn admin_role_check_admin() {
        assert!(AdminService::require_admin("admin").is_ok());
    }

    #[test]
    fn admin_role_check_member_rejected() {
        assert!(AdminService::require_admin("member").is_err());
    }

    #[test]
    fn admin_role_check_viewer_rejected() {
        assert!(AdminService::require_admin("viewer").is_err());
    }

    #[test]
    fn admin_role_check_empty_rejected() {
        assert!(AdminService::require_admin("").is_err());
    }
}
