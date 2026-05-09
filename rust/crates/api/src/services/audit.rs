//! Audit log service — logging actions and querying the audit trail.

use agentforge_core::{AppResult, OrgId, TenantScope, UserId};
use agentforge_db::entities::AuditLogEntry;
use uuid::Uuid;

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
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope.org_id(), action, resource_type, limit, offset).await
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
    #[test]
    fn audit_limit_clamping() {
        let limit: i64 = 500;
        let clamped = limit.clamp(1, 100);
        assert_eq!(clamped, 100);

        let limit: i64 = 0;
        let clamped = limit.clamp(1, 100);
        assert_eq!(clamped, 1);

        let limit: i64 = 50;
        let clamped = limit.clamp(1, 100);
        assert_eq!(clamped, 50);
    }

    #[test]
    fn audit_offset_clamping() {
        let offset: i64 = -5;
        let clamped = offset.max(0);
        assert_eq!(clamped, 0);

        let offset: i64 = 10;
        let clamped = offset.max(0);
        assert_eq!(clamped, 10);
    }
}
