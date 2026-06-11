-- 066: pairing codes for one-command Host CLI join.
--
-- A join code is minted at enrollment time and exchanged ("claimed") by the
-- bootstrap script on the operator machine for the sidecar environment of
-- one specific agent. Only the SHA-256 of the code is stored; the plaintext
-- exists only in the enrollment response shown to the operator.
--
-- Codes expire after a short TTL and remain claimable until expiry so an
-- interrupted bootstrap (network blip, missing CLI) can simply re-run the
-- same command. `used_at` / `claim_count` exist for audit.

CREATE TABLE IF NOT EXISTS agent_join_codes (
    id              UUID        PRIMARY KEY,
    organization_id UUID        NOT NULL,
    agent_id        UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    code_hash       TEXT        NOT NULL UNIQUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    used_at         TIMESTAMPTZ,
    claim_count     INTEGER     NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_agent_join_codes_agent
    ON agent_join_codes(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_join_codes_expires_at
    ON agent_join_codes(expires_at);
