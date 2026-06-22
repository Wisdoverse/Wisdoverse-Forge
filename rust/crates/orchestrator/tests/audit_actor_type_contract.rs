use chrono::Utc;
use sqlx::PgPool;

use agentforge_orchestrator::audit::{AuditAction, AuditLog, PgAuditStore, Store};

/// Pins the audit_logs CHECK (actor_type IN ('human','agent','system')).
///
/// Prior to the fix, `record_audit` helpers set `actor_type = "user"`, which
/// violated the CHECK and caused every human-actor audit insert to fail — making
/// workflow-signal audits (#797) silently drop and review-verdict routes (#842)
/// return 500.  This test proves that `"human"` satisfies both the schema CHECK
/// and the `WorkflowReviewApprove` action path.
#[sqlx::test(migrations = "./migrations")]
async fn human_actor_type_satisfies_check_constraint(pool: PgPool) {
    let store = PgAuditStore::new(pool);

    let result = store
        .create(&mut AuditLog {
            id: String::new(),
            action: AuditAction::WorkflowReviewApprove,
            actor_id: "user-abc123".to_string(),
            actor_type: "human".to_string(),
            resource: "workflow".to_string(),
            resource_id: Some("wf-contract-test".to_string()),
            org_id: "org-contract-test".to_string(),
            changes: None,
            ip_address: None,
            user_agent: None,
            created_at: Utc::now(),
        })
        .await;

    assert!(result.is_ok(), "human actor_type must satisfy the audit_logs CHECK: {result:?}");
}
