# Wisdoverse Forge Documentation

Rust-first platform docs for the Wisdoverse Forge governed AI workbench. Wisdoverse Forge
uses tasks, runs, evidence, context, skills, permissions, and runtime adapters
to make team AI work repeatable, reviewable, and portable across supported
runtimes.

If this directory disagrees with the code, treat the code as source of truth and
update the doc in the same PR.

## Truth Hierarchy

Use this order when documents disagree:

1. Source code, migrations, tests, Compose files, and Make targets.
2. Active docs under `architecture/`, `api/`, `guides/`, `runbooks/`, and `security/`.

Public docs should describe current contracts and reproducible operator
guidance. Historical plans, private review notes, and migration journals are not
part of this documentation set.

## Current Operating Contract

| Surface       | Current Contract                                                                                                     |
| ------------- | -------------------------------------------------------------------------------------------------------------------- |
| Backend       | Rust workspace under `rust/`; active API binary listens on `:4003`                                                   |
| Orchestration | Rust orchestrator listens on `:4010` and owns Temporal workflows                                                     |
| Browser app   | Vite/React app in `src/`; `prod` serves it via `agentforge-frontend`                                                 |
| Production    | `make prod-ext` is the external-service validation path; `make quickstart-selfhost` is the self-contained Caddy path |
| Health probes | API liveness is `/health`; deep readiness is `/api/health`                                                           |
| Attachments   | Metadata in PostgreSQL; bytes in local object storage or MinIO                                                       |

## Start Here

| Audience                     | Entry Points                                                                                                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First local run / developers | [Getting Started](guides/getting-started.md), [Architecture Overview](architecture/overview.md), [Configuration](guides/configuration.md)                                          |
| Operators (deploy / run)     | [Deployment](guides/deployment.md), [Runtime Validation](runbooks/runtime-validation.md), [Troubleshooting](guides/troubleshooting.md), [NATS Auth Runbook](runbooks/nats-auth.md) |
| Contributors                 | [Contributing](../CONTRIBUTING.md), [AGENTS.md](../AGENTS.md), [Architecture Overview](architecture/overview.md)                                                                   |
| API consumers                | [OpenAPI spec](api/openapi.yaml), [Turn API](api/turn-api.md)                                                                                                                      |

## Documentation Map

### Active — source of truth

| Path                           | Purpose                                                |
| ------------------------------ | ------------------------------------------------------ |
| [architecture/](architecture/) | System design, runtime boundaries, data flow           |
| [api/](api/)                   | OpenAPI specs (CI-enforced contract)                   |
| [guides/](guides/)             | Setup, configuration, deployment, troubleshooting      |
| [runbooks/](runbooks/)         | Operational playbooks for failure modes and procedures |
| [security/](security/)         | Security and dependency policy                         |

Keep this tree small. Add a doc only when it is part of the public runtime,
operator, security, or API contract.

## Documentation Rules

- Active docs describe the running system. Rewrite rather than annotate when state drifts.
- Do not publish dated plans, private review notes, or migration journals in the public docs tree.
- Every runtime, API, deployment, or workflow change updates the affected doc in the same PR.
- Keep retired implementation paths out of active docs unless the running code still exposes a compatibility boundary.
- Prefer relative links. Keep the first screen of each doc useful to its target audience.

## Writing Standards

- Write public and repository documentation in English first. If another
  language is useful for a specific audience, keep it as a secondary note below
  the English source text.
- State scope, prerequisites, and validation steps explicitly.
- Avoid dated filenames unless the date is part of a public artifact identity.
- When behavior is environment-specific, state the exact profile, command, and port.

## Related Files

- [../README.md](../README.md) — repository entry point
- [../SPEC.md](../SPEC.md) — service contract for the Wisdoverse Forge runtime model
- [../CONTRIBUTING.md](../CONTRIBUTING.md) — engineering workflow
- [../docker/README.md](../docker/README.md) — Docker asset reference
