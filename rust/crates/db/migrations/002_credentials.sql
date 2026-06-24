-- API Keys
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,          -- SHA-256 hash of the API key
    key_prefix TEXT NOT NULL,        -- First 8 chars for identification
    scopes TEXT[] NOT NULL DEFAULT '{}',  -- e.g., {'read', 'write', 'admin'}
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ           -- Soft revoke
);

-- SSH Keys
CREATE TABLE IF NOT EXISTS ssh_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,        -- SHA-256 fingerprint
    key_type TEXT NOT NULL DEFAULT 'ed25519',  -- ed25519, rsa, ecdsa
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Git Credentials
CREATE TABLE IF NOT EXISTS git_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    provider TEXT NOT NULL,           -- github, gitlab, bitbucket, custom
    credential_type TEXT NOT NULL,    -- token, ssh, oauth
    token_encrypted BYTEA,            -- AES-256-GCM encrypted
    token_nonce BYTEA,
    remote_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_api_keys_org ON api_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_ssh_keys_org ON ssh_keys(organization_id);
CREATE INDEX IF NOT EXISTS idx_ssh_keys_fingerprint ON ssh_keys(fingerprint);
CREATE INDEX IF NOT EXISTS idx_git_creds_org ON git_credentials(organization_id);

-- Updated_at triggers
DROP TRIGGER IF EXISTS git_credentials_updated_at ON git_credentials;
CREATE TRIGGER git_credentials_updated_at BEFORE UPDATE ON git_credentials FOR EACH ROW EXECUTE FUNCTION update_updated_at();
