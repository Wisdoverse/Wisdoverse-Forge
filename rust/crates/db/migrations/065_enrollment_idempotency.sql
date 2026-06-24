-- 065: idempotency table for POST /api/v1/agents/local-enroll.
--
-- A retried enrollment with the same (org_id, user_id, key) within the TTL
-- returns the original agent rather than minting a duplicate row with new
-- credentials. Closes the credential-proliferation concern from AppSec
-- review and the network-replay attack scenario from §16 of the spec.

CREATE TABLE IF NOT EXISTS enrollment_idempotency (
    org_id      UUID        NOT NULL,
    user_id     UUID        NOT NULL,
    key         TEXT        NOT NULL,
    agent_id    UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (org_id, user_id, key)
);

CREATE INDEX IF NOT EXISTS idx_enrollment_idempotency_expires_at
    ON enrollment_idempotency(expires_at);
