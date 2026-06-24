-- Migration 013: agents runtime/observability fields the frontend already expects.
--
-- The frontend `ManagedAgent` (src/api/AgentAPI.ts) and `AdminAgent` (src/ui/admin/types.ts)
-- types reference fields the Rust schema didn't carry, so the API has been emitting
-- placeholders ("", null, 0). Add the columns so callers see real values.
--
-- Fields:
--   cwd               TEXT      Working directory inside the container.
--   current_tool      TEXT      Last tool the agent invoked (refresh on event).
--   tokens_current    BIGINT    Tokens consumed by the in-flight turn.
--   tokens_cumulative BIGINT    Lifetime token consumption for the agent.
--   git_status        TEXT      Compact git status hint ("clean" / "+3 -1" / etc).
--   runtime_id        TEXT      Sidecar/runtime identifier ("af-XXXXXXXX") distinct from PK.
--   last_activity_at  TIMESTAMPTZ  Cached "max event created_at" so the admin list
--                                  doesn't recompute it from a LATERAL JOIN every call.

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS cwd TEXT,
    ADD COLUMN IF NOT EXISTS current_tool TEXT,
    ADD COLUMN IF NOT EXISTS tokens_current BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS tokens_cumulative BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS git_status TEXT,
    ADD COLUMN IF NOT EXISTS runtime_id TEXT,
    ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ;

-- Backfill last_activity_at from existing events so the cached field starts in sync.
UPDATE agents a
   SET last_activity_at = COALESCE(
       (SELECT MAX(created_at) FROM events WHERE agent_id = a.id),
       a.updated_at
   )
 WHERE last_activity_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_agents_runtime_id ON agents(runtime_id) WHERE runtime_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agents_last_activity ON agents(last_activity_at DESC NULLS LAST);
