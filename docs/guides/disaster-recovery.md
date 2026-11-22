# Disaster Recovery Runbook

This runbook is written for operators restoring a self-contained production
deployment started with `make prod`, `make prod-pull`, `make prod-backup`, or
`make prod-storage`.

If your deployment uses `make prod-ext`, PostgreSQL and Redis are external
services. That includes both databases: the API database (`DATABASE_URL`) and
the orchestrator database (`ORCHESTRATOR_DATABASE_URL`). The bundled `backup`
service does not cover external databases; you own their backups. Do not
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
| Redis (cache, pub/sub)             | N/A (ephemeral)                | 5 minutes (restart)           |
| RustFS (attachments)               | 24 hours                       | 2 hours                       |
| NATS (event stream)                | N/A (transient)                | 5 minutes (restart)           |
| Full system                        | 1 hour                         | 4 hours                       |

---

## 1. Backup Procedures

### PostgreSQL (both databases)

The self-contained stack has two PostgreSQL databases. Back up both, and keep
each pair together: a restore needs both files from the same run.

| Database     | Compose service   | Holds                                          | Backup file                           |
| ------------ | ----------------- | ---------------------------------------------- | ------------------------------------- |
| API          | `db`              | Users, organizations, agents, events, API data | `agentforge_<YYYYMMDD_HHMMSS>.dump`   |
| Orchestrator | `orchestrator-db` | Workflows, reviews, tasks, audit records       | `orchestrator_<YYYYMMDD_HHMMSS>.dump` |

Prerequisites: the stack runs with `make prod`, `make prod-pull`, or
`make prod-backup`, and `docker/.env` has `POSTGRES_PASSWORD` and
`ORCHESTRATOR_DB_PASSWORD`. Temporal's own databases on the `db` server are not
part of these dumps.

**Automated daily backup (recommended):**

```bash
make prod-backup
```

The `backup` service dumps both databases into `docker/backups/` once a day
and deletes dumps older than 7 days. Each dump is written as `.dump.partial`
and renamed to `.dump` only after both databases dumped. If either dump fails,
the service exits with the `pg_dump` error, restarts, and retries; it does not
delete older backups after a failure.

Success looks like this: the log shows `backup complete:` and the folder holds a
pair with the same timestamp.

```bash
$COMPOSE_BACKUP logs --tail 20 backup
ls -lt docker/backups | head -5
```

If `$COMPOSE_BACKUP ps backup` shows `Restarting`, read the log. It names the
database that failed. Copy `docker/backups/` to storage off this host; the
folder is lost with the server.

**Manual backup (both databases, same timestamp):**

```bash
$COMPOSE_BACKUP run --rm --no-deps --entrypoint sh backup -ec '
  ts=$(date +%Y%m%d_%H%M%S)
  PGPASSWORD="$POSTGRES_PASSWORD" pg_dump -h db -U agentforge -d agentforge -F c -f "/backups/agentforge_$ts.dump"
  PGPASSWORD="$ORCHESTRATOR_DB_PASSWORD" pg_dump -h orchestrator-db -U orchestrator -d orchestrator -F c -f "/backups/orchestrator_$ts.dump"
  ls -l "/backups/agentforge_$ts.dump" "/backups/orchestrator_$ts.dump"'
```

The single quotes keep the passwords inside the container; they come from
`docker/.env` and never appear in shell history.

**Verify backup integrity:**

```bash
$COMPOSE_BACKUP run --rm --no-deps --entrypoint pg_restore backup --list /backups/agentforge_YYYYMMDD_HHMMSS.dump | head -20
$COMPOSE_BACKUP run --rm --no-deps --entrypoint pg_restore backup --list /backups/orchestrator_YYYYMMDD_HHMMSS.dump | head -20
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

### PostgreSQL Restore (both databases)

Prerequisites:

- A backup pair with one timestamp, such as
  `docker/backups/agentforge_20260101_020000.dump` and
  `docker/backups/orchestrator_20260101_020000.dump`.
- `docker/.env` has the passwords of the databases you restore into.
- About 30 minutes during which the API and orchestrator are offline.

Restore order: **API database first, then the orchestrator database.**
Orchestrator rows point at API organizations, agents, and users by ID, and the
orchestrator calls the API when it starts. With the API restored first, those
IDs exist again when the orchestrator comes back. Use the pair from one run so
the two databases agree.

```bash
BACKUP_TS=YYYYMMDD_HHMMSS   # timestamp shared by the two files you chose

