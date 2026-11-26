# Disaster Recovery Runbook

This runbook is written for operators restoring a self-contained production
deployment started with `make prod`, `make prod-pull`, `make prod-backup`, or
`make prod-storage`.

If your deployment uses `make prod-ext`, PostgreSQL and Redis are external
services. That includes the API database (`DATABASE_URL`), the orchestrator
database (`ORCHESTRATOR_DATABASE_URL`), and Temporal's databases
(`TEMPORAL_DB_NAME` and its `_visibility` database on `DB_HOST`). The bundled
`backup` service does not cover external databases; you own their backups. Do not
enable the `backup` profile there: it has no bundled database to dump, so it
restarts with a connection error. Back up and restore them with your provider's
runbook, in the same order as below, then use this document only for the Forge
services, object storage, and health checks.

Before you start:

1. Confirm you are on the server that owns the deployment.
2. Confirm `docker/.env` is present and restored from secure storage.
3. Run commands from the repository root.
4. Set this helper once per shell session:

```bash
COMPOSE_PROD="docker compose --env-file docker/.env -f docker/compose.yml -f docker/compose.prod.yml --profile prod"
COMPOSE_PROD_STORAGE="docker compose --env-file docker/.env -f docker/compose.yml -f docker/compose.prod.yml -f docker/compose.storage.yml --profile prod --profile storage"
COMPOSE_BACKUP="$COMPOSE_PROD --profile backup"
```

The examples below use Compose service names from `docker/compose.yml`: `db`,
`redis`, `nats`, `agentforge-server`, `orchestrator`, `orchestrator-db`, and
`temporal`. Avoid old container nicknames such as `server` or `postgres`; those
are not service names in the current Compose files.

## RTO/RPO Targets

| Component                          | RPO (Recovery Point Objective) | RTO (Recovery Time Objective) |
| ---------------------------------- | ------------------------------ | ----------------------------- |
| PostgreSQL (agents, events, users) | 1 hour                         | 30 minutes                    |
| Orchestrator DB (workflows, tasks) | 1 hour                         | 30 minutes                    |
| Temporal DBs (running workflows)   | 1 hour                         | 30 minutes                    |
| Redis (cache, pub/sub)             | N/A (ephemeral)                | 5 minutes (restart)           |
| RustFS (attachments)               | 24 hours                       | 2 hours                       |
| NATS (event stream)                | N/A (transient)                | 5 minutes (restart)           |
| Full system                        | 1 hour                         | 4 hours                       |

---

## 1. Backup Procedures

### PostgreSQL (all bundled databases)

The self-contained stack keeps its state in four PostgreSQL databases on two
database servers. One backup run dumps all four and gives their files one
timestamp; this set of four files is a **backup set**.

| Database              | Server (Compose service) | Holds                                                 | Backup file                                  |
| --------------------- | ------------------------ | ----------------------------------------------------- | -------------------------------------------- |
| `orchestrator`        | `orchestrator-db`        | Workflows, reviews, orchestrator tasks, audit records | `orchestrator_<YYYYMMDD_HHMMSS>.dump`        |
| `temporal`            | `db`                     | Temporal workflow executions, history, and timers     | `temporal_<YYYYMMDD_HHMMSS>.dump`            |
| `temporal_visibility` | `db`                     | Temporal's workflow search index                      | `temporal_visibility_<YYYYMMDD_HHMMSS>.dump` |
| `agentforge` (API)    | `db`                     | Users, organizations, agents, events, API tasks       | `agentforge_<YYYYMMDD_HHMMSS>.dump`          |

Prerequisites: the stack runs with `make prod`, `make prod-pull`, or
`make prod-backup`, and `docker/.env` has `POSTGRES_PASSWORD` and
`ORCHESTRATOR_DB_PASSWORD`.

**Automated daily backup (recommended):**

```bash
make prod-backup
```

The `backup` service dumps the four databases into `docker/backups/` once a
day and deletes dumps older than 7 days. Each run first writes its dumps into
its own hidden staging folder, `docker/backups/.staging-<timestamp>-<id>/`. It
publishes them as `.dump` files only after all four dumped. If any dump fails,
the run removes its own staging folder, exits with the `pg_dump` error, and the
service restarts and retries. It does not publish a partial set and does not
delete older backups after a failure.

Stopping the service (`$COMPOSE_BACKUP stop backup`) or interrupting a run with
TERM, HUP, or INT ends it within a second or two, also in the middle of a dump.
The run stops its `pg_dump`, removes its own staging folder, releases the lock,
publishes nothing, and exits non-zero: 130 for a stop (Docker stops this image
with INT), 143 for TERM, 129 for HUP. A run killed without a chance to clean up (for example `docker kill`)
can leave its staging folder behind; the next run deletes staging folders older
than a day.

Only one run works at a time. Each run takes the lock file
`docker/backups/.backup.lock` first. The scheduled service waits as long as
another run holds the lock. A manual run waits up to 10 minutes, then stops
with exit code 75 and `another backup run still holds /backups/.backup.lock`,
without touching any file.

