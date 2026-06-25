-- F004: per-user session invalidation floor for durable forced logout.
--
-- A refresh/switch token whose `iat` (issued-at) predates this instant is
-- rejected, so a password reset — or the operator-gated legacy SHA-256
-- force-reset — durably kills every session minted before it, including
-- copied/stale refresh tokens still inside their multi-day lifetime. NULL means
-- "never invalidated" (every token is accepted on its own merits).
--
-- This migration is intentionally ADDITIVE and non-destructive: it never
-- rewrites a password hash, so deploying it can never lock anyone out. The
-- destructive legacy-hash force-reset is a separate, operator-opt-in step
-- (`FORCE_RESET_LEGACY_SHA256=true`) that runs only when a password-reset path
-- is configured — see the server startup routine.
--
-- Idempotent: `ADD COLUMN IF NOT EXISTS` tolerates re-runs and production drift.
ALTER TABLE users ADD COLUMN IF NOT EXISTS sessions_invalid_before TIMESTAMPTZ;

COMMENT ON COLUMN users.sessions_invalid_before IS
    'Refresh/switch tokens issued (iat) before this instant are rejected. Set by password reset and the operator-gated legacy SHA-256 force-reset. NULL = no invalidation.';