# 1. Stop everything that writes to the databases, plus the scheduled backup
$COMPOSE_BACKUP stop agentforge-server orchestrator backup

# 2. Make sure both databases are running
$COMPOSE_PROD up -d --wait db orchestrator-db

# 3. Restore the API database
$COMPOSE_BACKUP run --rm --no-deps -e BACKUP_TS="$BACKUP_TS" --entrypoint sh backup -ec '
  PGPASSWORD="$POSTGRES_PASSWORD" pg_restore -h db -U agentforge -d agentforge \
    --clean --if-exists --no-owner "/backups/agentforge_$BACKUP_TS.dump"'

# 4. Restore the orchestrator database in one transaction
$COMPOSE_BACKUP run --rm --no-deps -e BACKUP_TS="$BACKUP_TS" --entrypoint sh backup -ec '
  pg_restore --clean --if-exists --no-owner -f /tmp/orchestrator.sql "/backups/orchestrator_$BACKUP_TS.dump"
  sed -i "/^SET transaction_timeout = 0;$/d" /tmp/orchestrator.sql
  PGPASSWORD="$ORCHESTRATOR_DB_PASSWORD" psql -h orchestrator-db -U orchestrator -d orchestrator \
    --single-transaction -v ON_ERROR_STOP=1 -q -f /tmp/orchestrator.sql'

# 5. Start the application; both services run pending migrations at startup
$COMPOSE_PROD up -d --wait agentforge-server orchestrator

# 6. If you use the scheduled backup, start it again
$COMPOSE_BACKUP up -d backup
```

Step 4 differs from step 3 on purpose. The backup tools are PostgreSQL 18 and
the bundled orchestrator database is PostgreSQL 16. PostgreSQL 18 tools emit a
`SET transaction_timeout` line that version 16 rejects. The step removes that
line and stops on any other error. Nothing is changed unless the whole restore
succeeds.

Success looks like this:

- Steps 3 and 4 finish without `error:` lines, and step 5 returns once both
  services are healthy.
- `curl -s http://localhost:4003/api/health` returns `"status":"ready"`.
- `curl -s http://localhost:4010/health/ready` returns HTTP 200.
- Row counts match what you expect from the backup time:

```bash
$COMPOSE_PROD exec db psql -U agentforge -d agentforge -c \
  "select (select count(*) from orchestration_tasks) tasks,
          (select count(*) from _sqlx_migrations) migrations"
$COMPOSE_PROD exec orchestrator-db psql -U orchestrator -d orchestrator -c \
  "select count(*) as migrations from _sqlx_migrations"
```

> **Drill status.** An earlier drill covered only the API database. It used a
> local stack with a 1,000-task dataset, a custom-format dump of about 290 KB,
> and `pg_restore --clean --if-exists --no-owner`. After restore, task, org,
> and user counts and the `_sqlx_migrations` chain (86 migrations) matched the
> pre-disaster state. "Schema already exists" notices are harmless. The
> two-database procedure above has **not** been rehearsed yet. Record the first
> rehearsal here, with date, revision, and observed counts.

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
# 1. Start infrastructure and wait until it is healthy
$COMPOSE_PROD up -d --wait db redis nats orchestrator-db temporal
```

2. Copy the backup pair into `docker/backups/`. Then run steps 3 and 4 of
   [PostgreSQL Restore](#postgresql-restore-both-databases): the API database
   first, then the orchestrator database.

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

| Backup type                                                 | Retention | Storage                  |
| ----------------------------------------------------------- | --------- | ------------------------ |
| PostgreSQL daily (both databases, bundled `backup` service) | 7 days    | docker/backups/          |
| PostgreSQL weekly                                           | 90 days   | /backup/postgres/weekly/ |
| RustFS weekly                                               | 90 days   | /backup/rustfs/          |
| Redis RDB (if enabled)                                      | 7 days    | /backup/redis/           |
