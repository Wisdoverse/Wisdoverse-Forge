-- Migration 014: per-agent plugin enable/disable.
--
-- The org-scoped `plugins` table records what plugins exist; this join table
-- records which of those plugins are enabled (and with what config) for each
-- specific agent. Without it, the AgentPluginsTab can only show the global
-- list with a non-persisted toggle.

CREATE TABLE IF NOT EXISTS agent_plugins (
    agent_id   UUID NOT NULL REFERENCES agents(id)  ON DELETE CASCADE,
    plugin_id  UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    config     JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, plugin_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_plugins_agent ON agent_plugins(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_plugins_plugin ON agent_plugins(plugin_id);

DROP TRIGGER IF EXISTS agent_plugins_updated_at ON agent_plugins;
CREATE TRIGGER agent_plugins_updated_at
    BEFORE UPDATE ON agent_plugins
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
