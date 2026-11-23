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
day and deletes dumps older than 7 days. Each dump is written as
`.dump.partial` and renamed to `.dump` only after all four dumped. If any dump
fails, the service exits with the `pg_dump` error, restarts, and retries. It
does not publish a partial set and does not delete older backups after a
failure. Leftover `.dump.partial` files from a failed run are removed when the
next run starts.

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

It ends with `backup complete: <timestamp>` and exit code 0, or exits non-zero
with the `pg_dump` error. Manual sets follow the same 7-day retention.

**Verify a backup set:**

```bash
BACKUP_TS=YYYYMMDD_HHMMSS   # the timestamp of the set
$COMPOSE_BACKUP run --rm --no-deps -e BACKUP_TS="$BACKUP_TS" --entrypoint sh backup -ec '
  for db in orchestrator temporal temporal_visibility agentforge; do
    pg_restore --list "/backups/${db}_$BACKUP_TS.dump" >/dev/null
    echo "readable: ${db}_$BACKUP_TS.dump"
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
- **Skew you can see after a restore:**
  - A workflow started between the `orchestrator` dump and the `temporal`
    dump comes back as `draft` while Temporal still runs it. Its final status
    is written when the Temporal execution finishes.
  - An organization deleted between the `orchestrator` dump and the
    `agentforge` dump still has orchestrator rows.
    [The checks after a restore](#after-the-restore-check-for-skew) list them.
  - Steps that were running at backup time can run again after the restore,
    because Temporal retries steps that have no recorded result. An agent task
    can therefore run twice.
- **If you do not restore the Temporal databases** (for example they were lost,
  or you restore only `agentforge` and `orchestrator`), every workflow that was
  running is **stranded**. Its row says `running`, but no Temporal execution
  exists. Forge has no automatic or admin action that resolves this today:
  - cancel and review decisions fail with an internal error and leave the
    status unchanged;
  - a re-run is refused while the status is `running`;
  - no background job checks workflow rows.

  [The checks after a restore](#after-the-restore-check-for-skew) find
  stranded workflows. Resolving them is a known gap.

### PostgreSQL Restore (all bundled databases)

Prerequisites:

- A complete backup set: four files with one timestamp in `docker/backups/`.
- `docker/.env` has `POSTGRES_PASSWORD` and `ORCHESTRATOR_DB_PASSWORD` for the
  database servers you restore into.
- The same Forge release and the same `temporalio/auto-setup` image tag as when
  the backup was taken. Check with `$COMPOSE_PROD images`. See
  [Schema versions at startup](#schema-versions-at-startup).
- About 30 minutes during which the API, orchestrator, and Temporal are offline.

The restore never writes into the current databases. It restores into new,
empty `*_restore` databases, stops at the first error, and then swaps names.
The current databases are kept as `*_before_restore` until you remove them.

Restore order: restore all four databases while every service that uses them
is stopped. Then start the API first, Temporal next, and the orchestrator last.
The orchestrator is the only service that reads the other two: it calls the API
and runs the Temporal workflows.

```bash
BACKUP_TS=YYYYMMDD_HHMMSS   # timestamp of the backup set you chose

# 1. Stop everything that writes to the databases, plus the scheduled backup
$COMPOSE_BACKUP stop agentforge-server orchestrator temporal backup

# 2. Make sure both database servers are running
$COMPOSE_PROD up -d --wait db orchestrator-db

# 3. Create fresh, empty databases to restore into
$COMPOSE_PROD exec -T db psql -U agentforge -d postgres -v ON_ERROR_STOP=1 \
  -c 'CREATE DATABASE agentforge_restore' \
  -c 'CREATE DATABASE temporal_restore' \
  -c 'CREATE DATABASE temporal_visibility_restore'
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -v ON_ERROR_STOP=1 \
  -c 'CREATE DATABASE orchestrator_restore'

# 4. Restore each dump in one transaction that stops at the first error
$COMPOSE_BACKUP run --rm --no-deps -e BACKUP_TS="$BACKUP_TS" --entrypoint sh backup -ec '
  for db in agentforge temporal temporal_visibility; do
    PGPASSWORD="$POSTGRES_PASSWORD" pg_restore -h db -U agentforge -d "${db}_restore" \
      --no-owner --single-transaction --exit-on-error "/backups/${db}_$BACKUP_TS.dump"
  done
  pg_restore --no-owner -f /tmp/orchestrator.sql "/backups/orchestrator_$BACKUP_TS.dump"
  sed -i "/^SET transaction_timeout = 0;$/d" /tmp/orchestrator.sql
  PGPASSWORD="$ORCHESTRATOR_DB_PASSWORD" psql -h orchestrator-db -U orchestrator -d orchestrator_restore \
    --single-transaction -v ON_ERROR_STOP=1 -q -f /tmp/orchestrator.sql
  echo "restored set $BACKUP_TS"'

