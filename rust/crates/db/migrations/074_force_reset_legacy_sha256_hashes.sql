-- F004: force-reset legacy unsalted SHA-256 password hashes.
--
-- A 64-hex-char `users.password_hash` is a legacy single-iteration, unsalted
-- SHA-256 digest (trivially brute-forced / rainbow-tabled if the table leaks).
-- Replace every remaining one with a sentinel that matches NO supported hash
-- format (argon2 / bcrypt / 64-hex-sha256), so:
--   (1) the weak hash is removed at rest (no brute-force target on leak), and
--   (2) login fails for that row (`verify_password_compat` -> valid:false),
--       forcing the account through password reset.
--
-- The compat window is also closed in production at the code layer
-- (`verify_password_compat(.., allow_legacy_sha256=false)`), so newly imported
-- SHA-256 rows would not be accepted either; this migration eliminates the
-- already-stored ones rather than waiting for each user's next login.
--
-- Idempotent: re-running matches only rows still in the legacy 64-hex form
-- (none after the first apply), tolerating any production drift.
DO $$
DECLARE
    affected integer;
BEGIN
    UPDATE users
    SET password_hash = 'LEGACY_SHA256_RESET_REQUIRED'
    WHERE password_hash ~ '^[0-9a-fA-F]{64}$';
    GET DIAGNOSTICS affected = ROW_COUNT;
    RAISE NOTICE 'F004: force-reset % legacy SHA-256 password hash(es)', affected;
END $$;
