-- Saved prompts
CREATE TABLE IF NOT EXISTS prompts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    is_shared BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- File attachments
CREATE TABLE IF NOT EXISTS attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    agent_id UUID REFERENCES agents(id),
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    storage_backend TEXT NOT NULL DEFAULT 'local', -- local, minio
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Resource profiles
CREATE TABLE IF NOT EXISTS resource_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID REFERENCES organizations(id), -- NULL = system default
    name TEXT NOT NULL,
    cpu_millicores INT NOT NULL DEFAULT 1000,
    memory_mb INT NOT NULL DEFAULT 512,
    storage_mb INT NOT NULL DEFAULT 1024,
    max_pids INT NOT NULL DEFAULT 256,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Quota usage tracking
CREATE TABLE IF NOT EXISTS quota_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    resource_type TEXT NOT NULL, -- agents, storage, events
    current_usage BIGINT NOT NULL DEFAULT 0,
    max_allowed BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, resource_type)
);

-- Dev environments
CREATE TABLE IF NOT EXISTS dev_environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    project_id UUID REFERENCES projects(id),
    name TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'stopped', -- stopped, starting, running, error
    container_id TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_prompts_org ON prompts(organization_id);
CREATE INDEX IF NOT EXISTS idx_prompts_user ON prompts(user_id);
CREATE INDEX IF NOT EXISTS idx_attachments_org ON attachments(organization_id);
CREATE INDEX IF NOT EXISTS idx_attachments_agent ON attachments(agent_id);
CREATE INDEX IF NOT EXISTS idx_resource_profiles_org ON resource_profiles(organization_id);
CREATE INDEX IF NOT EXISTS idx_quota_org ON quota_usage(organization_id);
CREATE INDEX IF NOT EXISTS idx_devenv_org ON dev_environments(organization_id);
CREATE INDEX IF NOT EXISTS idx_devenv_project ON dev_environments(project_id);

DROP TRIGGER IF EXISTS prompts_updated_at ON prompts;
CREATE TRIGGER prompts_updated_at BEFORE UPDATE ON prompts FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS resource_profiles_updated_at ON resource_profiles;
CREATE TRIGGER resource_profiles_updated_at BEFORE UPDATE ON resource_profiles FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS dev_environments_updated_at ON dev_environments;
CREATE TRIGGER dev_environments_updated_at BEFORE UPDATE ON dev_environments FOR EACH ROW EXECUTE FUNCTION update_updated_at();
