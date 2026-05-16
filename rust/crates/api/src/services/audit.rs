//! Audit log service — logging actions and querying the audit trail.

use agentforge_core::{AppResult, OrgId, TenantScope, UserId};
use agentforge_db::entities::AuditLogEntry;
use uuid::Uuid;

use crate::domain::observability::AuditListPage;
use crate::repositories::audit::AuditRepository;

/// Business logic layer for audit log operations.
pub struct AuditService {
    repo: AuditRepository,
}

impl AuditService {
    pub fn new(repo: AuditRepository) -> Self {
        Self { repo }
    }

    /// List audit log entries (paginated, with optional filters).
    pub async fn list(
        &self,
        scope: &TenantScope,
        action: Option<&str>,
        resource_type: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AuditLogEntry>> {
        let page = AuditListPage::new(limit, offset);
        self.repo.list(scope.org_id(), action, resource_type, page.limit(), page.offset()).await
    }

    /// Log an action to the audit trail — callable from other modules.
    pub async fn log_action(
        &self,
        org_id: OrgId,
        user_id: Option<UserId>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        details: &serde_json::Value,
        ip_address: Option<&str>,
    ) -> AppResult<AuditLogEntry> {
        self.repo.create(org_id, user_id, action, resource_type, resource_id, details, ip_address).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::observability::AuditListPage;

    #[test]
    fn audit_limit_clamping() {
        assert_eq!(AuditListPage::new(500, 0).limit(), 100);
        assert_eq!(AuditListPage::new(0, 0).limit(), 1);
        assert_eq!(AuditListPage::new(50, 0).limit(), 50);
    }

    #[test]
    fn audit_offset_clamping() {
        assert_eq!(AuditListPage::new(10, -5).offset(), 0);
        assert_eq!(AuditListPage::new(10, 10).offset(), 10);
    }
}
