# Wisdoverse Forge

Wisdoverse Forge is a self-hosted governed AI workbench for teams.

It turns work into auditable tasks, runs, and evidence; operates AI agent
runtimes through isolated sessions and workflows; and provides the foundation
for reusable context, memory, skills, permissions, and runtime-aware execution
across LLMs and Container CLIs.

The product focus is repeatable, governed work rather than a developer-only
kanban or a single-vendor memory store. Wisdoverse Forge is designed so the same kind
of task can be done again with approved context, visible provenance, and
revocable reuse.

> [!WARNING]
> Wisdoverse Forge is an engineering preview for trusted self-hosted environments. The
> active backend is Rust-owned; legacy TypeScript server paths are not part of
> the current runtime.

## Running Wisdoverse Forge

### Requirements

- Docker and Docker Compose v2
- Node.js 24+
- Make
- Git
- Enough local resources for PostgreSQL, Redis, NATS, Temporal, and
  Docker-backed agent sessions

### Option 1. Build a compatible service

Use the service contract as the implementation target:

> Implement a Wisdoverse Forge-compatible service according to `SPEC.md`.

### Option 2. Run this implementation

Start with [Getting Started](docs/guides/getting-started.md), then use the
[Configuration Guide](docs/guides/configuration.md) and
[Deployment Guide](docs/guides/deployment.md) for environment variables, Compose
profiles, and production topology.

### Option 3. Work on this repository with an AI agent

Start the agent with the repository contracts:

> Work on Wisdoverse Forge from repository truth. Read `AGENTS.md`, `SPEC.md`,
> `docs/README.md`, and `CONTRIBUTING.md` before editing. Keep backend changes in
> `rust/`, frontend changes in `src/app` and `shared/`, and deployment changes in
> `docker/` plus the relevant runbook.

## What It Provides

- Rust API and WebSocket gateway for work state, agent lifecycle, auth,
  telemetry, and the internal MCP bridge
- Rust orchestrator and Temporal workflow runtime
- Docker-backed container CLI sessions with sidecars and hooks
- Task, run, review, event, and evidence surfaces for governed execution
- Skills, plugins, prompts, credentials, and runtime configuration primitives
- PostgreSQL, Redis, NATS, MinIO, and Docker runtime integrations
- React/Vite/Three.js browser UI plus a Rust platform CLI

## Documentation

- [SPEC.md](SPEC.md) - language-agnostic service contract
- [docs/README.md](docs/README.md) - documentation map and truth hierarchy
- [Architecture Overview](docs/architecture/overview.md) - runtime topology and
  data flow
- [Getting Started](docs/guides/getting-started.md) - local setup path
- [Contributing](CONTRIBUTING.md) - workflow, validation, and PR expectations

## License

Wisdoverse Forge is licensed under the Wisdoverse Forge Business Source License
1.1 (`LicenseRef-Wisdoverse-Forge-BSL-1.1`). Each version changes to the Apache
License, Version 2.0 four years after that version is first made publicly
available by Wisdoverse. See [LICENSE](LICENSE) for the full terms.
