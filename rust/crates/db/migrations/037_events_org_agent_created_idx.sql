-- Composite access path for tenant-scoped agent event projection.
CREATE INDEX IF NOT EXISTS idx_events_org_agent_created_id
    ON events(organization_id, agent_id, created_at DESC, id DESC);
