# Wisdoverse Forge

Wisdoverse Forge turns team work into auditable tasks, runs, and evidence — operated by AI agents inside isolated container, host-CLI, or provider runtimes under one governed control plane.

> [!WARNING]
> Wisdoverse Forge is a low-key engineering preview for trusted self-hosted environments. The active backend is Rust-owned; legacy TypeScript server paths are not part of the current runtime. Capabilities outside the [Runtime Validation](docs/runbooks/runtime-validation.md) boundary remain preview work.

## Running Wisdoverse Forge

### Requirements

- Docker and Docker Compose v2
- Node.js 24+
- Make, Git
- Resources for PostgreSQL, Redis, NATS, Temporal, and Docker-backed agent containers

### Option 1. Implement your own service

Point your coding agent at the spec:

> Implement a Wisdoverse Forge-compatible service according to `SPEC.md` (and the protocol contracts in `shared/types/`). Match the runtime boundary documented in `docs/runbooks/runtime-validation.md`.

### Option 2. Run this reference implementation

Tell your coding agent to set up this repository against your machine:

> Set up Wisdoverse Forge from `docs/guides/getting-started.md`. Run `npm install`, then `make quickstart-local`, then `npm run dev`. Open `http://localhost:4002`, register the first account, add a provider in Settings, and create an agent from the Start page. If a single-host VPS deployment is needed, follow the prebuilt-image path `make quickstart-selfhost-pull DOMAIN=<domain>` from the same guide.

For Host CLI enrollment (operator-managed CLI joins the platform), follow `docs/runbooks/host-cli-agent-enrollment.md`. For the migration 062-065 `runtime_kind` discriminator deployment, follow `docs/runbooks/migration-062-runtime-kind.md`.

### Option 3. Work on this repository

Brief the agent with the repository contracts before editing:

> Work on Wisdoverse Forge from repository truth. Read `AGENTS.md`, `SPEC.md`, `CLAUDE.md`, `docs/README.md`, `docs/architecture/ddd-contract.md`, and `CONTRIBUTING.md` before changing code. Keep backend changes in `rust/`, frontend changes in `src/app/` (Feature-Sliced Design layers) and `shared/`, and deployment changes in `docker/` plus the matching runbook. Run `npm run fsd:check`, `npm run lint`, `npm run typecheck`, and `cd rust && make ci` against any change. Use `gh` for GitHub PRs and `glab` for GitLab.

---

## What It Provides

- **Rust API + WebSocket gateway** for work state, agent lifecycle, auth, telemetry, internal MCP bridge
- **Rust orchestrator + Temporal workflow runtime**
- **Three first-class agent runtimes** governed by `agents.runtime_kind` (`container | cli | api`) with DB CHECK invariants on `(runtime_kind, cli_tool, container_id)`:
  - **Container Runtime** — platform-spawned Docker container running a Container CLI (`claude` / `codex` / `gemini` / `opencode`) plus sidecar
  - **Host CLI Runtime** — operator-managed CLI on the operator's machine that enrolls via sidecar and NATS, idempotent (`Idempotency-Key`), atomic `agent.enrolled` audit event
  - **API Runtime** — provider-backed prompt agent (Anthropic / OpenAI / Google) with no shell, no container
- **Task, run, review, event, evidence surfaces** for governed execution
- **Skills, plugins, prompts, credentials, runtime configuration primitives**
- **Per-agent NATS auth via callout** with HMAC-signed result envelopes, per-agent scoped pub/sub permissions, zero shared agent credentials
- **PostgreSQL + Redis + NATS + MinIO + Docker** integrations with online-safe migration patterns
- **React/Vite/Three.js browser UI** under strict Feature-Sliced Design boundaries (`app → pages → widgets → features → entities → shared`), gated by `npm run fsd:check`
- **Rust Platform CLI (`agentforge`)** with `migrate doctor` pre-flight, `agents enroll-local` host-CLI enrollment, ops subcommands
- **Multi-locale operator UI** (English + Chinese) with i18n error codes on every user-facing rejection

## Repository Map (for agents)

```
rust/                  Rust workspace (active backend)
  crates/core/         Shared domain types, errors, RuntimeKind, CliToolKind
  crates/db/           SQLx pool + migrations
  crates/auth/         JWT + Argon2 + auth middleware
  crates/infra/        Redis + NATS clients
  crates/api/          Axum routes / services / repositories / WS gateway / MCP bridge
                       (route → service → domain → repository layering enforced
                        by tests/route_ddd_boundary_test.rs)
  crates/platform/     Docker, security policy, warm pool
  crates/jobs/         PostgreSQL task queue
  crates/llm/          Multi-provider LLM gateway
  crates/orchestrator/ Temporal workflow logic
  crates/cli/          Platform CLI library
  bins/server/         Main API binary
  bins/orchestrator/   Orchestrator service binary
  bins/sidecar/        Agent container sidecar
  bins/cli/            `agentforge` operator CLI
src/                   React/Vite/Three.js frontend
  app/entities/        Domain types + specifications + stores (FSD entity layer)
  app/features/        User workflows (FSD feature layer)
  app/widgets/         Composed view surfaces (FSD widget layer)
  app/pages/           Route-level surfaces (FSD page layer)
  app/shared/          Cross-slice utilities, i18n, generated clients
shared/                Cross-stack TypeScript contracts + generated proto output
hooks/                 Agent container hook relay
docker/                Dockerfiles + Compose files (dev / prod / external profiles)
tests/                 Vitest + Playwright suites
docs/                  Architecture, runbooks, guides, specs
```

## Documentation

- [SPEC.md](SPEC.md) — language-agnostic service contract
- [AGENTS.md](AGENTS.md) — symlink to `CLAUDE.md`, the agent entrypoint
- [docs/README.md](docs/README.md) — documentation map and truth hierarchy
- [Architecture Overview](docs/architecture/overview.md) — runtime topology and data flow
- [DDD Layer Contract](docs/architecture/ddd-contract.md) — route / service / domain / repository rules
- [Aggregate Catalog](docs/architecture/aggregate-catalog.md) — DDD aggregates and modules
- [Threat Model](docs/security/threat-model.md) — STRIDE per trust boundary
- [Observability and SLOs](docs/runbooks/observability-slo.md) — SLIs, SLOs, alerts
- [Host CLI Enrollment](docs/runbooks/host-cli-agent-enrollment.md) — operator guide for local CLI joins
- [Migration 062 Runbook](docs/runbooks/migration-062-runtime-kind.md) — `runtime_kind` migration sequence (062/063/064/065)
- [Runtime Validation](docs/runbooks/runtime-validation.md) — current proofed runtime boundary
- [CLI Platform Support](docs/guides/cli-platform-support.md) — Platform CLI + sidecar multi-platform expectations
- [Versioning Policy](docs/versioning.md) — API versioning and release policy
- [Contributing](CONTRIBUTING.md) — workflow, validation, and PR expectations
- [Code of Conduct](CODE_OF_CONDUCT.md) — community standards
- [Security Policy](SECURITY.md) — vulnerability disclosure

## License

Wisdoverse Forge is licensed under the Wisdoverse Forge Business Source License 1.1 (`LicenseRef-Wisdoverse-Forge-BSL-1.1`). Each version changes to the Apache License, Version 2.0 four years after that version is first made publicly available by Wisdoverse. See [LICENSE](LICENSE) for the full terms.
