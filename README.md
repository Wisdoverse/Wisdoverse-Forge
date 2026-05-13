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
>
> The proofed runtime boundary is documented in
> [Runtime Validation](docs/runbooks/runtime-validation.md). README-visible
> capabilities outside that boundary remain preview work until the runbook lists
> a validation path for them.

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

For a clean local trial, the first commands are:

```bash
npm install
make quickstart-local
npm run dev
```

Then open `http://localhost:4002`, register the first account, and use the
in-app Start page to add a provider and create an agent.

For a single-host self-contained deployment, use the one-command path:

```bash
make quickstart-selfhost DOMAIN=forge.example.com
```

Use the default `DOMAIN=localhost` for a private trial. Public domains use
automatic HTTPS through Caddy when DNS points at the host and ports `80`/`443`
are reachable.

If `80` or `443` is already occupied on the host, pass alternate public ports:

```bash
make quickstart-selfhost DOMAIN=localhost HTTP_PORT=18080 HTTPS_PORT=18443
```

Then open `https://localhost:18443`.

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

## Current Preview Boundaries

The validated `prod-ext` contract covers the Rust API, WebSocket/NATS realtime
fanout, Rust orchestrator, Temporal workflow execution, PostgreSQL/Redis/NATS
health, and a browser-to-sidecar orchestration task path with task evidence.

The following surfaces are still preview placeholders and should not be
represented as complete product capabilities:

- Per-agent git status from `GET /api/v1/agents/:id/git`
- Voice transcription through `POST /api/v1/voice/transcribe`

## Documentation

- [SPEC.md](SPEC.md) - language-agnostic service contract
- [docs/README.md](docs/README.md) - documentation map and truth hierarchy
- [Architecture Overview](docs/architecture/overview.md) - runtime topology and
  data flow
- [Runtime Validation](docs/runbooks/runtime-validation.md) - current proofed
  runtime boundary and commands
- [Getting Started](docs/guides/getting-started.md) - local setup path
- [Contributing](CONTRIBUTING.md) - workflow, validation, and PR expectations

## License

Wisdoverse Forge is licensed under the Wisdoverse Forge Business Source License
1.1 (`LicenseRef-Wisdoverse-Forge-BSL-1.1`). Each version changes to the Apache
License, Version 2.0 four years after that version is first made publicly
available by Wisdoverse. See [LICENSE](LICENSE) for the full terms.
