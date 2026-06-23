-- 072: bootstrap a platform admin on upgrade (no-lockout for #881).
--
-- #881 moves every cross-org `/admin/*` endpoint behind the server-side
-- `users.is_admin` flag instead of the self-assignable per-org JWT role. On a
-- deployment that pre-dates that change, no user may have `is_admin = true`, so
-- after upgrade nobody could administer the platform — and `is_admin` is only
-- settable by an existing admin. This promotes the oldest surviving account to
-- platform admin so the deployment is never locked out.
--
-- Idempotent + safe: the `NOT EXISTS` guard makes this a no-op whenever any
-- (non-deleted) admin already exists, so re-running it (or running it on a
-- deployment that already has an admin) changes nothing. No hardcoded email —
-- the "oldest by created_at" account is chosen. Fresh installs are handled at
-- runtime by the first-user promotion in `UserRepository::create`; the two
-- paths share the same "first/oldest user becomes admin" invariant.
UPDATE users
   SET is_admin = true
 WHERE id = (
         SELECT id FROM users
          WHERE deleted_at IS NULL
          ORDER BY created_at ASC
          LIMIT 1
       )
   AND NOT EXISTS (
         SELECT 1 FROM users WHERE is_admin AND deleted_at IS NULL
       );
