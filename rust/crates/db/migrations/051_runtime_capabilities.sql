CREATE TABLE IF NOT EXISTS runtime_capabilities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cli_tool TEXT NOT NULL,
    runtime_kind TEXT NOT NULL,
    max_context_tokens INTEGER NOT NULL,
    supports_skills_mount BOOLEAN NOT NULL,
    supports_hooks BOOLEAN NOT NULL,
    supports_subagents BOOLEAN NOT NULL,
    supports_mcp_bridge BOOLEAN NOT NULL,
    supports_terminal BOOLEAN NOT NULL,
    capability_profile JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT runtime_capabilities_cli_tool_check
        CHECK (cli_tool IN ('claude', 'codex', 'gemini', 'opencode')),
    CONSTRAINT runtime_capabilities_runtime_kind_check
        CHECK (runtime_kind IN ('container', 'cli', 'api')),
    CONSTRAINT runtime_capabilities_max_context_tokens_check
        CHECK (max_context_tokens > 0),
    CONSTRAINT runtime_capabilities_profile_object_check
        CHECK (jsonb_typeof(capability_profile) = 'object'),
    CONSTRAINT runtime_capabilities_unique_key UNIQUE (cli_tool, runtime_kind)
);

CREATE INDEX IF NOT EXISTS runtime_capabilities_runtime_kind_idx
    ON runtime_capabilities (runtime_kind, cli_tool);
