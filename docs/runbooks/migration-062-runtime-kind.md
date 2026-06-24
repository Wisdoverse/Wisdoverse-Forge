# Migration 062 / 063 / 064 — `agents.runtime_kind` discriminator

## Before you migrate

Run `agentforge migrate doctor` first. It will:

- count agent rows and refuse without `--force` if > 100,000
- report any agent row that would fail the post-062 invariant CHECK
- estimate lock duration
- verify replication subscribers are reachable

## During migration

The three migrations land in order:

1. 062 — column add + backfill (no CHECK yet). Old + new application instances coexist.
2. (Operator step) Confirm every API instance is on the new release.
3. 063 — CHECK constraints. From this point, INSERTs without `runtime_kind`
   that should be `'container'` (have `cli_tool`) will be rejected.
4. 064 — indexes. Schedule in a maintenance window.

## If 062's pre-flight assertion fails

The migration aborts with a row count of agents that would later violate
the invariant. To find the offenders:

    SELECT id, runtime_kind, cli_tool, container_id, runtime_id
    FROM agents
    WHERE NOT (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL)
    );

Fix each row manually (most commonly: an api row that has cli_tool from a
pre-018-rename era — clear cli_tool and set runtime_kind='api'). Then re-run.

## Restore from a pre-062 backup into a post-062 schema

A pre-062 dump has no `runtime_kind` column/values, so the post-062 `NOT NULL`
and `DEFAULT` must be dropped before the restore (a data-only restore would
otherwise fail the `NOT NULL`), then re-applied after the backfill. Wrap the
whole thing in a transaction.

```sql
BEGIN;
ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_invariants;
ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_check;
ALTER TABLE agents ALTER COLUMN runtime_kind DROP NOT NULL;
ALTER TABLE agents ALTER COLUMN runtime_kind DROP DEFAULT;
-- (restore the pre-062 rows here — they arrive with runtime_kind NULL)
-- Re-run the 062 backfill against the restored rows:
UPDATE agents SET runtime_kind = CASE
    WHEN cli_tool IS NULL                                      THEN 'api'
    WHEN runtime_id IS NOT NULL AND runtime_id LIKE 'host-%'   THEN 'cli'
    ELSE                                                            'container'
END
WHERE runtime_kind IS NULL;
ALTER TABLE agents
    ALTER COLUMN runtime_kind SET NOT NULL,
    ALTER COLUMN runtime_kind SET DEFAULT 'api';
ALTER TABLE agents ADD CONSTRAINT agents_runtime_kind_check
    CHECK (runtime_kind IN ('container','cli','api')) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;
ALTER TABLE agents ADD CONSTRAINT agents_runtime_kind_invariants
    CHECK (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL    AND container_id IS NULL)
    ) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;
COMMIT;
```

If the restored data contains an invariant offender (e.g. an `api`-shaped row
with a stale `container_id`), the final `VALIDATE CONSTRAINT` will fail — fix it
with the offender query above before re-running the validate.

## Logical replication subscribers

`VALIDATE CONSTRAINT` on the primary does NOT validate the same CHECK on
logical-replication subscribers. Each subscriber's DBA must run:

    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;
    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;

directly against the subscriber after 063 ships.

## Dry-run record (2026-05-30, issue #462)

Rehearsed the full 062→065 sequence against a scratch copy of the local
`prod-ext` `agentforge-db` (Postgres 18.4) seeded with 4 representative agents
(container / host-cli / api) plus one deliberate offender (`api` shape with a
stale `container_id`). Findings — both since fixed:

1. **062's pre-flight assertion was weaker than 063's CHECK.** The `api` arm
   read `cli_tool IS NULL` but omitted `AND container_id IS NULL`. The offender
   passed 062 clean, then 063's `VALIDATE CONSTRAINT` failed
   (`agents_runtime_kind_invariants is violated by some row`), leaving a
   half-applied migration — exactly the state the pre-flight exists to prevent.
   Fixed so 062's predicate is byte-identical to 063's (and a parity test now
   guards against future drift).
2. **This rollback runbook omitted the `NOT NULL`/`DEFAULT` handling** needed to
   restore a pre-062 dump into a post-062 schema. Corrected above.

After the fix, the rehearsal passed end-to-end:

- `agentforge migrate doctor` (pre): 4 rows, column absent, OK.
- Fixed 062 on the offender dataset: aborts with `1 rows would violate
agents_runtime_kind_invariants` (pre-flight now catches it).
- After remediating the offender (`UPDATE ... SET container_id = NULL`), 062
  re-runs idempotently: `ADD COLUMN` ~3 ms, backfill ~10 ms, pre-flight ~1 ms,
  `SET NOT NULL` ~4 ms (4-row dataset).
- 063 (CHECK), 064 (index + partial UNIQUE on `runtime_id`), 065
  (`enrollment_idempotency`) all apply clean.
- End state: distribution container=1 / cli=1 / api=2; all three constraints
  present and `convalidated = t`; `uq_agents_runtime_id` rejects a duplicate
  `host-` runtime_id; `migrate doctor` (post): `invariant CHECK pre-flight: 0
offenders, OK`.
- Corrected rollback drill (drop constraints + NOT NULL/DEFAULT → re-backfill →
  re-add + re-validate) restores the post-062 state cleanly.

Timings are from a 4-row scratch DB and are not representative of production
volume; re-estimate with `agentforge migrate doctor` (which gates on row count)
before a real run.
