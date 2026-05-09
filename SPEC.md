# Wisdoverse Forge Service Specification

Status: Draft v1 for the current Rust-first runtime.

Purpose: define the service contract for a self-hosted governed AI workbench
that operates team work through explicit tasks, isolated runtime sessions,
workflow orchestration, reusable context, skills, and observable evidence.

## Normative Language

The key words `MUST`, `MUST NOT`, `REQUIRED`, `SHOULD`, `SHOULD NOT`,
`RECOMMENDED`, `MAY`, and `OPTIONAL` are to be interpreted as described in RFC 2119.

Implementation-defined means the behavior is part of a concrete deployment or
adapter contract, but this specification does not prescribe one universal
policy. Implementations MUST document the selected behavior.

## 1. Problem Statement

Wisdoverse Forge is a long-running self-hosted governed AI workbench for team-scale
work. It turns human work requests into explicit task, run, and evidence state;
creates or resumes isolated agent runtimes; routes prompts, reusable context,
and runtime events; and exposes enough status to review, debug, and operate
multiple concurrent agent runs.

The service solves five operational problems:

- It moves agent work out of one-off terminal supervision and into auditable
  task, workflow, and session state.
- It keeps Task, Run, and Evidence as the canonical facts for judging and
  reusing completed work.
- It isolates container CLI execution in managed Docker-backed sessions.
- It keeps realtime progress visible through persisted events and WebSocket
  updates.
- It gives context and skills a governed platform boundary with provenance,
  scope, and revocation instead of leaving them trapped in vendor-specific
  assistant history.
- It separates platform operation from container CLI behavior, so `claude`,
  `codex`, `gemini`, `opencode`, and future tools can run behind the same
  service boundary.
- It provides a Rust-owned API and orchestration boundary that can be validated,
  deployed, and operated by self-hosted teams.

Important boundary:

- Wisdoverse Forge is a governed work control plane, runtime manager, and
  orchestration surface.
- Container CLIs perform runtime-specific work inside managed sessions.
- External system updates, ticket updates, pull requests, CI, and repository
  writes are performed through the tools available to the user, agent, or
  workflow environment.
- A successful run can end at a workflow-defined handoff state such as review,
  validation, or deployment readiness; it does not always mean the work should
  be landed automatically.

## 2. Goals And Non-Goals

### 2.1 Goals

- Provide authenticated, tenant-scoped APIs for users, projects, agents, tasks,
  reviews, workflows, and runtime status.
- Preserve task, run, and evidence state as inspectable records for governance,
  review, and reuse.
- Launch and manage isolated agent sessions through Docker-backed runtimes.
- Support multiple container CLIs behind a stable platform model.
- Provide primitives for reusable context, skills, prompts, credentials, and
  runtime-aware execution policies.
- Persist durable domain state in PostgreSQL.
- Use Redis, NATS, MinIO or local object storage, Docker, and Temporal where the
  selected deployment profile enables them.
- Broadcast realtime runtime and orchestration state to browser clients.
- Run live workflows through the Rust orchestrator and Temporal when enabled.
- Document deployment topology, configuration, and validation paths in-repo.

### 2.2 Non-Goals

- Hosted SaaS operation by default.
- A replacement for every external issue tracker, code host, or CI provider.
- A universal sandbox for arbitrary untrusted code beyond the configured
  container, host, and policy boundaries.
- A single mandated container CLI. Wisdoverse Forge is a platform boundary; individual
  CLI tools remain adapter-specific.
- New backend behavior in removed legacy TypeScript server paths.

## 3. System Overview

### 3.1 Main Components

1. `Browser App`
   - Presents agent, task, workflow, review, event, and runtime surfaces.
   - Talks to the Rust API over HTTP and WebSocket.
2. `Rust API`
   - Owns auth, tenant scope, user/admin APIs, agent lifecycle, realtime gateway,
     jobs integration, and the internal MCP bridge.
