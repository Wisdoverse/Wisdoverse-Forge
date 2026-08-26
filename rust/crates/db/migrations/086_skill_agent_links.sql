-- Skill agent links: explicit attachment between a governance skill and an
-- agent. Attached skills are the "matching agents" a team picks when a piece
-- of work proves reusable — the record makes the relationship first-class so
-- the UI can offer attach/detach and the runtime can preference these skills
-- for the agent's context.
CREATE TABLE IF NOT EXISTS skill_agent_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    skill_id UUID NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    attached_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT skill_agent_links_unique UNIQUE (skill_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_skill_agent_links_skill ON skill_agent_links(skill_id);
CREATE INDEX IF NOT EXISTS idx_skill_agent_links_agent ON skill_agent_links(agent_id);
