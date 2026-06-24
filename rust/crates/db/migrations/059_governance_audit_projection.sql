-- Unit 5.2: governance audit projection support.
--
-- Phase 2 governance services persist `governance.context.*` decisions in
-- `audit_log`. Keep the projection read path fast as the general audit log
-- grows.

CREATE INDEX IF NOT EXISTS idx_audit_log_governance_context_created
    ON audit_log(organization_id, created_at DESC, id DESC)
    WHERE action LIKE 'governance.context.%';

CREATE INDEX IF NOT EXISTS idx_audit_log_governance_context_actor
    ON audit_log(organization_id, user_id, created_at DESC, id DESC)
    WHERE action LIKE 'governance.context.%';
