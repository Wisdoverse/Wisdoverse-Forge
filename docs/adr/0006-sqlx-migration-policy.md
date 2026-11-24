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
5. **Legacy table collisions are reconciled in code, not in a migration.**
   A database first created by the legacy backend can hold a table in
   `public` under the same name a later migration creates with
   `CREATE TABLE IF NOT EXISTS`, but with the legacy shape. The migration then
   skips the table and fails on the next statement. Before the migrator runs,
   `run_migrations` (`rust/crates/db/src/pool.rs`) checks each entry in
   `LEGACY_TABLE_COLLISIONS` (migration version, table, a column only the
   current shape has). When the ledger lacks that version and `public.<table>`
   exists without that column, it moves the table, with its rows, indexes,
   constraints and owned sequences, to the schema `legacy_pre_<version>`
   (for example `legacy_pre_089`). It never uses `legacy`, because migration
   027 drops that schema with CASCADE. Foreign keys that point at it keep
   pointing at the moved table. Nothing is dropped. If the target schema
   already holds a table of that name, startup refuses with an error naming
   it and nothing is changed. A fresh database, a database already past that
   version, and a table that already has the current shape are left alone.
   The migrations themselves stay immutable (rule 1). An entry may be added
   only if all of the following hold:
   1. The conflicting migration has already been applied in at least one
      deployed database, so it cannot be edited.
   2. A new corrective migration cannot fix it, because it cannot be ordered
      before the conflicting one.
   3. The collision was observed on a real upgraded database, not
      hypothesised.
   4. The entry moves data aside and never drops it.
   5. The entry ships tests for row preservation, for a re-run or restart
      after a crash, and for the ledger guard.

   Otherwise, write a normal corrective migration.
6. **The PostgreSQL job queue (`agentforge-jobs`)** uses `FOR UPDATE SKIP
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
- [Backend database and queue rules](../agents/backend.md#database-and-queue).
- `agentforge-jobs` source — the `pg_notify` + `FOR UPDATE SKIP LOCKED`
  pattern.