# 5. Check the restored copies before using them
$COMPOSE_PROD exec -T db psql -U agentforge -d agentforge_restore -c \
  "select (select count(*) from organizations) orgs, (select count(*) from _sqlx_migrations) migrations"
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator_restore -c \
  "select (select count(*) from workflows) workflows, (select count(*) from _sqlx_migrations) migrations"

# 6. Swap: keep the current databases as *_before_restore, promote the restored copies
$COMPOSE_PROD exec -T db psql -U agentforge -d postgres -v ON_ERROR_STOP=1 <<'SQL'
DO $$
DECLARE n text;
BEGIN
  FOREACH n IN ARRAY ARRAY['agentforge', 'temporal', 'temporal_visibility'] LOOP
    IF EXISTS (SELECT 1 FROM pg_database WHERE datname = n) THEN
      EXECUTE format('ALTER DATABASE %I RENAME TO %I', n, n || '_before_restore');
    END IF;
    EXECUTE format('ALTER DATABASE %I RENAME TO %I', n || '_restore', n);
  END LOOP;
END $$;
SQL
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -v ON_ERROR_STOP=1 <<'SQL'
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_database WHERE datname = 'orchestrator') THEN
    ALTER DATABASE orchestrator RENAME TO orchestrator_before_restore;
  END IF;
  ALTER DATABASE orchestrator_restore RENAME TO orchestrator;
END $$;
SQL

# 7. Start the services: API, then Temporal, then the orchestrator
$COMPOSE_PROD up -d --wait agentforge-server
$COMPOSE_PROD up -d --wait temporal
$COMPOSE_PROD up -d --wait orchestrator

# 8. If you use the scheduled backup, start it again
$COMPOSE_BACKUP up -d backup
```

Step 4 handles the orchestrator database differently on purpose. The backup
tools are PostgreSQL 18, and the bundled orchestrator database is PostgreSQL 16.
PostgreSQL 18 tools write a `SET transaction_timeout` line that version 16
rejects. Step 4 removes that line and stops on any other error.

If any of steps 3 to 6 fails, nothing has replaced your current databases.
Fix the cause, remove the `*_restore` databases with
`DROP DATABASE <name>_restore` (connected to `postgres`), and start again at
step 3. If step 6 stops because a `*_before_restore` database already exists
from an earlier restore, remove that one first.

Success looks like this:

- Step 4 prints `restored set <timestamp>` without `error:` lines. A one-row
  `set_config` result before it is normal output.
- Step 5 shows the counts you expect for the backup time.
- Step 7 returns once each service is healthy.
- `curl -s http://localhost:4003/api/health` returns `"status":"ready"`.
- `curl -s http://localhost:4010/health/ready` returns HTTP 200.

#### After the restore: check for skew

