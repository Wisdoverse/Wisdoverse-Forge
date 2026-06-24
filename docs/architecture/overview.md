# Architecture Overview

This document describes the current default runtime path. Wisdoverse Forge runs a
Rust-first backend made of two primary services: `agentforge-server` for the
work, user, and agent control plane, and `agentforge-orchestrator` for task,
review, knowledge, and workflow orchestration. Live workflow execution and
worker startup are owned by the Rust orchestrator in the default path.

## System Context

```text
Browser (Vite in dev, frontend artifact service or static assets in prod)
      |
      | HTTP / WebSocket
      v
Rust API :4003 ----------------------------------------------+
  - auth, users, projects, admin, work state                  |
  - agent lifecycle, evidence, and realtime gateway           |
  - internal MCP bridge for runtime execution                 |
      |                                                       |
      | internal MCP                                          | NATS events / DB writes
      v                                                       |
Rust Orchestrator :4010 ----> Temporal :7233 / UI :8233       |
  - tasks, reviews, teams, metrics, workflows, knowledge      |
  - workflow worker bootstrap                                 |
                                                              v
PostgreSQL / Redis / NATS / Docker runtime / agent images
```

## Service Inventory

| Component               | Default Port                         | Responsibility                                                                            | Code                                                 |
| ----------------------- | ------------------------------------ | ----------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Frontend app            | `4002` in dev                        | Browser UI and local Vite loop; `prod` serves the built SPA through `agentforge-frontend` | `src/`                                               |
| Rust API                | `4003`                               | User-facing REST API, WebSocket gateway, work state, internal MCP bridge, runtime APIs    | `rust/bins/server`, `rust/crates/api`                |
| Rust orchestrator       | `4010`                               | Tasks, reviews, teams, metrics, knowledge, workflow CRUD and live runtime                 | `rust/bins/orchestrator`, `rust/crates/orchestrator` |
| Temporal                | `7233`, `8233`                       | Durable workflow engine and operator UI                                                   | Compose service                                      |
| Application PostgreSQL  | `5432`                               | Persistent state for the Rust API domain                                                  | `rust/crates/db`                                     |
| Orchestrator PostgreSQL | `5432` internal / external by config | Persistent state for orchestrator entities and workflow metadata                          | `rust/crates/orchestrator` migrations                |
| Redis                   | `6379`                               | Cache and coordination support where enabled                                              | `rust/crates/infra`                                  |
| NATS                    | `4222`, `8222`                       | Event transport between runtimes and the API/jobs layer                                   | `rust/crates/infra`, `rust/crates/jobs`              |
| Docker runtime          | n/a                                  | Isolated agent runtime execution and image-based tool routing                             | `rust/crates/platform`                               |
| Optional OpenSearch     | external                             | Knowledge search backend when enabled                                                     | Orchestrator config                                  |

## Core Flows

### 1. API and Realtime Flow

1. Browsers and automation clients call the Rust API on `:4003`.
2. The API validates auth, persists state to PostgreSQL, and uses Redis or NATS when those integrations are enabled.
3. Realtime clients connect to `/ws` on the Rust API for live event delivery.

### 2. Agent Execution Flow

1. The Rust API exposes an internal MCP endpoint at `/mcp` when `MCP_ENABLED=true`.
2. The MCP bridge provisions or resumes agent sessions in Docker using the configured workspace root and tool images.
3. Runtime events are published to NATS.
4. Rust jobs consume, persist, and rebroadcast those events to connected clients.

### 3. Workflow Execution Flow

1. Clients call the Rust orchestrator on `:4010` for `/api/v1/workflows/**`.
2. The orchestrator stores workflow definitions and node metadata in its own persistence layer.
3. When live runtime is enabled, the orchestrator starts a Temporal workflow and a local workflow worker.
4. Workflow activities call the Rust API internal MCP endpoint to create sessions, send prompts, and poll status.
5. The orchestrator updates workflow state and exposes run, status, cancel, signal, and history APIs.

## Deployment Topologies

| Topology                    | Description                                                                               | Typical Command Path                   |
| --------------------------- | ----------------------------------------------------------------------------------------- | -------------------------------------- |
| Local development           | Backend services in Docker Compose, frontend via Vite                                     | `make quickstart-local`, `npm run dev` |
| Self-contained production   | Rust services, frontend artifact service, internal PostgreSQL, Redis, Temporal, and Caddy | `make quickstart-selfhost-pull`        |
| External-service production | Rust services attached to externally managed databases and networks                       | `make prod-ext`                        |

The backend Compose stack does not provide the frontend dev server. In
development, the UI is a separate Vite process. In self-contained production,
Caddy serves browser routes through the `agentforge-frontend` artifact service;
external-service deployments may still publish `dist/` through their own web
tier.

## Repository Boundaries

| Path                                           | Role                                               | Status              |
| ---------------------------------------------- | -------------------------------------------------- | ------------------- |
| `rust/`                                        | Backend code                                       | Active              |
| `src/`, `shared/`                              | Frontend and shared contracts                      | Active              |
| `docker/`                                      | Compose files, Dockerfiles, deploy helpers         | Active              |
| `tests/unit`, `tests/integration`, `tests/e2e` | Validation path                                    | Active              |
| `hooks/`                                       | Event relay hook (container → sidecar via UDS)     | Active              |
| `rust/bins/buildx-plugin/`                     | Agent-container `docker buildx build` proxy helper | Active narrow scope |

The legacy TypeScript `server/` and `tests/legacy/` trees were removed after the Rust cutover (see git history for the full legacy state). The agent-container `docker buildx` proxy lives on the Rust path: `rust/bins/buildx-plugin/` builds the CLI plugin installed by the agent base image to route `docker buildx build` through the Docker proxy path.

Runtime ownership is described here and enforced by the Rust workspace, Docker
Compose files, and tests. The current proofed runtime boundary and command
evidence are tracked in [Runtime Validation](../runbooks/runtime-validation.md).
If you change runtime ownership, compose defaults, deployment topology, or a
README-visible capability boundary, update this document and the relevant
operational guides in the same PR.
