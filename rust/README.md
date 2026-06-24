# Wisdoverse Forge Rust Backend

## Quick Start

```bash
cd rust
cargo check          # Verify compilation
cargo test           # Run all tests
cargo build --release # Build release binaries
```

## Project Structure

| Crate    | Purpose                                        |
| -------- | ---------------------------------------------- |
| core     | Shared types, errors, config, TenantScope      |
| db       | SQLx pool, migrations, entity structs          |
| auth     | JWT, Argon2, AuthUser middleware               |
| infra    | Redis + NATS clients (graceful degradation)    |
| api      | Axum router, routes, services, repositories    |
| platform | Docker container management, security, pool    |
| jobs     | PostgreSQL task queue (FOR UPDATE SKIP LOCKED) |
| llm      | Multi-provider LLM gateway                     |
| server   | Main server binary                             |
| sidecar  | Agent container sidecar                        |

## Development

```bash
make ci              # Run full CI check locally
make fmt             # Format code
make clippy          # Lint
make audit           # Security audit
```

## Environment Variables

Required:

- `DATABASE_URL` -- PostgreSQL connection string
- `JWT_SECRET` -- JWT signing key (>= 32 chars)

Optional:

- `REDIS_URL` -- Redis connection (graceful degradation if absent)
- `NATS_URL` -- NATS connection (graceful degradation if absent)
- `PORT` -- Server port (default: 4003)
- `HOST` -- Bind address (default: 0.0.0.0)
- `LOG_LEVEL` -- Log level (default: info)
- `CORS_ORIGIN` -- Allowed CORS origin (required in production)

## Docker

```bash
make docker          # Build server + sidecar images
```

## Tests

197 tests covering:

- Security policy enforcement
- Error handling (3-layer system)
- JWT/Argon2 auth
- Tenant isolation (TenantScope)
- HMAC message signing
- WAL buffer resilience
- Type system invariants
