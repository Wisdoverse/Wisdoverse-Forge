//! Governed context audit service.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::AuditLogEntry;
use sqlx::{Postgres, Transaction};

use crate::domain::context_governance::ContextGovernancePolicy;
pub use crate::domain::context_governance::{
    ContextAuditEvent, ContextScopeKind, GOVERNANCE_CONTEXT_ACTION_PREFIX, HIGH_ENTROPY_MIN_BYTES,
    HIGH_ENTROPY_THRESHOLD_BITS, MAX_CLASSIFICATION_INPUT_BYTES, ScopeExpansionDecision, ScopeExpansionRejection,
    ScopeExpansionRejectionReason, ScopeExpansionRequest, SecretPattern, Sensitivity, SensitivityClassification,
};
use crate::repositories::audit::AuditRepository;

pub struct ContextGovernanceService;

impl ContextGovernanceService {
    pub fn classify_sensitivity(content: &str) -> SensitivityClassification {
        ContextGovernancePolicy::classify_sensitivity(content)
    }

    pub async fn emit_audit(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        event: ContextAuditEvent<'_>,
    ) -> AppResult<AuditLogEntry> {
        ContextGovernancePolicy::validate_audit_event(&event)?;
        AuditRepository::create_in_tx(
            tx,
            scope.org_id(),
            Some(scope.user_id()),
            event.action,
            event.resource_type,
            event.resource_id,
            &event.payload,
            event.ip_address,
        )
        .await
    }

    pub fn gate_scope_expansion(
        request: ScopeExpansionRequest,
    ) -> Result<ScopeExpansionDecision, ScopeExpansionRejection> {
        ContextGovernancePolicy::gate_scope_expansion(request)
    }
}
