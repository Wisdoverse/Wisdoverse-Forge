-- Migration 034: one-time password reset tokens.
--
-- Tokens are stored as SHA-256 hashes only. The raw token exists only in the
-- email link and is never persisted. A successful reset marks the token used
-- and invalidates any other outstanding reset tokens for the same user.

CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user_active
    ON password_reset_tokens(user_id, created_at DESC)
    WHERE used_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_expiry
    ON password_reset_tokens(expires_at)
    WHERE used_at IS NULL;
