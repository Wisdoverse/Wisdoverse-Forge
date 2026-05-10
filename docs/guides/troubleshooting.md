# Troubleshooting Guide

This guide covers common issues on the current Rust-first runtime path.

## Quick Health Checks

```bash
curl http://localhost:4003/health
curl http://localhost:4003/api/health
curl http://localhost:4010/health
```

If you use the full local stack, also verify Temporal UI at `http://localhost:8233`.

## Core Service Logs

```bash
docker compose logs agentforge-server
docker compose logs orchestrator
docker compose logs temporal
docker compose logs db
docker compose logs redis
docker compose logs nats
```

## Common Problems

### Backend stack will not start

Check `docker/.env` first. Missing `POSTGRES_PASSWORD`, `REDIS_PASSWORD`,
`JWT_SECRET`, or the NATS callout values listed in
the [NATS Auth Runbook](../runbooks/nats-auth.md) will usually block startup.

Then verify ports:

```bash
lsof -i :4003
lsof -i :4010
lsof -i :5173
lsof -i :7233
```

### Frontend is not available

`make dev` does not start Vite. Start the frontend separately:

```bash
npm run dev
```

### Rust API healthy, orchestrator unhealthy

Check orchestrator configuration and dependencies:

```bash
docker compose logs orchestrator
```

Common causes:

- missing `ORCHESTRATOR_DATABASE_URL` wiring,
- invalid internal token configuration,
- Temporal enabled but unavailable,
- MCP endpoint or token mismatch.

### Workflow endpoints return Temporal errors

Verify:

- `ORCHESTRATOR_TEMPORAL_ENABLED=true` in the intended environment,
- Temporal is reachable at `ORCHESTRATOR_TEMPORAL_HOST`,
- `ORCHESTRATOR_MCP_TOKEN` is set,
- the Rust API MCP bridge is enabled and reachable.

In the default Compose path, those values are wired for you. Failures usually point to service startup or network issues.

In the external profile, also check:

```bash
docker exec agentforge-temporal temporal operator cluster health --address temporal-internal:7233
docker exec agentforge-server getent hosts temporal-internal
```

If the first command does not return `SERVING`, or `temporal-internal` resolves to an unexpected address inside the Rust container, the issue is usually Compose network wiring rather than application code.

### Agent execution fails in workflow activities

Check the Rust API logs and Docker availability. The internal MCP bridge requires Docker access and a valid `MCP_TOKEN`.

```bash
docker compose logs agentforge-server
docker info
```

### Realtime events are missing

Verify NATS and the backend consumers:

```bash
docker compose logs nats
docker compose logs agentforge-server
```

For the default callout-auth deployment, `NATS_URL` should use backend user
credentials, for example `nats://backend:<password>@nats:4222`, or remain unset
so Compose derives it from `NATS_BACKEND_PASSWORD`.

You can confirm the Rust client is attached with:

```bash
docker exec agentforge-nats wget -qO- http://127.0.0.1:8222/connz?subs=1
```

If `connz` shows `num_connections: 0`, check `NATS_URL`,
`NATS_BACKEND_PASSWORD`, `NATS_AUTH_SERVICE_PASSWORD`, and the Rust startup logs
first.

If the browser is connected but no live updates arrive, inspect `/ws` traffic and confirm the frontend is pointed at the correct API base.

### Database or migration failures

Verify database reachability and rerun migrations through the supported entry point:

```bash
npm run migrate
```

## Related Guides

- [Configuration Guide](configuration.md)
- [Deployment Guide](deployment.md)
- [Architecture Overview](../architecture/overview.md)
