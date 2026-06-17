# Contributing to Wisdoverse Forge

Wisdoverse Forge is Rust-first. New product and runtime work belongs in `rust/`, `src/`, `shared/`, `docker/`, and the active test suites. The agent-container buildx proxy and the entire runtime path are Rust-owned.

## Prerequisites

- Node.js 24+
- Docker and Docker Compose v2
- Make
- Git
- A local environment that can run PostgreSQL-, Redis-, NATS-, and Temporal-backed containers

## Local Setup

```bash
git clone https://github.com/Wisdoverse/Wisdoverse-Forge.git wisdoverse-forge
cd wisdoverse-forge
npm install

cp docker/.env.example docker/.env
# Set POSTGRES_PASSWORD, REDIS_PASSWORD, JWT_SECRET, and NATS callout values.

make setup
make dev
npm run dev
```

`make dev` brings up the backend stack. `npm run dev` starts the Vite frontend separately.

## Supported Development Loops

| Loop                 | Commands                                             | Use When                                                                         |
| -------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------- |
| Full platform        | `make setup`, `make dev`, `npm run dev`              | Daily development, orchestration work, workflow runtime, end-to-end verification |
| API-only             | `npm run server`, `npm run dev`                      | Frontend or API work that does not require orchestrator or Temporal              |
| Orchestrator-focused | `cd rust && cargo run --bin agentforge-orchestrator` | Focused orchestrator development against existing infrastructure                 |
| Rust workspace only  | `cd rust && cargo test --workspace`                  | Pure backend iteration and contract work                                         |

For workflow runtime work, use the full platform loop unless you have already provisioned a compatible Temporal and orchestrator database locally.

### Fast inner loop (sub-minute backend reload)

`make dev` rebuilds the backend Docker image on each change. For tight backend
iteration, run only the infrastructure in Docker and run the Rust server locally
so a change recompiles in seconds instead of rebuilding an image:

```bash
cargo install cargo-watch   # one-time
make dev-infra              # PostgreSQL (5432), Redis (6379), NATS (4222) only
make backend-watch          # cargo watch -x 'run --bin agentforge-server'
npm run dev                 # frontend, separate terminal
```

`make backend-watch` recompiles and reruns `agentforge-server` on every save.
Point the server's `DATABASE_URL` / `REDIS_URL` / `NATS_URL` at the
`dev-infra` services on `localhost` (the ports above).

What is already hot — no rebuild needed:

- **Frontend:** `npm run dev` (Vite) is live-reload and proxies `/api` + `/ws`
  to the local server; frontend changes never rebuild the backend.
- **Configuration and feature flags:** the server is fully env-driven, so
  flipping a flag (for example `PRESENCE_REDIS_ENABLED`) or rotating config
  needs only a server restart — never a rebuild.

For an already-deployed external-profile stack, redeploy a single backend
service with `make deploy-server` / `make deploy-orchestrator` instead of
rebuilding the whole stack (see `docs/guides/deployment.md`).

## Repository Boundaries

| Path                                           | Status | Notes                                                                      |
| ---------------------------------------------- | ------ | -------------------------------------------------------------------------- |
| `rust/`                                        | Active | Backend workspace                                                          |
| `src/` and `shared/`                           | Active | Frontend and shared contracts                                              |
| `docker/`                                      | Active | Compose, Dockerfiles, deployment helpers                                   |
| `tests/unit`, `tests/integration`, `tests/e2e` | Active | Validation suites                                                          |
| `hooks/`                                       | Active | Event relay hook (container → sidecar via UDS)                             |
| `rust/bins/buildx-plugin/`                     | Active | Rust helper for intercepting `docker buildx build` inside agent containers |

## Validation Expectations

Run the union of the checks for every area you touched.

| Change Scope                          | Minimum Validation                                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Documentation only                    | `git diff --check`                                                                                                              |
| Frontend / shared / scripts           | `npm run lint`, `npm run format:check`, `npm run typecheck`, `npm run test:unit`                                                |
| Rust code                             | `cd rust && cargo fmt --all --check`, `cd rust && cargo clippy --workspace -- -D warnings`, `cd rust && cargo test --workspace` |
| Proto or generated platform contracts | `npm run proto:gen`, `npm run proto:check`                                                                                      |

If a change spans multiple areas, run the combined matrix before pushing.

## Branches and Commits

- Use descriptive branches such as `feat/rust-workflow-runtime`, `fix/orchestrator-health`, or `docs/runtime-guides`.
- Follow Conventional Commits, for example `feat(orchestrator): start temporal worker in live mode`.
- Keep commits reviewable. Separate functional changes, refactors, and documentation sweeps when possible.

