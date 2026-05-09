# Getting Started

This guide gets a developer workstation onto the current Rust-first runtime path.

## Choose a Local Mode

| Mode          | Command Path                            | Includes                                                                      | Best For                                                   |
| ------------- | --------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Full platform | `make setup`, `make dev`, `npm run dev` | Rust API, Rust orchestrator, Temporal, PostgreSQL, Redis, NATS, Vite frontend | Daily development, workflow work, end-to-end verification  |
| API-only loop | `npm run server`, `npm run dev`         | Rust API and Vite frontend only                                               | UI or API work that does not need orchestrator or Temporal |

The full platform mode is the default recommendation.

## Prerequisites

- Node.js 24+
- Docker and Docker Compose v2
- Make
- Git

## 1. Clone the Repository

```bash
git clone https://github.com/Wisdoverse/Wisdoverse-Forge.git wisdoverse-forge
cd wisdoverse-forge
npm install
```

## 2. Configure the Docker Environment

```bash
cp docker/.env.example docker/.env
```

Set at least these values in `docker/.env` before starting the backend stack:

- `POSTGRES_PASSWORD`
- `REDIS_PASSWORD`
- `JWT_SECRET`
- NATS callout variables listed in the [NATS Auth Runbook](../runbooks/nats-auth.md)

`MCP_TOKEN` can be set explicitly as well. If omitted, the default Compose value is used for local development.

## 3. Start the Backend Stack

```bash
make setup
make dev
```

This starts the default backend services for local development:

- Rust API on `:4003`
- Rust orchestrator on `:4010`
- Temporal on `:7233` with UI on `:8233`
- PostgreSQL, Redis, and NATS

## 4. Start the Frontend

Open a second terminal in the repository root and run:

```bash
npm run dev
```

The Vite frontend will be available on `http://localhost:5173`.

On a fresh database, open the frontend and complete the web setup/register flow
to create the initial owner/admin account. The example environment files do not
define a default application administrator.

## 5. Verify the Environment

```bash
curl http://localhost:4003/health
curl http://localhost:4010/health
```

Expected results:

- Rust API returns a healthy liveness response.
- Rust orchestrator returns `{"status":"healthy"}`.

Then open:

- `http://localhost:5173` for the frontend
- `http://localhost:8233` for Temporal UI

## 6. Optional Lightweight API Loop

If you only need the Rust API and frontend, you can skip Compose and run:

```bash
npm run server
npm run dev
```

This loop does not provide the orchestrator, Temporal, or the full backend integration stack. Use it only when those dependencies are not part of your change.

## Common Issues

- Missing passwords or tokens in `docker/.env` will cause Compose startup failures.
- If ports `4003`, `4010`, `5173`, `5432`, `6379`, or `8233` are already in use, stop the conflicting process or change the mapped ports.
- If Docker is unavailable, internal MCP and container-backed agent features will not work.

## Next Reads

- [Configuration Guide](configuration.md) for runtime variables.
- [Architecture Overview](../architecture/overview.md) for service boundaries and flow diagrams.
- [Deployment Guide](deployment.md) for production-oriented Compose usage.
