-- rust/crates/db/migrations/031_agent_messages.sql
CREATE TABLE agent_messages (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  agent_id        UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  role            TEXT NOT NULL CHECK (role IN ('user','assistant')),
  content         TEXT NOT NULL,
  tokens_in       INTEGER,
  tokens_out      INTEGER,
  model           TEXT,
  finish_reason   TEXT, -- 'stop' | 'error' | 'interrupted' | NULL (user role)
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX agent_messages_agent_created_idx ON agent_messages (agent_id, created_at ASC);
CREATE INDEX agent_messages_org_idx           ON agent_messages (organization_id);
