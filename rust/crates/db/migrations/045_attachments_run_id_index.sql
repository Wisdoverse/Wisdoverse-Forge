-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_attachments_org_run_created_id
    ON attachments(organization_id, run_id, created_at ASC, id ASC)
    WHERE run_id IS NOT NULL;