These checks are read-only. They find the skew described in
[What a restore recovers](#what-a-restore-recovers):

```bash
# Workflows the orchestrator thinks are running. For each one, ask Temporal:
# "not found" means the workflow is stranded.
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator -c \
  "select id, name, temporal_workflow_id, updated_at from workflows where status = 'running' order by updated_at"
TEMPORAL_WORKFLOW_ID=orchestrator-WORKFLOW_ID   # one temporal_workflow_id from the list above
$COMPOSE_PROD exec temporal temporal workflow describe --address temporal-internal:7233 \
  --namespace default --workflow-id "$TEMPORAL_WORKFLOW_ID"

# Organization IDs used by orchestrator rows that the API database does not have
ORG_IDS=$($COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d orchestrator -At -c \
  "select coalesce(string_agg(distinct org_id, ','), '') from (select org_id from workflows union select org_id from tasks union select org_id from participants union select org_id from code_reviews) s")
echo "select o as org_id_missing_in_api from unnest(string_to_array(:'ids', ',')) o where not exists (select 1 from organizations where id::text = o)" |
  $COMPOSE_PROD exec -T db psql -U agentforge -d agentforge -v ids="$ORG_IDS"
```

After a few days of normal operation, remove the old copies. This deletes them
permanently:

```bash
$COMPOSE_PROD exec -T db psql -U agentforge -d postgres -v ON_ERROR_STOP=1 \
  -c 'DROP DATABASE agentforge_before_restore' \
  -c 'DROP DATABASE IF EXISTS temporal_before_restore' \
  -c 'DROP DATABASE IF EXISTS temporal_visibility_before_restore'
$COMPOSE_PROD exec -T orchestrator-db psql -U orchestrator -d postgres -v ON_ERROR_STOP=1 \
  -c 'DROP DATABASE orchestrator_before_restore'
```

**External databases (`make prod-ext`).** You own these backups. To restore
the same way, create an empty database with your provider. It must be owned by
the role in `DATABASE_URL` or `ORCHESTRATOR_DATABASE_URL`, and the API role must
be allowed to create the `pgcrypto` extension. Run the step 4 commands against
it with your provider's host and user. Then point the URL at the new database.

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

**2026-11-23 — mechanics rehearsal with a synthetic schema.** This is **not**
an application-level restore proof; that rehearsal is still open. It used no
Forge binaries, no Temporal server, and no real data.

- Revision: branch `fix/1325-1326-compose-backup-readiness` on `35fdd5f8`
  plus the follow-up commit that adds this record. The backup command and the
  guide blocks were run as committed.
- Images: `postgres:18-alpine` (PostgreSQL 18.7, `sha256:0115267ab5a5…`) for
  the backup tools and the API server; `postgres:16-alpine` (PostgreSQL 16.16,
  `sha256:1758b8743084…`) for the orchestrator server.
- Setup: plain `docker run` containers on a private internal network, with no
  published ports and no Compose project. The `db` server held `agentforge`,
  `temporal`, and `temporal_visibility`; `orchestrator-db` held `orchestrator`.
  The schemas used the object kinds of the real migrations: `pgcrypto`, an
  enum type, extra schemas, CHECK and circular foreign keys, partial unique and
  GIN indexes, `jsonb`, `text[]`, `inet`, `bytea`, plpgsql trigger functions, a
  materialized view with a unique index, and `_sqlx_migrations`. Each had a few
  rows.
- Backup: the `backup` service command exactly as `docker compose config`
  renders it for `prod` + `backup`, in the same image, with `BACKUP_ONCE=1`.
  Result: `backup complete:` and four `.dump` files, exit 0.
- Restore: the blocks under
  [PostgreSQL Restore](#postgresql-restore-all-bundled-databases) run verbatim
  in a shell, with `$COMPOSE_PROD` / `$COMPOSE_BACKUP` mapped to `docker exec`
  / `docker run` of the same images. Compose lifecycle lines (`stop`, `up`) and
  the Temporal CLI line were not exercised.
- Results:
  - All four databases restored into `*_restore` and were swapped.
  - `pg_dump --schema-only` of each restored database matched the original,
    ignoring only `\restrict` keys and the dump header.
  - Per-table row counts and data matched.
  - The reconciliation query listed the one synthetic orphan organization ID,
    and the running-workflow query listed the one running workflow.
  - The PostgreSQL 18 archive contained `SET transaction_timeout = 0;`. Without
    the `sed` line, PostgreSQL 16 failed with `unrecognized configuration
    parameter "transaction_timeout"` (exit 3).
  - A second restore with no existing `temporal` databases, which is the
    new-host case, only promoted the restored copies.
- Failure: with `orchestrator-db` unreachable, the run exited 1 with
  `could not translate host name "orchestrator-db"`. With `db` unreachable
  after the orchestrator dump, the run exited 1. Both times no new `.dump` was
  published, and the earlier set's SHA-256 sums were unchanged. The next
  successful run removed the leftover `.partial` files.
- Guide fix found by the rehearsal: a `<placeholder>` inside a command is shell
  redirection syntax. Pasting the block failed with a syntax error, so the
  Temporal check now uses a `TEMPORAL_WORKFLOW_ID=` line.

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

2. Copy the backup set into `docker/backups/`. Then run steps 3 to 7 of
   [PostgreSQL Restore](#postgresql-restore-all-bundled-databases). On a new
   host, step 6 finds no old `temporal` databases and only promotes the
   restored copies.

For managed RustFS storage, start RustFS and restore attachments before starting
the API:

```bash
$COMPOSE_PROD_STORAGE up -d --wait rustfs
mc alias set backup "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY"
mc alias set rustfs "$S3_ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY"
mc mirror --overwrite --preserve "backup/agentforge-YYYYMMDD" rustfs/agentforge
```

Then start application services with the storage override so the API uses
RustFS:

```bash
$COMPOSE_PROD_STORAGE up -d --wait agentforge-server orchestrator
```

For local attachment storage, skip the RustFS restore above and start the API
with the regular production profile:

```bash
$COMPOSE_PROD up -d --wait agentforge-server orchestrator
```

Verify health:

```bash
curl -s http://localhost:4003/health | jq .
curl -s http://localhost:4003/api/health | jq .
curl -s http://localhost:4010/health | jq .
```

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

```bash
# Application health
curl -s http://localhost:4003/health | jq .

# Readiness (database connectivity)
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
