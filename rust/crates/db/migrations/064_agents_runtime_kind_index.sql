-- 064: add indexes for agents.runtime_kind discriminator queries
-- and close the runtime_id collision concern with a partial UNIQUE index.

SET lock_timeout = '10s';

CREATE INDEX IF NOT EXISTS idx_agents_runtime_kind ON agents(runtime_kind);

-- runtime_id is the per-agent sidecar identity. host_cli rows derive it from
-- the full Agent UUID (`host-{uuid}`); container rows leave it NULL until the
-- sidecar registers. Two rows with the same runtime_id would mean two agents
-- could authenticate as the same NATS principal — a privilege confusion.
CREATE UNIQUE INDEX IF NOT EXISTS uq_agents_runtime_id
    ON agents(runtime_id)
    WHERE runtime_id IS NOT NULL;
