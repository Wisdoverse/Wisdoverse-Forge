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

1. `ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_invariants;`
2. `ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_check;`
3. Restore.
4. Re-run the backfill UPDATE from 062 against the restored rows.
5. Re-add both constraints with `NOT VALID`, then `VALIDATE`.

## Logical replication subscribers

`VALIDATE CONSTRAINT` on the primary does NOT validate the same CHECK on
logical-replication subscribers. Each subscriber's DBA must run:

    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;
    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;

directly against the subscriber after 063 ships.
