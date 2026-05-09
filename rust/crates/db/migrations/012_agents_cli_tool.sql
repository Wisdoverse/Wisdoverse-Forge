-- Migration 012: persist `cli_tool` on agents so the container-start endpoint
-- can pick the right image (`agentforge-agent:claude` etc) instead of trying to
-- pull `<model-name>:latest` from a registry.
--
-- Backfill best-effort: if `model` already looks like an `agentforge-agent:<tool>`
-- image string we extract the suffix; otherwise we leave NULL and the start
-- endpoint refuses with a clear error.

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS cli_tool TEXT;

UPDATE agents
SET cli_tool = LOWER(SPLIT_PART(model, ':', 2))
WHERE cli_tool IS NULL
  AND model LIKE 'agentforge-agent:%'
  AND LOWER(SPLIT_PART(model, ':', 2)) IN ('claude', 'codex', 'gemini', 'opencode');

-- Lowercased provider hints can also disambiguate (e.g. provider='Anthropic' + cli_tool null).
UPDATE agents
SET cli_tool = 'claude'
WHERE cli_tool IS NULL
  AND model IS NOT NULL
  AND LOWER(model) LIKE 'claude%';

UPDATE agents
SET cli_tool = 'codex'
WHERE cli_tool IS NULL
  AND model IS NOT NULL
  AND (LOWER(model) LIKE 'gpt%' OR LOWER(model) LIKE 'o1%' OR LOWER(model) LIKE 'codex%');

UPDATE agents
SET cli_tool = 'gemini'
WHERE cli_tool IS NULL
  AND model IS NOT NULL
  AND LOWER(model) LIKE 'gemini%';

CREATE INDEX IF NOT EXISTS idx_agents_cli_tool ON agents(cli_tool) WHERE cli_tool IS NOT NULL;
