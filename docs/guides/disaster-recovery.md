# Disaster Recovery Runbook

This runbook is written for operators restoring a self-contained production
deployment started with `make prod`, `make prod-pull`, `make prod-backup`, or
`make prod-storage`.

If your deployment uses `make prod-ext`, PostgreSQL and Redis are external
services. Restore those systems with your provider's runbook, then use this
document only for the Forge services, object storage, and health checks.

Before you start:

1. Confirm you are on the server that owns the deployment.
2. Confirm `docker/.env` is present and restored from secure storage.
3. Run commands from the repository root.
4. Set this helper once per shell session:

```bash
COMPOSE_PROD="docker compose --env-file docker/.env -f docker/compose.yml -f docker/compose.prod.yml --profile prod"
```

The examples below use Compose service names from `docker/compose.yml`: `db`,
`redis`, `nats`, `agentforge-server`, `orchestrator`, `orchestrator-db`, and
`temporal`. Avoid old container nicknames such as `server` or `postgres`; those
are not service names in the current Compose files.

## RTO/RPO Targets

| Component                          | RPO (Recovery Point Objective) | RTO (Recovery Time Objective) |
| ---------------------------------- | ------------------------------ | ----------------------------- |
| PostgreSQL (agents, events, users) | 1 hour                         | 30 minutes                    |
| Redis (cache, pub/sub)             | N/A (ephemeral)                | 5 minutes (restart)           |
| MinIO (attachments)                | 24 hours                       | 2 hours                       |
| NATS (event stream)                | N/A (transient)                | 5 minutes (restart)           |
| Full system                        | 1 hour                         | 4 hours                       |

---

## 1. Backup Procedures

### PostgreSQL

**Automated daily backup (cron):**

```bash
# /etc/cron.d/agentforge-backup
0 2 * * * root pg_dump -h localhost -U agentforge agentforge \
  --format=custom --compress=9 \
  -f /backup/agentforge-$(date +\%Y\%m\%d-\%H\%M).dump 2>&1 | logger -t agentforge-backup
```

**Manual backup:**

```bash
pg_dump -h localhost -U agentforge agentforge \
  --format=custom --compress=9 \
  -f /backup/agentforge-manual-$(date +%Y%m%d-%H%M).dump
```

**Verify backup integrity:**

```bash
pg_restore --list /backup/agentforge-YYYYMMDD-HHMM.dump | head -20
```

### Redis

Redis is used for caching and pub/sub — data is ephemeral. No backup required.
If persistence is enabled (RDB/AOF), copy the dump file:

```bash
redis-cli BGSAVE
cp /var/lib/redis/dump.rdb /backup/redis-dump-$(date +%Y%m%d).rdb
```

### MinIO (Attachments)

```bash
# Using mc (MinIO client)
mc mirror minio/agentforge /backup/minio-agentforge-$(date +%Y%m%d)/
```

### NATS

NATS JetStream data is transient. If persistence is needed, configure JetStream file storage and back up the data directory:

```bash
cp -r /var/lib/nats/jetstream /backup/nats-jetstream-$(date +%Y%m%d)/
```

---

## 2. Restore Procedures

### PostgreSQL Restore

```bash
# Stop services that write to PostgreSQL
$COMPOSE_PROD stop agentforge-server orchestrator

# Restore from backup
pg_restore -h localhost -U agentforge -d agentforge \
  --clean --if-exists --no-owner \
  /backup/agentforge-YYYYMMDD-HHMM.dump

# Run pending migrations
npm run migrate

# Start the application services again
$COMPOSE_PROD start agentforge-server orchestrator

# Verify
curl -s http://localhost:4003/health | jq .
curl -s http://localhost:4010/health | jq .
```

### Redis Restore

```bash
# Stop Redis
$COMPOSE_PROD stop redis

# Replace dump file
cp /backup/redis-dump-YYYYMMDD.rdb /var/lib/redis/dump.rdb

# Start Redis
$COMPOSE_PROD start redis
```

### MinIO Restore

If you use the managed MinIO profile, start MinIO before mirroring objects:

```bash
docker compose --env-file docker/.env -f docker/compose.yml --profile storage up -d minio
```

```bash
mc mirror /backup/minio-agentforge-YYYYMMDD/ minio/agentforge
```

### Full System Rebuild

```bash
# 1. Start infrastructure
$COMPOSE_PROD up -d db redis nats orchestrator-db temporal

# 2. Wait for PostgreSQL
until pg_isready -h localhost -U agentforge; do sleep 1; done

# 3. Restore database
pg_restore -h localhost -U agentforge -d agentforge \
  --clean --if-exists --no-owner /backup/agentforge-latest.dump

# 4. Run migrations
npm run migrate

# 5. Restore attachments if you use managed MinIO storage
docker compose --env-file docker/.env -f docker/compose.yml --profile storage up -d minio
mc mirror /backup/minio-agentforge-latest/ minio/agentforge

# 6. Start application services
$COMPOSE_PROD up -d agentforge-server orchestrator

# 7. Verify health
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

| Backup type            | Retention | Storage                  |
| ---------------------- | --------- | ------------------------ |
| PostgreSQL daily       | 30 days   | /backup/postgres/        |
| PostgreSQL weekly      | 90 days   | /backup/postgres/weekly/ |
| MinIO weekly           | 90 days   | /backup/minio/           |
| Redis RDB (if enabled) | 7 days    | /backup/redis/           |