3. `Rust Orchestrator`
   - Owns tasks, reviews, teams, metrics, knowledge, workflow CRUD, and live
     workflow execution.
   - Starts the local workflow worker when Temporal runtime is enabled.
4. `Workflow Engine`
   - Temporal provides durable workflow execution and operator visibility.
5. `Persistence And Infrastructure`
   - PostgreSQL stores durable domain state.
   - Redis and NATS provide cache, coordination, event transport, and wake-up
     behavior where enabled.
   - MinIO or local object storage stores uploaded file bytes.
6. `Agent Runtime`
   - Docker creates isolated sessions with configured workspace roots and
     tool-specific images.
   - Sidecars and hooks relay runtime events to the platform.
7. `Platform CLI`
   - The `agentforge` operator binary manages platform tasks such as migrations,
     agent operations, and diagnostics.

### 3.2 Runtime Layers

Wisdoverse Forge is easiest to reason about in these layers:

1. `Policy Layer`
   - Auth, tenant scope, container policy, CORS, WebSocket origin checks, and
     environment-specific deployment settings.
2. `Control Plane Layer`
   - Rust API routes, services, repositories, jobs, and realtime gateway.
3. `Orchestration Layer`
   - Rust orchestrator task, review, workflow, knowledge, metrics, and Temporal
     runtime.
4. `Execution Layer`
   - Docker runtime, sidecar, container CLI processes, workspaces, hooks, and
     internal MCP bridge.
5. `Evidence Layer`
   - Persisted events, workflow history, logs, review state, task results, and
     validation outputs that operators can inspect before accepting work.
6. `Operator Surface Layer`
   - Browser UI, REST APIs, WebSocket streams, OpenAPI specs, runbooks, and the
     platform CLI.

## 4. Core Domain Model

### 4.1 Organization And Tenant Scope

Every tenant-scoped operation MUST derive organization scope from authenticated
middleware. Repository methods that read or mutate tenant data MUST constrain the
query by organization.

### 4.2 User

A user is an authenticated actor in an organization. Users can own projects,
create work, configure credentials, and review agent output according to
authorization policy.

### 4.3 Agent

An agent is a managed AI work actor. It MAY be backed by a container CLI
session or by provider-backed execution, depending on runtime configuration,
capability requirements, and product surface.

### 4.4 Container CLI

A container CLI is the coding CLI running inside an agent container, such as
`claude`, `codex`, `gemini`, or `opencode`. Platform documentation MUST use
`Container CLI` when referring to that in-container tool and `Platform CLI` when
referring to the `agentforge` operator binary.

### 4.5 Session

A session represents a concrete runtime interaction with an agent. Container
sessions are created or resumed through the Rust API internal MCP bridge and are
bound to Docker runtime state, workspace configuration, and event flow.

### 4.6 Task

A task is a work item tracked by the orchestration APIs. Tasks have tenant
scope, assignment state, priority, progress, result or error state, and
dispatch metadata. Dispatchable task state MUST be handled by repository and
service methods that preserve tenant boundaries.

### 4.7 Review

A review records code-review or work-review state linked to a task or session.
Review comments, assignees, and state transitions are part of the orchestration
domain and MUST remain tenant-scoped.

### 4.8 Workflow

A workflow is an orchestrator-owned definition and runtime. When live runtime is
enabled, workflow execution starts a Temporal workflow and uses activities that
call the Rust API internal MCP endpoint to create sessions, send prompts, and
poll status.

### 4.9 Event

Events describe runtime, hook, sidecar, job, and orchestration activity. Events
MUST be safe to persist, broadcast, and inspect. Sensitive payloads MUST be
redacted or excluded before serialization.

### 4.10 Evidence

Evidence is the operator-visible record used to judge whether work is complete:
events, workflow history, task state, review comments, logs, CI status, and
explicit validation output. Evidence MAY be materialized differently by feature
area, but it SHOULD remain traceable to the task, session, workflow, or run that
produced it.

### 4.11 Context Asset

A context asset is a reusable memory, source snippet, decision, preference, or
domain fact that can be attached to a user, team, project, task, agent, or run.
Context assets MUST have provenance, ownership, scope, and revocation metadata
before they are reused across runs.

