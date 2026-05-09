-- no-transaction
-- Audit logging infrastructure

BEGIN;

CREATE TABLE IF NOT EXISTS audit_logs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    action      TEXT NOT NULL,
    actor_id    TEXT NOT NULL,
    actor_type  TEXT NOT NULL CHECK (actor_type IN ('human', 'agent', 'system')),
    resource    TEXT NOT NULL,
    resource_id TEXT,
    org_id      TEXT NOT NULL,
    changes     JSONB,
    ip_address  INET,
    user_agent  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_org_time ON audit_logs (org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs (actor_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_logs (resource, resource_id);

COMMIT;