The dumps run one after another, in this order: `orchestrator`, `temporal`,
`temporal_visibility`, then `agentforge`. Orchestrator rows hold Temporal
execution IDs and API IDs, and Temporal executions point at orchestrator
workflows. Dumping the referencing side first and the referenced API side last
means a record created during the run is less likely to be referenced but
missing. The set is not one snapshot; see
[What a restore recovers](#what-a-restore-recovers).

Success looks like this: the log shows `backup complete: <timestamp>` and the
folder holds four files with that timestamp.

```bash
$COMPOSE_BACKUP logs --tail 20 backup
ls -lt docker/backups | head -5
```

If `$COMPOSE_BACKUP ps backup` shows `Restarting`, read the log. It names the
database that failed. Copy `docker/backups/` to storage off this host; the
folder is lost with the server.

**Manual backup (same code as the schedule, one pass):**

```bash
$COMPOSE_BACKUP run --rm -e BACKUP_ONCE=1 backup
```

It ends with `backup complete: <timestamp>` and exit code 0. Otherwise it exits
non-zero with the `pg_dump` error, or with code 75 when another run kept the
lock for 10 minutes. Manual sets follow the same 7-day retention.

**Verify a backup set:**

```bash
BACKUP_SET=YYYYMMDD_HHMMSS   # the timestamp of the set
$COMPOSE_BACKUP run --rm --no-deps -e BACKUP_SET="$BACKUP_SET" --entrypoint sh backup -ec '
  for db in orchestrator temporal temporal_visibility agentforge; do
    pg_restore --list "/backups/${db}_$BACKUP_SET.dump" >/dev/null
    echo "readable: ${db}_$BACKUP_SET.dump"
  done'
```

### Redis

Redis is used for caching and pub/sub — data is ephemeral. No backup required.
If persistence is enabled (RDB/AOF), copy the dump file:

```bash
redis-cli BGSAVE
cp /var/lib/redis/dump.rdb /backup/redis-dump-$(date +%Y%m%d).rdb
```

### RustFS (Attachments)

Run `mc` on the Compose network, or use an endpoint reachable from the client.
Install the existing `mc` S3-compatible client. Configure its endpoint and
credentials in the shell that runs `mc`; Compose's `docker/.env` file does not
export values into the shell. Do not put real credentials in shell history.

```bash
mc alias set rustfs "$S3_ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY"
mc alias set backup "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY"
mc mirror --overwrite --preserve rustfs/agentforge "backup/agentforge-$(date +%Y%m%d)"
```

Back up to an independent S3 bucket so `--preserve` retains user metadata. A
mirror to a local filesystem does not retain S3 user metadata unless you also
export and restore a per-object metadata inventory. Keep a per-object SHA-256
and size inventory with the backup, and periodically restore to a separate
bucket or deployment. This backs up S3 API objects, not the RustFS data volume.

### NATS

NATS JetStream data is transient. If persistence is needed, configure JetStream file storage and back up the data directory:

```bash
cp -r /var/lib/nats/jetstream /backup/nats-jetstream-$(date +%Y%m%d)/
```

---

## 2. Restore Procedures

### What a restore recovers

Read this before you restore, so you know what to check afterwards.

- **Recovered:** each database as it was when its own dump ran.
- **Not recovered:** everything written after the backup run. Work done
  between two dumps of the same run can be in one database but missing from
  another. The four dumps run one after another, seconds to minutes apart, so
  they are not one snapshot.
- **Skew between the `orchestrator` dump and the `temporal` dump.** Workflow
  rows can disagree with Temporal executions in two ways:
  - **Started between the two dumps and still running when Temporal was
    dumped.** The row comes back as `draft` without an execution ID, while
    Temporal still runs the workflow. This is the only case that corrects
    itself. The workflow's last step, `finalize_workflow_status`, had not run
    yet, so Temporal runs it after the restore and writes the final status.
  - **Finished between the two dumps.** The row comes back as `running` (or
    `draft`), but the restored Temporal execution has already completed,
    including its final status step. Completed steps do not run again, so the
    row never reaches its final status. Step statuses (`workflow_nodes`) that
    changed between the two dumps lag behind in the same way.
- **Other skew:**
  - An organization deleted between the `orchestrator` dump and the
    `agentforge` dump still has orchestrator rows.
  - Steps that were running at backup time can run again after the restore,
    because Temporal retries steps that have no recorded result. An agent task
    can therefore run twice.
- **If you do not restore the Temporal databases** (for example they were lost,
  or you restore only `agentforge` and `orchestrator`), every workflow that was
  running is **stranded**: its row says `running`, but no Temporal execution
  exists.

The orchestrator repairs workflow rows that say `running` by itself, once
Temporal is back (see [After the restore](#after-the-restore-check-for-skew)):

- a row whose recorded Temporal run has finished gets that run's final status;
- a row whose execution is gone is marked `failed` after at least 10 minutes,
  and can then be run again;
- a row whose execution still runs is left alone.

Rows that came back as `draft` are not touched; `draft` workflows can be run
normally.

### PostgreSQL Restore (all bundled databases)

Prerequisites:

- A complete backup set: four files with one timestamp in `docker/backups/`.
- `docker/.env` has `POSTGRES_PASSWORD` and `ORCHESTRATOR_DB_PASSWORD` for the
  database servers you restore into.
- The same Forge release and the same `temporalio/auto-setup` image tag as when
  the backup was taken. Check with `$COMPOSE_PROD images`. See
  [Schema versions at startup](#schema-versions-at-startup).
- About 30 minutes during which the API, orchestrator, and Temporal are offline.
- A **bash** shell in the repository root, with the helper variables from the
  top of this guide set. The block below stops at once in other shells.

The restore never writes into the current databases. It runs in five phases
and stops at the first error:

1. **Preflight** checks the backup set, the database names, and open
   connections. It creates nothing.
2. **Restore** loads each dump into a new, empty `<db>_restore` database.
3. **Validate** checks that every restored database has its tables and
   migration ledger.
4. **Promote** runs only when all four databases passed. Each database is
   swapped in its own transaction: the current one becomes
   `<db>_before_restore` and `<db>_restore` becomes `<db>`.
5. **Start** brings up the API, then Temporal, then the orchestrator, and
   checks health. The orchestrator is last because it is the only service that
   reads the other two: it calls the API and runs the Temporal workflows.

Edit the `BACKUP_SET=` line, then paste the whole block. It runs in a subshell,
so a failure stops the block without closing your terminal.

```bash
( [ -n "${BASH_VERSION:-}" ] || { echo "Run this block in bash." >&2; exit 1; }
set -euo pipefail; shopt -s inherit_errexit
BACKUP_SET=YYYYMMDD_HHMMSS   # timestamp of the backup set to restore; type it once, here
sql() { local svc=$1 role=$2 db=$3; shift 3; $COMPOSE_PROD exec -T "$svc" psql -U "$role" -d "$db" -v ON_ERROR_STOP=1 -qAt "$@"; }

echo "== Phase 1: preflight (creates nothing)"
[[ "$BACKUP_SET" =~ ^[0-9]{8}_[0-9]{6}$ ]] || { echo "BACKUP_SET must look like 20260101_020000, not: $BACKUP_SET" >&2; exit 1; }
for db in orchestrator temporal temporal_visibility agentforge; do
  [ -s "docker/backups/${db}_${BACKUP_SET}.dump" ] || { echo "missing or empty: docker/backups/${db}_${BACKUP_SET}.dump" >&2; exit 1; }
done
$COMPOSE_BACKUP run --rm --no-deps -T -e BACKUP_SET="$BACKUP_SET" --entrypoint sh backup -ec '
  for db in orchestrator temporal temporal_visibility agentforge; do
    pg_restore --list "/backups/${db}_$BACKUP_SET.dump" >/dev/null || { echo "unreadable archive: ${db}_$BACKUP_SET.dump" >&2; exit 1; }
  done'
echo "  backup set $BACKUP_SET: four archives present and readable"
$COMPOSE_BACKUP stop agentforge-server orchestrator temporal backup
$COMPOSE_PROD up -d --wait db orchestrator-db
taken=$(sql db agentforge postgres -c "select datname from pg_database where datname ~ '^(agentforge|temporal|temporal_visibility)_(before_)?restore$'"
        sql orchestrator-db orchestrator postgres -c "select datname from pg_database where datname ~ '^orchestrator_(before_)?restore$'")
[ -z "$taken" ] || { echo "these databases already exist; undo or clean up the earlier restore first:" $taken >&2; exit 1; }
busy=$(sql db agentforge postgres -c "select datname || ' ' || usename || ' ' || coalesce(application_name, '') || ' ' || coalesce(client_addr::text, 'local') from pg_stat_activity where datname in ('agentforge', 'temporal', 'temporal_visibility') and pid <> pg_backend_pid()"
       sql orchestrator-db orchestrator postgres -c "select datname || ' ' || usename || ' ' || coalesce(application_name, '') || ' ' || coalesce(client_addr::text, 'local') from pg_stat_activity where datname = 'orchestrator' and pid <> pg_backend_pid()")
[ -z "$busy" ] || { printf 'still connected; stop these clients first:\n%s\n' "$busy" >&2; exit 1; }
echo "  no *_restore or *_before_restore names, no open connections"

echo "== Phase 2: restore into fresh *_restore databases"
sql db agentforge postgres -c 'CREATE DATABASE agentforge_restore' -c 'CREATE DATABASE temporal_restore' -c 'CREATE DATABASE temporal_visibility_restore'
sql orchestrator-db orchestrator postgres -c 'CREATE DATABASE orchestrator_restore'
$COMPOSE_BACKUP run --rm --no-deps -T -e BACKUP_SET="$BACKUP_SET" --entrypoint sh backup -ec '
  for db in agentforge temporal temporal_visibility; do
    PGPASSWORD="$POSTGRES_PASSWORD" pg_restore -h db -U agentforge -d "${db}_restore" \
      --no-owner --single-transaction --exit-on-error "/backups/${db}_$BACKUP_SET.dump"
  done
  pg_restore --no-owner -f /tmp/orchestrator.sql "/backups/orchestrator_$BACKUP_SET.dump"
  sed -i "/^SET transaction_timeout = 0;$/d" /tmp/orchestrator.sql
  PGPASSWORD="$ORCHESTRATOR_DB_PASSWORD" psql -h orchestrator-db -U orchestrator -d orchestrator_restore \
    --single-transaction -v ON_ERROR_STOP=1 -q -o /dev/null -f /tmp/orchestrator.sql'
echo "  restored set $BACKUP_SET into the *_restore databases"

echo "== Phase 3: validate every restored database"
check() { local svc=$1 role=$2 db=$3 want=$4 what=$5 query=$6 got
  got=$(sql "$svc" "$role" "$db" -c "$query")
  [ "$got" -ge "$want" ] || { echo "  $db: $what = $got, expected at least $want; nothing was renamed" >&2; exit 1; }
  echo "  $db: $what = $got"; }
check db agentforge agentforge_restore 1 "applied migrations" "select count(*) from _sqlx_migrations"
check db agentforge agentforge_restore 3 "core tables" "select count(*) from information_schema.tables where table_schema = 'public' and table_name in ('organizations', 'users', 'agents')"
check orchestrator-db orchestrator orchestrator_restore 1 "applied migrations" "select count(*) from _sqlx_migrations"
check orchestrator-db orchestrator orchestrator_restore 3 "core tables" "select count(*) from information_schema.tables where table_schema = 'public' and table_name in ('workflows', 'tasks', 'participants')"
check db agentforge temporal_restore 1 "schema_version rows" "select count(*) from schema_version"
check db agentforge temporal_restore 3 "core tables" "select count(*) from information_schema.tables where table_schema = 'public' and table_name in ('namespaces', 'executions', 'history_node')"
check db agentforge temporal_visibility_restore 1 "schema_version rows" "select count(*) from schema_version"
check db agentforge temporal_visibility_restore 1 "core tables" "select count(*) from information_schema.tables where table_schema = 'public' and table_name = 'executions_visibility'"

echo "== Phase 4: promote (one transaction per database)"
promote() { sql "$1" "$2" postgres -v live="$3" -v old="${3}_before_restore" -v new="${3}_restore" <<'SQL'
SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'live') AS had_live \gset
BEGIN;
\if :had_live
ALTER DATABASE :"live" RENAME TO :"old";
\endif
ALTER DATABASE :"new" RENAME TO :"live";
COMMIT;
\if :had_live
\echo '  promoted' :new 'to' :live '- previous copy kept as' :old
\else
\echo '  promoted' :new 'to' :live '- there was no previous copy'
\endif
SQL
}
promote db agentforge agentforge
promote db agentforge temporal
promote db agentforge temporal_visibility
promote orchestrator-db orchestrator orchestrator

echo "== Phase 5: start the API, then Temporal, then the orchestrator"
$COMPOSE_PROD up -d --wait agentforge-server
$COMPOSE_PROD up -d --wait temporal
$COMPOSE_PROD up -d --wait orchestrator
curl -fsS http://localhost:4003/api/health; echo
curl -fsS -o /dev/null -w '  orchestrator readiness: HTTP %{http_code}\n' http://localhost:4010/health/ready
echo "restore of backup set $BACKUP_SET complete" )
```

If you use the scheduled backup, start it again: `$COMPOSE_BACKUP up -d backup`.

Phase 2 handles the orchestrator database differently on purpose. The backup
tools are PostgreSQL 18, and the bundled orchestrator database is PostgreSQL 16.
PostgreSQL 18 tools write a `SET transaction_timeout` line that version 16
rejects, so phase 2 removes that line and stops on any other error.

Success looks like this:

- Each phase prints its lines, phase 3 shows a count for every check, and phase
  4 prints one `promoted` line per database.
- The block ends with `restore of backup set <timestamp> complete`.
- `/api/health` prints `"status":"ready"`, and orchestrator readiness shows
  `HTTP 200`.

#### If the block stopped

- **Stopped in phase 1:** nothing was created or renamed. Fix what it printed
  and paste the block again. Start the services again if you stop here:
  `$COMPOSE_PROD up -d --wait agentforge-server temporal orchestrator`.
- **Stopped in phase 2 or 3:** your current databases are untouched. Nothing
  was renamed. Delete the restored copies, fix the cause, and paste the block
  again:

  ```bash
  $COMPOSE_PROD exec -T db psql -U agentforge -d postgres -v ON_ERROR_STOP=1 \
    -c 'DROP DATABASE IF EXISTS agentforge_restore' \
    -c 'DROP DATABASE IF EXISTS temporal_restore' \
    -c 'DROP DATABASE IF EXISTS temporal_visibility_restore'
  $COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -v ON_ERROR_STOP=1 \
    -c 'DROP DATABASE IF EXISTS orchestrator_restore'
  ```

- **Stopped in phase 4:** some databases may already be swapped. Each swap is
  one transaction, so every database is either fully swapped or untouched.
  List the names:

  ```bash
  $COMPOSE_PROD exec -T db psql -U agentforge -d postgres -At -c \
    "select datname from pg_database where datname ~ '^(agentforge|temporal|temporal_visibility)(_restore|_before_restore)?$' order by 1"
  $COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -At -c \
    "select datname from pg_database where datname ~ '^orchestrator(_restore|_before_restore)?$' order by 1"
  ```

  For each database `<db>`, read the result like this:

  - `<db>_restore` still exists: not swapped. Nothing to undo.
  - `<db>_restore` is gone and `<db>_before_restore` exists: swapped. To undo,
    rename back in the opposite order, in one transaction. Set the three
    variables for the database. Use `DB=orchestrator SERVICE=orchestrator-db
    ROLE=orchestrator` for the orchestrator, and `SERVICE=db ROLE=agentforge`
    for the others.

    ```bash
    DB=agentforge SERVICE=db ROLE=agentforge
    $COMPOSE_PROD exec -T "$SERVICE" psql -U "$ROLE" -d postgres -v ON_ERROR_STOP=1 \
      -c "ALTER DATABASE $DB RENAME TO ${DB}_restore; ALTER DATABASE ${DB}_before_restore RENAME TO $DB;"
    ```

  - `<db>_restore` is gone and there is no `<db>_before_restore`: swapped on a
    host that had no previous copy. To undo, run only the first rename:
    `ALTER DATABASE $DB RENAME TO ${DB}_restore;`.

  When every database is back to its state before the restore, the listing
  shows `<db>` and `<db>_restore` for a database that existed before, and only
  `<db>_restore` for one that did not. Then delete the `*_restore` copies as
  for phases 2 and 3. After that, only the `<db>` names that existed before
  remain; a database that did not exist before has neither name. Then remove
  the cause; the block prints it, for example a client that is still
  connected. Paste the block again, or start the services again.

- **Stopped in phase 5:** all four databases are already promoted. They passed
  the phase 3 checks, so they are valid. Your previous databases are kept,
  untouched, as `<db>_before_restore`; a database that did not exist before
  the restore has no such copy. Some services may be running. See which
  ones, and why the one that failed did not start:

  ```bash
  $COMPOSE_PROD ps agentforge-server temporal orchestrator
  $COMPOSE_PROD logs --tail 50 agentforge-server temporal orchestrator
  ```

  Then choose one of the two ways forward. Do not paste the whole block
  again; while `<db>_before_restore` names exist, phase 1 refuses to start.

  **Resume phase 5 (usual choice).** Choose this when the cause is outside the
  restored data, for example a missing `docker/.env` value, a port in use, a
  missing image, or a health check that timed out on a slow host. Fix the
  cause, then run these lines. They are safe to repeat: services that already
  run are left as they are.

  ```bash
  $COMPOSE_PROD up -d --wait agentforge-server
  $COMPOSE_PROD up -d --wait temporal
  $COMPOSE_PROD up -d --wait orchestrator
  curl -fsS http://localhost:4003/api/health; echo
  curl -fsS -o /dev/null -w '  orchestrator readiness: HTTP %{http_code}\n' http://localhost:4010/health/ready
  ```

  Success looks the same as for the whole block. Then run the
  [checks for skew](#after-the-restore-check-for-skew).

  **Go back to the databases from before the restore.** Choose this when the
  restored data is the cause and you cannot fix it, for example startup stops
  with `migration <N> was previously applied but is missing` and you cannot
  run the release the backup came from, or you decide not to restore after
  all. Anything the started services wrote since phase 5 began is dropped
  with the restored copies.

  1. Stop every service that writes to the databases first. A rename fails
     with `is being accessed by other users` while one is still connected:

     ```bash
     $COMPOSE_BACKUP stop agentforge-server orchestrator temporal backup
     ```

  2. Run the undo block from phase 4 once for each of the four databases,
     with `DB=agentforge`, `DB=temporal`, `DB=temporal_visibility` (all with
     `SERVICE=db ROLE=agentforge`), and
     `DB=orchestrator SERVICE=orchestrator-db ROLE=orchestrator`. A database
     without a `<db>_before_restore` copy had none before the restore; for it,
     run only the first rename, as described there. If a rename reports
     `is being accessed by other users`, that database was left unchanged: a
     session of a stopped service has not closed yet. Wait a few seconds and
     run the undo for that database again.
  3. Run the phase 4 name listing. No `<db>_before_restore` is left. A
     database that existed before the restore shows `<db>` and `<db>_restore`.
     A database that did not exist before shows only `<db>_restore`.
  4. Start the services with the resume lines above. They now run on your
     previous databases, exactly as before the restore; a database that did
     not exist before is absent again.
  5. When they are healthy, delete the `*_restore` copies with the phase 2 and
     3 cleanup block.

Remove the `*_before_restore` copies only when you will not go back: the
services run healthy on the restored databases, the skew checks are done, and a
few days of normal operation have passed. The command is under
[After the restore](#after-the-restore-check-for-skew). After you went back,
there are no `*_before_restore` copies; delete the `*_restore` copies instead.

#### After the restore: check for skew

What happens on its own: once a minute, the workflow execution reconciler
checks workflow rows that say `running` against Temporal, up to 50 rows a
minute, the least recently checked first. It starts with the orchestrator even
when Temporal is still down, and keeps trying to reach Temporal until it
answers. It never starts, cancels or signals a workflow, and it changes nothing
while Temporal cannot be asked. Each row it checks records the time
(`execution_checked_at`) and the answer (`execution_observation`: `running`,
`closed`, `missing` or `unverifiable`).

| Temporal says about the workflow's current execution | What the reconciler does |
| --- | --- |
| It is running (or paused) | Nothing. |
| The run the row recorded has finished | Sets the matching status (`completed`, `failed` or `cancelled`) with cause `reconciled_from_temporal`. |
| It does not exist, first time seen | Records `execution_missing_since`. The row stays `running`. |
| It does not exist, still after the grace period (default 10 minutes, `ORCHESTRATOR_WORKFLOW_RECONCILE_GRACE_SECS`) | Sets `failed` with cause `execution_lost`. The outcome is unknown, not a business failure: the run may have finished or never got far. |
| It does not exist, but the row cannot be proven to belong to this Temporal | Nothing; it is counted as `unverifiable`. |
| Temporal cannot be asked, or the answer is not understood | Nothing; the reconciler backs off. |

A "does not exist" answer only counts when the workflow was started in the
same Temporal namespace the orchestrator is connected to now; the orchestrator
records the namespace's server-assigned id when it starts a workflow. If you
restored the Temporal databases, that id is unchanged and repairs need no
action from you. Two cases need your confirmation, because a wrong Temporal
address and a real loss look the same:

- workflows started before the orchestrator recorded namespace ids, and
- every workflow, if Temporal was re-created instead of restored (a new
  namespace gets a new id).

Before you start, find the Temporal namespace the orchestrator uses. The
bundled Compose stack sets `ORCHESTRATOR_TEMPORAL_NAMESPACE=default` in
`docker/compose.yml`. For a custom deployment, read the value from the running
container:

```bash
$COMPOSE_PROD exec orchestrator printenv ORCHESTRATOR_TEMPORAL_NAMESPACE
```

Success looks like one line with the namespace name, for example `default`. The
commands below use `TEMPORAL_NAMESPACE`; set it to that value:

```bash
TEMPORAL_NAMESPACE=default   # the value printed above
```

The orchestrator reads its settings only when its container is created. After
you change a value in `docker/.env`, recreate the container with the `up`
command shown below; `restart` keeps the old value. The bundled stack runs one
orchestrator container. If your deployment runs several orchestrator replicas,
do steps 3 and 7 against every one of them.

To repair them:

1. **Check the connection.** The orchestrator must point at the Temporal you
   restored or re-created (`ORCHESTRATOR_TEMPORAL_HOST`,
   `ORCHESTRATOR_TEMPORAL_NAMESPACE`). Ask Temporal about one workflow you know
   was running:

   ```bash
   TEMPORAL_WORKFLOW_ID=orchestrator-WORKFLOW_ID   # orchestrator- plus a workflow id
   $COMPOSE_PROD exec temporal temporal workflow describe --address temporal-internal:7233 \
     --namespace "$TEMPORAL_NAMESPACE" --workflow-id "$TEMPORAL_WORKFLOW_ID"
   ```

   Success looks like the workflow's details. `not found` for every workflow
   you try means the orchestrator may point at the wrong Temporal: stop here
   and fix the connection first.

2. **Turn the acknowledgment on.** Add this line to `docker/.env`:

   ```bash
   ORCHESTRATOR_WORKFLOW_RECONCILE_TRUST_CURRENT_TEMPORAL=true
   ```

   Then recreate the orchestrator container so it reads the new value:

   ```bash
   $COMPOSE_PROD up -d --force-recreate --no-deps --wait orchestrator
   ```

   Success looks like the command finishing without an error, with the
   orchestrator container reported as healthy.

3. **Confirm the acknowledgment is on.** Ask the orchestrator:

   ```bash
   $COMPOSE_PROD exec orchestrator curl -s http://localhost:4010/health/ready
   ```

   Success looks like `"workflowReconcileTrustCurrentTemporal":true` in the
   output. If it says `false`, the container still runs with the old setting:
   repeat step 2. Do not go on until every orchestrator replica says `true`,
   because a replica without the acknowledgment keeps marking rows
   `unverifiable`.

4. **Note the cut-off time**, only after step 3 succeeded on every replica:

   ```bash
   CUTOFF=$(date -u '+%Y-%m-%d %H:%M:%S+00')
   echo "$CUTOFF"
   ```

   Success looks like the current time in UTC. Keep this shell open: the next
   step reads `CUTOFF`.

5. **Wait until the reconciler has finished.** Run this check every few
   minutes. It reads the cut-off time through `psql -v` and the query from
   standard input:

   ```bash
   $COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator \
     -v ON_ERROR_STOP=1 -v cutoff="$CUTOFF" <<'SQL'
   select
     count(*) filter (where execution_checked_at is null or execution_checked_at < :'cutoff')
       as not_checked_since_cutoff,
     count(*) filter (where execution_missing_since is not null)
       as waiting_out_grace,
     count(*) filter (where execution_observation = 'unverifiable' and execution_checked_at >= :'cutoff')
       as unverifiable_since_cutoff
   from workflows
   where status = 'running' and temporal_workflow_id is not null;
   SQL
   ```

   - `not_checked_since_cutoff`: running workflows the reconciler has not
     checked since the cut-off.
   - `waiting_out_grace`: workflows that are missing in Temporal and will be
     marked `failed` (`execution_lost`) when the grace period ends.
   - `unverifiable_since_cutoff`: workflows still reported as not provable
     after the cut-off. This should stay `0`. If it does not, a replica is
     still running without the acknowledgment: go back to step 3.

   Success looks like `0 | 0 | 0`. This takes at least the grace period
   (default 10 minutes), and longer when many workflows are running: the
   reconciler checks up to 50 per minute.

6. **Turn the acknowledgment off.** Remove the line from `docker/.env` (or set
   it to `false`) and recreate the container again:

   ```bash
   $COMPOSE_PROD up -d --force-recreate --no-deps --wait orchestrator
   ```

   Success looks like the command finishing without an error.

7. **Confirm the acknowledgment is off**, on every replica:

   ```bash
   $COMPOSE_PROD exec orchestrator curl -s http://localhost:4010/health/ready
   ```

   Success looks like `"workflowReconcileTrustCurrentTemporal":false`. The
   orchestrator also stops logging the acknowledgment warning, and `/metrics`
   shows `orchestrator_workflow_reconcile_trust_current_temporal 0`.

Rows that are still not provable without the acknowledgment (`unverifiable`):

```bash
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator -c \
  "select id, org_id, execution_checked_at from workflows
    where status = 'running' and execution_observation = 'unverifiable' order by execution_checked_at"
```

The reconciled list shows which workflows were repaired, when and why, and
which ones are waiting out the grace period:

```bash
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator -c \
  "select id, org_id, status, status_cause, execution_observation, execution_missing_since, reconciled_at
     from workflows
    where status_cause is not null or execution_missing_since is not null
    order by coalesce(reconciled_at, execution_missing_since) desc"
```

You can also see it in the orchestrator log (one `workflow reconciled from
Temporal` line per repaired workflow, and a warning when every workflow checked
in one pass has no execution) and in the `/metrics` counter
`orchestrator_workflow_reconcile_total{outcome=...}`. A workflow's status read
(`GET /api/v1/workflows/WORKFLOW_ID/status` on the orchestrator) returns its
`executionState` (`running`, `missing`, `closed`, `unverifiable` or `unknown`)
next to the stored cause fields.

Before you run a workflow marked `execution_lost` again, check Temporal first if
the namespace or address changed recently: when its real execution still runs
somewhere else, a re-run does the work twice. The same applies after a restore
from an older backup: steps that were running at backup time can run again.

The reconciler is on by default (`ORCHESTRATOR_WORKFLOW_RECONCILE_ENABLED`).

One check stays manual. It lists organization IDs used by orchestrator rows that the API
database does not have:

```bash
ORG_IDS=$($COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator -At -c \
  "select coalesce(string_agg(distinct org_id, ','), '') from (select org_id from workflows union select org_id from tasks union select org_id from participants union select org_id from code_reviews) s")
echo "select o as org_id_missing_in_api from unnest(string_to_array(:'ids', ',')) o where not exists (select 1 from organizations where id::text = o)" |
  $COMPOSE_PROD exec -T db psql -U agentforge -d agentforge -v ids="$ORG_IDS"
```

Orchestrator rows of an organization that no longer exists are not removed
automatically; fixing them is manual.

After a few days of normal operation, remove the old copies. This deletes them
permanently:

```bash
$COMPOSE_PROD exec -T db psql -U agentforge -d postgres -v ON_ERROR_STOP=1 \
  -c 'DROP DATABASE IF EXISTS agentforge_before_restore' \
  -c 'DROP DATABASE IF EXISTS temporal_before_restore' \
  -c 'DROP DATABASE IF EXISTS temporal_visibility_before_restore'
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -v ON_ERROR_STOP=1 \
  -c 'DROP DATABASE IF EXISTS orchestrator_before_restore'
```

**External databases (`make prod-ext`).** You own these backups. To restore
the same way, create an empty database with your provider. It must be owned by
the role in `DATABASE_URL` or `ORCHESTRATOR_DATABASE_URL`, and the API role must
be allowed to create the `pgcrypto` extension. Run the phase 2 restore commands
against it with your provider's host and user. Then point the URL at the new
database.

#### Schema versions at startup

- **Backup older than the running release:** the API and orchestrator apply
  the missing migrations forward when they start
  (`rust/bins/server/src/main.rs`, `run_migrations`;
  `rust/crates/orchestrator/src/state.rs`, `run_migrations`).
- **Backup newer than the running release:** startup stops with
  `migration <N> was previously applied but is missing in the resolved migrations`.
  The migrator does not ignore unknown applied migrations. Start the release the
  backup came from instead.
- **Temporal:** the `temporalio/auto-setup` image upgrades its schema forward
  on start and never downgrades it. Restore the Temporal databases only into the
  same Temporal server version, which means the same image tag.

### Restore rehearsal record

**2026-11-24, second round — mechanics rehearsal of interrupted backups, the
streamed skew check, and phase 5 recovery.** This is **not** an
application-level restore proof; that rehearsal is still open. It used no Forge
binaries, no Temporal server, and no real data.

- Revision: branch `fix/1325-1326-compose-backup-readiness` at the commit that
  adds this entry. The backup command was run as Compose renders it (rendered
  with a dummy env file made from `docker/.env.example`, entrypoint
  `/bin/sh -ec`, the same image). The restore block, the skew check, the
  organization check, and the phase 5 recovery blocks were run byte-identical
  from this guide; only the `BACKUP_SET=` and `DB=` lines were edited, as the
  guide says.
- Images: `postgres:18-alpine` (PostgreSQL 18.7, `sha256:0115267ab5a5…`) and
  `postgres:16-alpine` (PostgreSQL 16.16, `sha256:1758b8743084…`).
- Setup: plain `docker run` containers on one private internal network, no
  published ports, no Compose project. `$COMPOSE_PROD` and `$COMPOSE_BACKUP`
  were mapped to `docker exec` and `docker run`. The three services were
  stand-ins that hold one open database connection each, and the API stand-in
  writes one row when it starts. `curl` and the Temporal CLI printed `SKIPPED`.
  All four databases used small synthetic schemas, including the Temporal
  ones (unlike the first round).
- Results by case:
  - **Interrupted dump** (the `agentforge` dump held on a table lock, three
    dumps already in the staging folder): TERM, HUP, INT, and `docker stop`
    each ended the container in 0.6 to 1.5 s with exit code 143, 129, 130,
    and 130. Every time the staging folder was gone, the lock was free, no
    file was published, and the four published files were byte-identical
    (SHA-256) to before. The previous command, for comparison: `docker stop`
    took 10.6 s, ended with 137 (killed), and left its staging folder.
  - **Stop during the daily wait:** `docker stop` and TERM ended the container
    in 1.0 s (exit 130) and 0.8 s (exit 143); the previous command took 11.3 s
    (exit 137).
  - **Skew check with 5,200 unfinished workflow rows:** it printed `COPY 5200`
    and listed exactly the eight planted mismatches (two finished between the
    dumps, two stranded, two `draft` rows with a running execution, two older
    run IDs). The previous check failed on the same data with
    `Argument list too long`. With the orchestrator side failing, the check
    printed `the skew check did not finish`. The organization check listed the
    one planted orphan ID.
  - **Phase 5 failure** (the Temporal start replaced by a stub that fails):
    the block exited 1 after phase 4, with the API stand-in running. Resume:
    after removing the stub, the resume lines started Temporal and the
    orchestrator; a second run changed nothing. The databases kept the
    restored data and the `*_before_restore` copies. Go back: after a second
    restore that failed the same way, the stop line, the undo block once per
    database, and the name listing left `<db>` and `<db>_restore` for all
    four databases. All four were identical in data to before that restore
    (`pg_dump --data-only`, SHA-256); the row the API stand-in wrote went with
    the `*_restore` copies. The resume lines and the cleanup block then left
    the four original names and three running stand-ins.
- Guide fixes found by this round:
  - Docker stops this image with INT, not TERM, so a stop exits 130.
  - An undo run before the stand-ins' sessions closed failed with
    `is being accessed by other users` and left that database unchanged; the
    guide now says to wait and run that undo again.
- Follow-up, same day: a signal can arrive between starting a child and
  recording its process ID. A stub test in the backup image (`pg_dump`
  replaced by a stub that blocks; TERM at a random 8 to 20 ms after start,
  2,000 runs) failed the previous command: 10 surviving children, 2 wrong exit
  codes, and 2 staging folders left. The current command passed: 0, 0, and 0.
  The interrupted-dump case then passed again against real PostgreSQL: TERM
  exit 143 in 0.95 s, `docker stop` exit 130 in 0.73 s, staging gone, lock
  free, published set byte-identical.
- Not covered: that the real `docker compose up -d --wait` leaves a running
  service alone (Compose's documented behavior; the stand-in did it), and Ctrl+C
  on a manual run in a terminal.

**2026-11-24 — mechanics rehearsal with a synthetic schema, current
procedure.** This is **not** an application-level restore proof; that
rehearsal is still open. It used no Forge binaries, no Temporal server, and no
real data.

- Revision: branch `fix/1325-1326-compose-backup-readiness` at the commit that
  adds this entry. The backup command and these guide blocks were run as
  committed (byte-identical):
  - manual backup;
  - the five-phase restore block;
  - the phase 2/3 cleanup block, the phase 4 name listing, and the undo block;
  - both skew checks and the Temporal CLI line;
  - removal of old copies.

  The "Verify a backup set" block was not re-run in this round.
- Images: `postgres:18-alpine` (PostgreSQL 18.7, `sha256:0115267ab5a5…`) for
  the backup tools and the API server; `postgres:16-alpine` (PostgreSQL 16.16,
  `sha256:1758b8743084…`) for the orchestrator server.
- Setup:
  - Plain `docker run` containers on a private internal network, with no
    published ports and no Compose project.
  - `$COMPOSE_PROD` and `$COMPOSE_BACKUP` were mapped to `docker exec` and
    `docker run` of the same images. Compose lifecycle lines (`stop`, `up`),
    `curl`, and the Temporal CLI line printed `SKIPPED`.
  - The `temporal` and `temporal_visibility` databases used the real
    `schema.sql` files from the `temporalio/auto-setup:1.26` image, plus the
    `schema_version` table as `temporal-sql-tool` creates it.
  - The API and orchestrator databases used synthetic schemas with the object
    kinds of the real migrations, as in the earlier round.
- Results by case:
  - **Success path:** all five phases passed. Phase 3 printed a count for each
    of the eight checks, and phase 4 printed four `promoted` lines. Each
    restored database matched its `*_before_restore` original in
    `pg_dump --schema-only` (ignoring `\restrict` keys and the dump header),
    data, and per-table row counts.
  - **Corrupted archive:** the block stopped in phase 1 with
    `unreadable archive: agentforge_<set>.dump`. Nothing was created; names and
    row counts were unchanged.
  - **Phase 2 failure** (`db` unreachable before the restore): the block exited
    1 with `could not translate host name "db"`. No rename happened, and the
    live databases' row counts were unchanged. The cleanup block then returned
    names and counts to the exact starting state.
  - **Phase 4 failure:** a session was opened on `orchestrator` after the
    three `db`-server swaps. The block exited 3 with `database "orchestrator"
    is being accessed by other users`. `orchestrator` and `orchestrator_restore`
    were both intact, because that swap rolled back. The name listing showed
    three swapped databases. The undo block, once per swapped database, and
    then the cleanup block returned names and row counts to the exact state
    before the case.
  - **Two backup runs at once** (a scheduled pass and a one-shot, started
    together, each pass about 7 s): the scheduled pass logged `another backup
    run is in progress; waiting for it`. The one-shot exited 0. Each run
    published a complete set of four files with its own timestamp, no staging
    folder was deleted by the other run, and earlier sets were unchanged.
  - **Lock held by another run:** a one-shot with `BACKUP_LOCK_WAIT_SECS=10`
    exited 75 with `still holds /backups/.backup.lock after 10s; not started`.
    No file was added, removed, or changed.
  - **Dump failure mid-run** (`db` unreachable after the orchestrator dump):
    the run exited 1. It published nothing, left no staging folder, and earlier
    sets were unchanged.
  - **Skew check:** it listed exactly the four planted mismatches:
    - `running` with a `Completed` execution (finished between the dumps);
    - `running` with no execution (stranded);
    - `draft` with a `Running` execution (corrects itself);
    - `running` with an older run ID.

    It did not list the consistent `running` row or the never-started `draft`.
    The organization check listed the one planted orphan ID.
- Guide fix found by this round: phase 1 printed only `Segmentation fault`
  for a corrupted archive, because `pg_restore --list` crashes on it. Phase 1
  now names the unreadable archive.

**2026-11-23 — earlier mechanics rehearsal** (revision `914ea77c`; the
staging, lock, and phased restore described above replaced that procedure).
Backup and the earlier restore commands restored four synthetic databases with
matching schema, data, and counts. The PostgreSQL 18 archive's
`SET transaction_timeout = 0;` line failed on PostgreSQL 16 without the `sed`
line (exit 3). Failed backup runs exited 1 without publishing a set. That round
also found that a `<placeholder>` inside a command is shell redirection syntax.

Earlier drill, before Temporal and the orchestrator were covered: a backup,
wipe, and restore of the API database only, on a local stack with 1,000 tasks.
It used `pg_restore --clean --if-exists --no-owner`. Counts and the
`_sqlx_migrations` chain (86 migrations) matched.

### Redis Restore

```bash
# Stop Redis
$COMPOSE_PROD stop redis

# Replace dump file
cp /backup/redis-dump-YYYYMMDD.rdb /var/lib/redis/dump.rdb

# Start Redis
$COMPOSE_PROD start redis
```

### RustFS Restore

If you use the managed RustFS profile, start RustFS before mirroring objects.
The storage override activates services that depend on the `prod` profile, so
enable both profiles; naming only `rustfs` keeps the command scoped to storage.
Configure `mc` aliases in the current shell; `docker/.env` does not export
credentials into it:

```bash
$COMPOSE_PROD_STORAGE up -d --wait rustfs
mc alias set backup "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY"
mc alias set rustfs "$S3_ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY"
```

```bash
mc mirror --overwrite --preserve "backup/agentforge-YYYYMMDD" rustfs/agentforge
```

### Full System Rebuild

```bash
# 1. Start infrastructure and wait until it is healthy (not Temporal yet)
$COMPOSE_PROD up -d --wait db redis nats orchestrator-db
```

2. Copy the backup set into `docker/backups/`.
3. For managed RustFS storage, start RustFS and restore attachments first:

   ```bash
   $COMPOSE_PROD_STORAGE up -d --wait rustfs
   mc alias set backup "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY"
   mc alias set rustfs "$S3_ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY"
   mc mirror --overwrite --preserve "backup/agentforge-YYYYMMDD" rustfs/agentforge
   ```

   Then set `COMPOSE_PROD="$COMPOSE_PROD_STORAGE"` in the same shell, so the
   restore block starts the API with RustFS. For local attachment storage, skip
   this step.

4. Run the [PostgreSQL Restore](#postgresql-restore-all-bundled-databases)
   block. On a new host, phase 4 finds no old `temporal` databases and only
   promotes the restored copies. The empty `agentforge` database that the new
   `db` container created is kept as `agentforge_before_restore`.

---

## 3. Scenario Playbooks

### Database corruption

1. Stop the application services
2. Check PostgreSQL logs: `$COMPOSE_PROD logs db`
3. If recoverable: `pg_resetwal` or repair
4. If not recoverable: restore from latest backup (see above)
5. Run migrations to catch up
6. Restart application

### Redis failure

1. Redis is optional (circuit breaker handles degradation)
2. Restart Redis: `$COMPOSE_PROD restart redis`
3. Application will auto-reconnect
4. Cache will be cold — expect slower responses temporarily

### Container crash / agent failure

1. Check container status: `docker ps -a | grep agentforge`
2. View logs: `docker logs <container_id>`
3. Restart application services: `$COMPOSE_PROD restart agentforge-server orchestrator`
4. If persistent: `make prod-down && make prod`

### Full server loss

1. Provision new server with Docker
2. Clone repository: `git clone <repo-url>`
3. Restore `.env` configuration from secure storage
4. Run `make setup` for Docker networks
5. Follow "Full System Rebuild" above
6. Update DNS if IP changed

---

## 4. Health Check Commands

Liveness and readiness are explained in
[Deployment: Health Verification](deployment.md#health-verification).

```bash
# Liveness (process is running)
curl -s http://localhost:4003/health | jq .

# Readiness (PostgreSQL and NATS reachable; Compose health uses this)
curl -s http://localhost:4003/api/health | jq .

# Orchestrator health
curl -s http://localhost:4010/health | jq .

# Prometheus metrics (requires admin auth token)
curl -s -H "Authorization: Bearer <admin-token>" http://localhost:4003/metrics | head -20

# PostgreSQL connectivity
pg_isready -h localhost -U agentforge

# Redis connectivity
redis-cli -h localhost -a '<redis-password-from-docker-env>' ping

# Docker container status
$COMPOSE_PROD ps

# NATS connectivity
nats server check connection
```

---

## 5. Retention Policy

| Backup type                                              | Retention | Storage                  |
| -------------------------------------------------------- | --------- | ------------------------ |
| PostgreSQL daily (bundled `backup` service, 4 databases) | 7 days    | docker/backups/          |
| PostgreSQL weekly                                        | 90 days   | /backup/postgres/weekly/ |
| RustFS weekly                                            | 90 days   | /backup/rustfs/          |
| Redis RDB (if enabled)                                   | 7 days    | /backup/redis/           |