### 4.12 Skill Package

A skill package is a versioned reusable workflow asset. It SHOULD define when to
use it, when not to use it, required inputs, steps, tools or runtime
requirements, examples, owner, success evidence, and rollback behavior.

## 5. Runtime Contracts

### 5.1 API Contract

- Public infrastructure endpoints such as `/health` MAY be unauthenticated.
- New HTTP, WebSocket, and MCP routes SHOULD be behind auth middleware unless
  they are intentionally public infrastructure.
- API responses SHOULD keep the existing `{ ok: true/false, ...data }` style
  where that surface already uses it.
- Internal errors MUST NOT leak implementation details to clients.

### 5.2 WebSocket Contract

- Browser-facing realtime delivery is served by the Rust API WebSocket gateway.
- WebSocket auth MUST validate JWT tokens and configured origins.
- Orchestration-specific realtime delivery is served by the Rust orchestrator
  where the orchestration surface requires it.

### 5.3 Agent Runtime Contract

- Agent containers MUST be created through the configured platform runtime.
- Container security validation MUST continue to block privileged mode, host
  PID, Docker socket mounts, and missing resource limits unless an explicit
  policy changes that behavior.
- Container CLI behavior MUST flow through sidecar, hooks, internal MCP, NATS,
  HTTP APIs, and persisted events rather than bypassing the platform path.

### 5.4 Orchestration Contract

- The Rust orchestrator owns workflow CRUD and live workflow execution.
- Temporal is REQUIRED for live workflow runtime in the default production
  contract.
- Workflow activities that need agents SHOULD call the Rust API internal MCP
  endpoint rather than creating an independent agent runtime path.

### 5.5 Persistence Contract

- PostgreSQL is REQUIRED for durable platform state.
- Existing production migrations MUST NOT be edited after deployment. Add a new
  corrective migration instead.
- Queue and wake-up code that uses notification channels MUST retain a polling
  fallback.

## 6. Deployment Profiles

Wisdoverse Forge supports these deployment profiles:

- `dev`: backend services in Docker Compose, browser app through Vite.
- `prod`: self-contained production profile with Rust services and bundled
  infrastructure services.
- `external`: production profile attached to externally managed services and
  networks.

Production contract work SHOULD be validated with `make prod-ext` when it
affects external-service topology.

## 7. Observability And Operations

Implementations SHOULD expose:

- Liveness and readiness checks for API, orchestrator, and core dependencies.
- Structured logs for API, orchestrator, jobs, sidecar, and runtime failures.
- Realtime event streams for browser and operator surfaces.
- Runbooks for common failure modes such as NATS auth, deployment, and runtime
  troubleshooting.
- Validation evidence in pull requests and release gates.

## 8. Security Requirements

- Secrets MUST NOT be logged.
- Sensitive response fields MUST use serialization controls such as
  `skip_serializing`.
- LLM provider credentials MUST be encrypted where the configured runtime
  requires it.
- Per-agent NATS credentials and permissions MUST preserve the agent isolation
  model.
- Test and manual login flows SHOULD use documented development accounts rather
  than ad hoc debug users.

## 9. Conformance Checklist

A Wisdoverse Forge-compatible change SHOULD answer:

- Which runtime layer does this change touch?
- Which tenant-scoped repository queries changed?
- Which API, WebSocket, MCP, or workflow contract changed?
- Which event or evidence surfaces prove the behavior?
- Which docs and runbooks were updated?
- Which validation commands ran?

## 10. Related Documents

- `README.md` for the public project overview and quick start.
- `docs/architecture/overview.md` for the current runtime topology.
- `docs/architecture/orchestration.md` for the Rust orchestrator contract.
- `docs/architecture/overview.md` for event and data flow.
- `docs/guides/configuration.md` for runtime configuration.
- `docs/guides/deployment.md` for deployment profiles.
- `CONTRIBUTING.md` for engineering workflow and validation expectations.
