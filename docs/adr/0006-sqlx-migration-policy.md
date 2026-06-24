# ADR 0006 — SQLx migration policy

## Status

Accepted.

## Context

`sqlx::migrate!` verifies each migration against a checksum stored in the
`_sqlx_migrations` table. Editing a migration that has already run in any
environment changes its checksum and causes startup to fail with a verification
error. Production has run a long tail of historical migrations that cannot be
recomputed cheaply.

A second class of failure comes from non-idempotent migrations: deploys are
sometimes resumed after partial failure (CI crash, manual intervention) and
the migration needs to be safe to re-run.

## Decision

1. **Never edit a migration that has run in production.** If a previous
   migration has a bug, add a new corrective migration that fixes the schema
   forward. Treat the historical file as immutable history.
2. **Make migrations idempotent when they tolerate existing production
   drift.** Use `CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`,
   `DO $$ ... $$` blocks that check for column existence before adding, and
   guarded `INSERT ... ON CONFLICT DO NOTHING`. Migrations that _must_ run
   exactly once (e.g. data backfills) state that requirement in a comment and
   are guarded by a sentinel column or row.
3. **One migration per logical change.** Bundling unrelated schema changes
   makes rollbacks impossible and review noisy. Numbered file names enforce
   ordering.
4. **Migrations live in `rust/crates/db/migrations/`.** Adopted tables that
   previously lived outside SQLx tracking carry a schema-contract test so a
   fresh test database and production never drift into a split-schema state.
5. **The PostgreSQL job queue (`agentforge-jobs`)** uses `FOR UPDATE SKIP
LOCKED`. `pg_notify` is a wake-up signal only; a polling fallback must
   exist so notification loss does not stall the queue.

## Consequences

- Deploys are not blocked by checksum drift when production state matches the
  migration history.
- Migration review focuses on "is this safe under partial application?" and
  "is the rollback path clear?".
- Forward-only migrations make rollback a deployment of a previous artifact,
  not a schema reversal. Operators rely on tested backup paths
  (`docs/guides/disaster-recovery.md`) when a schema change misbehaves.
- New contributors learn quickly that editing a numbered migration is never
  the right answer; a corrective follow-up always is.

## References

- `rust/crates/db/migrations/` — current migration set.
- `docs/guides/disaster-recovery.md` — backup and restore procedure.
- `AGENTS.md` — "Backend Contracts" / "DB migrations" section.
- `agentforge-jobs` source — the `pg_notify` + `FOR UPDATE SKIP LOCKED`
  pattern.
