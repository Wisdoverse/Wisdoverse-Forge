-- Plugins
CREATE TABLE IF NOT EXISTS plugins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID REFERENCES organizations(id), -- NULL = global plugin
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '0.1.0',
    description TEXT,
    config JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Skills
CREATE TABLE IF NOT EXISTS skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    trigger_pattern TEXT, -- regex or keyword trigger
    content TEXT NOT NULL, -- skill prompt content
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Analytics events
CREATE TABLE IF NOT EXISTS analytics_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID REFERENCES users(id),
    event_name TEXT NOT NULL,
    properties JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Licenses
CREATE TABLE IF NOT EXISTS licenses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    license_key TEXT NOT NULL UNIQUE,
    plan_name TEXT NOT NULL,
    max_agents INT NOT NULL DEFAULT 5,
    max_users INT NOT NULL DEFAULT 10,
    valid_from TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_until TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Voice providers
CREATE TABLE IF NOT EXISTS voice_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL, -- openai, deepgram, elevenlabs, custom
    config JSONB NOT NULL DEFAULT '{}',
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Tiles (dashboard layout)
CREATE TABLE IF NOT EXISTS tiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    tile_type TEXT NOT NULL, -- agent, feed, chart, custom
    config JSONB NOT NULL DEFAULT '{}',
    position_x INT NOT NULL DEFAULT 0,
    position_y INT NOT NULL DEFAULT 0,
    width INT NOT NULL DEFAULT 1,
    height INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_plugins_org ON plugins(organization_id);
CREATE INDEX IF NOT EXISTS idx_skills_org ON skills(organization_id);
CREATE INDEX IF NOT EXISTS idx_analytics_org ON analytics_events(organization_id);
CREATE INDEX IF NOT EXISTS idx_analytics_name ON analytics_events(event_name);
CREATE INDEX IF NOT EXISTS idx_analytics_created ON analytics_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_licenses_org ON licenses(organization_id);
CREATE INDEX IF NOT EXISTS idx_licenses_key ON licenses(license_key);
CREATE INDEX IF NOT EXISTS idx_voice_org ON voice_providers(organization_id);
CREATE INDEX IF NOT EXISTS idx_tiles_user ON tiles(user_id);

DROP TRIGGER IF EXISTS plugins_updated_at ON plugins;
CREATE TRIGGER plugins_updated_at BEFORE UPDATE ON plugins FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS skills_updated_at ON skills;
CREATE TRIGGER skills_updated_at BEFORE UPDATE ON skills FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS licenses_updated_at ON licenses;
CREATE TRIGGER licenses_updated_at BEFORE UPDATE ON licenses FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS voice_providers_updated_at ON voice_providers;
CREATE TRIGGER voice_providers_updated_at BEFORE UPDATE ON voice_providers FOR EACH ROW EXECUTE FUNCTION update_updated_at();
DROP TRIGGER IF EXISTS tiles_updated_at ON tiles;
CREATE TRIGGER tiles_updated_at BEFORE UPDATE ON tiles FOR EACH ROW EXECUTE FUNCTION update_updated_at();
