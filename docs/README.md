# Wisdoverse Forge Documentation

Rust-first platform docs for the Wisdoverse Forge governed AI workbench. Wisdoverse Forge
uses tasks, runs, evidence, context, skills, permissions, and runtime adapters
to make team AI work repeatable, reviewable, and portable across supported
runtimes.

The next product cycle validates recurring repository maintenance, starting
with dependency upgrades and failed-PR repair on one approved repository.
[Product Roadmap](../ROADMAP.md) sets priorities and stage gates;
[Product Validation](guides/product-validation.md) explains how a team measures
results. This focus does not expand the currently supported runtime contract.

If this directory disagrees with the code, treat the code as source of truth and
update the doc in the same PR.

## Truth Hierarchy

Use this order when documents disagree:

1. Source code, migrations, tests, Compose files, and Make targets.
2. Active docs under `architecture/`, `api/`, `guides/`, `runbooks/`, and `security/`.

Public docs should describe current contracts and reproducible operator
guidance. The roadmap describes intended work and evaluation targets, not
proof of available behavior. Historical plans, private review notes, pilot
records, and migration journals are not part of this documentation set.

Use **planned**, **implemented**, **runtime-verified**, and **adopted** separately.
Runtime evidence names the revision and environment; adoption requires measured
repeat use. A feature or a measurement hook alone does not establish either.

## Current Operating Contract

| Surface       | Current Contract                                                                                                          |
| ------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Backend       | Rust workspace under `rust/`; active API binary listens on `:4003`                                                        |
| Orchestration | Rust orchestrator listens on `:4010` and owns Temporal workflows                                                          |
| Browser app   | Vite/React app in `src/`; `prod` serves it via `agentforge-frontend`                                                      |
| Production    | `make prod-ext` is the external-service validation path; `make quickstart-selfhost-pull` is the self-contained Caddy path |
| Health probes | API liveness is `/health`; deep readiness is `/api/health`                                                                |
| Attachments   | Metadata in PostgreSQL; bytes in local object storage or RustFS/S3                                                        |

## Start Here

| Audience                     | Entry Points                                                                                                                                                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First local run / developers | [Getting Started](guides/getting-started.md), [Task Workflow Guide](guides/task-workflow.md), [Architecture Overview](architecture/overview.md), [Configuration](guides/configuration.md)                                          |
| Operators (deploy / run)     | [Deployment](guides/deployment.md), [RustFS migration](guides/rustfs-migration.md), [Runtime Validation](runbooks/runtime-validation.md), [Troubleshooting](guides/troubleshooting.md), [NATS Auth Runbook](runbooks/nats-auth.md) |
| Self-fix loop operators      | [Self-Fix Loop](guides/self-fix-loop.md), [Maintenance Delivery](guides/maintenance-delivery.md), [Self-Fix Security Model](security/self-fix-loop.md)                                                                                                                                     |
| CLI and local-agent users    | [CLI Platform Support](guides/cli-platform-support.md), [Host CLI Agent Enrollment](runbooks/host-cli-agent-enrollment.md), [Getting Started](guides/getting-started.md)                                                           |
| Product reviewers / pilot teams | [Product Roadmap](../ROADMAP.md), [Product Validation](guides/product-validation.md), [Product UX Direction](architecture/product-ux-direction.md), [Task Workflow Guide](guides/task-workflow.md) |
| Contributors                 | [Contributing](../CONTRIBUTING.md), [PR Status Summary](guides/pr-status-summary.md), [AGENTS.md](../AGENTS.md), [Architecture Overview](architecture/overview.md)                                                                 |
| API consumers                | [OpenAPI spec](api/openapi.yaml), [Turn API](api/turn-api.md)                                                                                                                                                                      |

## Documentation Map

### Active — source of truth

| Path                           | Purpose                                                            |
| ------------------------------ | ------------------------------------------------------------------ |
| [adr/](adr/)                   | Architecture Decision Records                                      |
| [architecture/](architecture/) | System design, runtime boundaries, data flow, product UX direction |
| [api/](api/)                   | OpenAPI specs (CI-enforced contract)                               |
| [guides/](guides/)             | Setup, configuration, deployment, troubleshooting                  |
| [runbooks/](runbooks/)         | Operational playbooks for failure modes and procedures             |
| [security/](security/)         | Security and dependency policy                                     |
| [versioning.md](versioning.md) | API versioning and release policy                                  |

Keep this tree small. Add a doc only when it is part of the public runtime,
operator, security, or API contract.

## Documentation Rules

- Active docs describe the running system. Rewrite rather than annotate when state drifts.
- Roadmap and product-direction sections label future work explicitly. Keep
  priorities and exit criteria in the roadmap, execution decisions in PRs/issues,
  delivered changes in release notes/changelog, and reproducible runtime proof
  in runbooks.
- Each capability claim links to implementation or runtime evidence as
  appropriate. Keep pilot measurements on team-owned systems and publish only
  consented, anonymized summaries or synthetic examples.
- Do not publish dated plans, private review notes, or migration journals in the public docs tree.
- Use reserved example domains and `dev@example.com` for public examples. Keep
  operator identities, private endpoints, and credentials out of docs and fixtures.
  Run `node scripts/check-secret-scan.mjs` before publishing; CI supplies the
  private hostname blocklist through the `INTERNAL_HOSTNAME_BLOCKLIST` repository
  secret. Without that secret, the hostname check has no configured targets.
- Every runtime, API, deployment, or workflow change updates the affected doc in the same PR.
- Keep retired implementation paths out of active docs unless the running code still exposes a compatibility boundary.
- Prefer relative links. Keep the first screen of each doc useful to its target audience.
- Write product and CLI docs for first-time users by default. Put expert
  internals after the basic path, and make every command copy-pasteable.

## Writing Standards

- Write public and repository documentation in English first. If another
  language is useful for a specific audience, keep it as a secondary note below
  the English source text.
- State scope, prerequisites, and validation steps explicitly.
- Avoid dated filenames unless the date is part of a public artifact identity.
- When behavior is environment-specific, state the exact profile, command, and port.

## Related Files

- [../README.md](../README.md) — repository entry point
- [../ROADMAP.md](../ROADMAP.md) — product direction, validation cycle and demand-triggered investment
- [../SPEC.md](../SPEC.md) — service contract for the Wisdoverse Forge runtime model
- [../CONTRIBUTING.md](../CONTRIBUTING.md) — engineering workflow
- [../CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) — community standards
- [../SECURITY.md](../SECURITY.md) — vulnerability disclosure policy
- [../docker/README.md](../docker/README.md) — Docker asset reference