## Pull Request Standard

Every PR should include:

- a short summary of the user or operator impact,
- the concrete validation commands that were run,
- screenshots or API examples when UI or contract behavior changes,
- migration, environment, or rollout notes when runtime behavior changes,
- documentation updates for any changed API, runtime, deployment path, or contributor workflow.

### Low-token PR status checks

Do not repeatedly refresh PR, MR, or CI status inside an agent chat. Use one
compact snapshot first.

For GitHub pull requests:

```bash
npm run pr:summary
```

For GitLab merge requests or pipelines, use one `glab` snapshot with the fields
needed for `ACTION`, `WAIT`, or `DONE`; do not use watch mode from chat.

Read the buckets this way:

- `ACTION`: fix the listed PRs or checks now.
- `WAIT`: stop checking in chat; review, CI, or the merge queue is still
  working. Reuse `npm run pr:summary:local` if you only need to show the last
  saved snapshot.
- `DONE`: no action is needed for that PR or MR.

For an external monitor, schedule the low-noise command instead of asking an
agent to watch a loop:

```bash
npm run pr:summary:monitor
```

That command reuses the local snapshot for 1 hour when it runs too soon and
exits with an alert only when a PR needs action. See
[docs/guides/pr-status-summary.md](docs/guides/pr-status-summary.md) for the
refresh rules and emergency one-time override.
Do not lower the repeat-read guard below 60 seconds or put the emergency
override in scripts, aliases, scheduled jobs, or agent instructions. Do not
lower the monitor cache below 1 hour in scheduled jobs.

### Merge requirements

`main` is protected by a GitHub ruleset: all 15 status checks must be green, one
approving review is required, and force-push / branch deletion / non-linear
history are blocked. Merges go through `gh pr merge --squash --delete-branch`
once those conditions are met.

The Repository Admin role holds a `pull_request`-scoped bypass actor for
**break-glass merges only** — an admin may waive the one-approval requirement
(never the status checks, never the PR itself) when a second reviewer is
genuinely unavailable. This is governed by
[docs/runbooks/break-glass-merge.md](docs/runbooks/break-glass-merge.md); every
break-glass merge must leave the documented audit comment. Direct pushes to
`main` are not permitted for anyone, including admins.

## User Experience Standard

Build and document features for first-time operators, not only professional
engineers.

- The first path in UI and docs should be the shortest safe path.
- Prerequisites must appear before commands or configuration fields.
- Each workflow should say what success looks like and what to do next.
- Advanced internals belong in a later section, runbook, or validation block.
- Error messages should explain the failed action, why it matters, and the next
  action the user can take.
- Avoid exposing implementation-only terms when a product term exists in the
  glossary.

For UI changes, PRs should include a screenshot or a concise description of the
operator path that was tested. For CLI changes, include the command used, the
target platform, and the expected success output.

## CLI Platform Standard

The Platform CLI and local sidecar are supported product surfaces. Changes to
`rust/bins/cli`, `rust/crates/cli`, `rust/bins/sidecar`, release packaging, or
Host CLI enrollment must preserve the multi-platform policy in
[`docs/guides/cli-platform-support.md`](docs/guides/cli-platform-support.md).

CLI-related PRs should state support for:

- Linux x86_64 and ARM64,
- macOS Apple Silicon and Intel,
- Windows x86_64,
- Windows ARM64 when the release pipeline has a validated runner and signer.

If a platform cannot be validated in the PR, document it as unverified rather
than implying support. Public release artifacts should include checksums,
signature or provenance instructions, and a smoke test for `agentforge --help`
and `agentforge-sidecar --help`.

## Documentation Policy

Documentation is English-first. Public docs, repository guides, examples, and
operator-facing runbooks should use English as the source text. Add translated
notes only as secondary material when they are necessary for a specific
audience.

Update docs in the same PR when you change:

- default runtime ownership,
- API or workflow contracts,
- Compose profiles or deployment topology,
- contributor setup or validation commands,
- environment variables or operational runbooks.

At minimum, runtime and deployment changes usually require updates to `README.md`, `docs/architecture/overview.md`, and the relevant guide under `docs/guides/`.
CLI and local-agent changes require updates to
`docs/guides/cli-platform-support.md` or
`docs/runbooks/host-cli-agent-enrollment.md` when user-facing behavior changes.

## Security and Secrets

- Never commit live credentials, tokens, or production hostnames.
- Use placeholder values in examples.
- Treat internal tokens such as `MCP_TOKEN` and `ORCHESTRATOR_INTERNAL_TOKEN` as secrets.
- If a change affects auth, workspace isolation, or container boundaries, call that out explicitly in the PR.
