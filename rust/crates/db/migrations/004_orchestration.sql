-- Orchestration tasks
CREATE TABLE IF NOT EXISTS orchestration_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'pending', -- pending, running, completed, failed, canceled
    created_by UUID NOT NULL REFERENCES users(id),
    assigned_agent_id UUID REFERENCES agents(id),
    parent_task_id UUID REFERENCES orchestration_tasks(id),
    result JSONB,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Participants (agents registered for orchestration)
CREATE TABLE IF NOT EXISTS participants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    name TEXT NOT NULL,
    capabilities TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'available', -- available, busy, offline
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_heartbeat_at TIMESTAMPTZ,
    UNIQUE(organization_id, agent_id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_orch_tasks_org ON orchestration_tasks(organization_id);
CREATE INDEX IF NOT EXISTS idx_orch_tasks_status ON orchestration_tasks(status);
CREATE INDEX IF NOT EXISTS idx_orch_tasks_parent ON orchestration_tasks(parent_task_id);
CREATE INDEX IF NOT EXISTS idx_participants_org ON participants(organization_id);
CREATE INDEX IF NOT EXISTS idx_participants_status ON participants(status);

-- Triggers
DROP TRIGGER IF EXISTS orchestration_tasks_updated_at ON orchestration_tasks;
CREATE TRIGGER orchestration_tasks_updated_at BEFORE UPDATE ON orchestration_tasks FOR EACH ROW EXECUTE FUNCTION update_updated_at();
