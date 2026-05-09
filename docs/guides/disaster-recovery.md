# Disaster Recovery Runbook

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
# Stop the application
docker compose -f docker/compose.yml stop server

# Restore from backup
pg_restore -h localhost -U agentforge -d agentforge \
  --clean --if-exists --no-owner \
  /backup/agentforge-YYYYMMDD-HHMM.dump

# Run pending migrations
npm run migrate

# Start the application
docker compose -f docker/compose.yml start server

# Verify
curl -s http://localhost:4003/health | jq .
```

### Redis Restore

```bash
# Stop Redis
docker compose -f docker/compose.yml stop redis

# Replace dump file
cp /backup/redis-dump-YYYYMMDD.rdb /var/lib/redis/dump.rdb

# Start Redis
docker compose -f docker/compose.yml start redis
```

### MinIO Restore

```bash
mc mirror /backup/minio-agentforge-YYYYMMDD/ minio/agentforge
```

### Full System Rebuild

```bash
# 1. Start infrastructure
docker compose -f docker/compose.yml up -d postgres redis minio nats

# 2. Wait for PostgreSQL
until pg_isready -h localhost -U agentforge; do sleep 1; done

# 3. Restore database
pg_restore -h localhost -U agentforge -d agentforge \
  --clean --if-exists --no-owner /backup/agentforge-latest.dump

# 4. Run migrations
npm run migrate

# 5. Restore attachments
mc mirror /backup/minio-agentforge-latest/ minio/agentforge

# 6. Start application
docker compose -f docker/compose.yml up -d server

# 7. Verify health
curl -s http://localhost:4003/health | jq .
curl -s http://localhost:4003/api/health | jq .
```

---

## 3. Scenario Playbooks

### Database corruption

1. Stop the application server
2. Check PostgreSQL logs: `docker logs agentforge-postgres`
3. If recoverable: `pg_resetwal` or repair
4. If not recoverable: restore from latest backup (see above)
5. Run migrations to catch up
6. Restart application

### Redis failure

1. Redis is optional (circuit breaker handles degradation)
2. Restart Redis: `docker compose restart redis`
3. Application will auto-reconnect
4. Cache will be cold — expect slower responses temporarily

### Container crash / agent failure

1. Check container status: `docker ps -a | grep agentforge`
2. View logs: `docker logs <container_id>`
3. Restart: `docker compose restart server`
4. If persistent: `docker compose down && docker compose up -d`

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

# Prometheus metrics (requires admin auth token)
curl -s -H "Authorization: Bearer <admin-token>" http://localhost:4003/metrics | head -20

# PostgreSQL connectivity
pg_isready -h localhost -U agentforge

# Redis connectivity
redis-cli ping

# Docker container status
docker compose -f docker/compose.yml ps

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
