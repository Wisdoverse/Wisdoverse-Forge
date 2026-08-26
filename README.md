# Wisdoverse Forge

[![CI](https://github.com/Wisdoverse/Wisdoverse-Forge/actions/workflows/ci.yml/badge.svg)](https://github.com/Wisdoverse/Wisdoverse-Forge/actions/workflows/ci.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/Wisdoverse/Wisdoverse-Forge/badge)](https://securityscorecards.dev/viewer/?uri=github.com/Wisdoverse/Wisdoverse-Forge)
[![License: BSL 1.1](https://img.shields.io/badge/license-BSL%201.1-blue.svg)](LICENSE)

Wisdoverse Forge is a self-hosted, governed AI workbench for a team. It turns
requests into managed agent work: create tasks, assign agents, watch progress,
review results, and keep evidence and reusable learnings in one workspace.

**Product status.** Early access for self-hosted teams. The one-command local
path below is the supported first-run experience — from a clean checkout to one
reviewed task in your first session. Before a deployment carries a real team,
run [Runtime Validation](docs/runbooks/runtime-validation.md) and follow the
[Long-Term Product Roadmap](ROADMAP.md) quality gates. Forge
never phones home: your data and provider keys stay on your machines, and
provider keys are encrypted with an operator-supplied key.

## Running Wisdoverse Forge

### What you need first

- Docker and Docker Compose v2
- Node.js 24+
- Make, Git
- Enough local resources to run the browser app, backend services, and agent
  work area together

### Option 1. One-command start

This is the path a real user takes: clone, run one command, follow the setup
checklist in the browser.

1. Install the prerequisites (Docker + Docker Compose v2, Node.js 24+, Make,
   Git).
2. Clone the repository, then run:

   ```bash
   git clone https://github.com/Wisdoverse/Wisdoverse-Forge.git wisdoverse-forge
   cd wisdoverse-forge
   make product
   ```

`make product` installs app dependencies if missing, prepares
`docker/.env`, starts and health-checks the backend services, starts the
browser app, and opens it at `http://localhost:4002`. Press **Ctrl+C** in that
terminal to stop the browser app and the services the command started;
`make product-down` stops the stack later. The full first-run guide is
[Getting Started](docs/guides/getting-started.md).

Success looks like this:

- The browser opens on the app; you can register the first account.
- The **Start** checklist guides you through a workspace, an AI service, an
  agent, and one small task.
- Settings lets you add an AI service and choose **Check connection**.

Developers who want terminals separated still have two commands:
`make quickstart-local` for the backend stack, then `npm run dev` for the
browser app.

### Option 2. Ask an agent to set it up

Tell your coding agent to set up this repository against your machine:

> Set up Wisdoverse Forge from `docs/guides/getting-started.md`. Run `make product` (or, for separate terminals: `npm install`, `make quickstart-local`, `npm run dev`). Open `http://localhost:4002`, register the first account, follow the Start checklist, add an AI service in Settings, and create an agent. If a single-host VPS deployment is needed, follow the prebuilt-image path `make quickstart-selfhost-pull DOMAIN=<domain>` from the same guide.

To connect this computer as a managed agent, follow
[Host CLI Agent Enrollment](docs/runbooks/host-cli-agent-enrollment.md). For
multi-platform CLI expectations, see
[CLI Platform Support](docs/guides/cli-platform-support.md).

### Option 3. Work on this repository

Brief the agent with the repository contracts before editing:

> Work on Wisdoverse Forge from repository truth. Read `AGENTS.md`, `SPEC.md`, `CLAUDE.md`, `docs/README.md`, `docs/architecture/ddd-contract.md`, and `CONTRIBUTING.md` before changing code. Keep backend changes in `rust/`, frontend changes in `src/app/` (Feature-Sliced Design layers) and `shared/`, and deployment changes in `docker/` plus the matching runbook. Run `npm run fsd:check`, `npm run lint`, `npm run typecheck`, and `cd rust && make ci` against any change. Use `gh` for GitHub PRs and `glab` for GitLab.

### Option 4. Implement a compatible service

Point your coding agent at the service contract:

> Implement a Wisdoverse Forge-compatible service according to `SPEC.md` and
> the protocol contracts in `shared/types/`. Match the proven boundary in
> `docs/runbooks/runtime-validation.md`.

---

## What It Provides

- **Tasks and runs** so teams can see what agents are doing and what happened.
- **Agents that fit different work styles**:
  - **Project files** for agents that edit shared project files.
  - **This computer** for agents that work from a local machine.
  - **Simple chat agents** for planning, writing, and review without file access.
- **Review and evidence views** so people can check important work before using
  it.
- **Skills, plugins, prompts, and saved access** so repeat work is easier to set
  up.
- **Admin health and update pages** with plain-language next steps for common
  setup problems.
- **Platform CLI (`agentforge`)** for operators who want to run migrations,
  check setup, or connect a local machine as a managed agent.
- **English and Chinese UI copy** with user-safe error messages.

For technical readers, the current implementation uses a Rust API and
WebSocket gateway, a Rust orchestrator, PostgreSQL, Redis, NATS, MinIO, Docker,
Temporal, and a React/Vite/Three.js browser app. The frontend follows strict
Feature-Sliced Design boundaries (`app -> pages -> widgets -> features ->
entities -> shared`) checked by `npm run fsd:check`.

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

- [ROADMAP.md](ROADMAP.md) — long-term product roadmap: vision, phases, quality bar
- [Product UX Direction](docs/architecture/product-ux-direction.md) — product contract and acceptance checklist
- [SPEC.md](SPEC.md) — language-agnostic service contract
- [AGENTS.md](AGENTS.md) — symlink to `CLAUDE.md`, the agent entrypoint
- [docs/README.md](docs/README.md) — documentation map and truth hierarchy
- [Architecture Overview](docs/architecture/overview.md) — runtime topology and data flow
- [DDD Layer Contract](docs/architecture/ddd-contract.md) — route / service / domain / repository rules
- [Aggregate Catalog](docs/architecture/aggregate-catalog.md) — DDD aggregates and modules
- [Threat Model](docs/security/threat-model.md) — STRIDE per trust boundary
- [Observability and SLOs](docs/runbooks/observability-slo.md) — SLIs, SLOs, alerts
- [Self-host Operator Runbook](docs/runbooks/self-host-ops.md) — config knobs, weekly checklist, incident pointers, rotation
- [Host CLI Enrollment](docs/runbooks/host-cli-agent-enrollment.md) — operator guide for local CLI joins
- [Migration 062 Runbook](docs/runbooks/migration-062-runtime-kind.md) — `runtime_kind` migration sequence (062/063/064/065)
- [Runtime Validation](docs/runbooks/runtime-validation.md) — current proofed runtime boundary
- [Offline Install](docs/guides/offline-install.md) — air-gapped bundles, checksums, Ed25519 signing
- [CLI Platform Support](docs/guides/cli-platform-support.md) — Platform CLI + sidecar multi-platform expectations
- [CLI Agent Image Auto-Update](docs/guides/cli-image-auto-update.md) — keep agent images current, prune superseded overlays, operator-initiated roll
- [Project Git Clone](docs/guides/project-git-clone.md) — create a project from a git repository; clone status, retry, and the layered SSRF/credential defense
- [Clone Egress Firewall](docs/runbooks/clone-egress-firewall.md) — required deploy-layer egress policy for project git clone
- [Versioning Policy](docs/versioning.md) — API versioning and release policy
- [Contributing](CONTRIBUTING.md) — workflow, validation, and PR expectations
- [Support](SUPPORT.md) — where to get help and how to ask
- [Code of Conduct](CODE_OF_CONDUCT.md) — community standards
- [Security Policy](SECURITY.md) — vulnerability disclosure

## License

Wisdoverse Forge is licensed under the Wisdoverse Forge Business Source License 1.1 (`LicenseRef-Wisdoverse-Forge-BSL-1.1`). Each version changes to the Apache License, Version 2.0 four years after that version is first made publicly available by Wisdoverse. See [LICENSE](LICENSE) for the full terms.
