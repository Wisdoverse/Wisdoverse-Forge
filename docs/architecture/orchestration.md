# Orchestration Architecture

Wisdoverse Forge orchestration is owned by the Rust orchestrator in the default runtime path. It is responsible for participant provisioning, tasks, reviews, teams, metrics, knowledge, and workflow execution backed by Temporal when enabled.

## Service Boundary

| Service           | Port            | Responsibility                                                  |
| ----------------- | --------------- | --------------------------------------------------------------- |
| Rust API          | `4003`          | Agent execution, internal MCP bridge, user-facing control plane |
| Rust orchestrator | `4010`          | Orchestration APIs, workflow runtime, orchestration realtime    |
| Temporal          | `7233` / `8233` | Durable workflow execution and operator UI                      |

## Core Domains

| Domain       | Description                                                                 |
| ------------ | --------------------------------------------------------------------------- |
| Participants | Internal participants linked to users or agents for orchestration scenarios |
| Tasks        | Work items, assignment state, transitions, and execution metadata           |
| Reviews      | Review rounds, comments, approvals, and rejections                          |
| Teams        | Team definitions and memberships                                            |
| Metrics      | Aggregated operational metrics over orchestration state                     |
| Knowledge    | Search and indexing integrations for orchestration context                  |
| Workflows    | DAG definitions, node state, runtime history, and live execution            |

## Workflow Runtime

In the current default path:

1. Workflow CRUD is served by the Rust orchestrator.
2. When `ORCHESTRATOR_TEMPORAL_ENABLED=true`, the orchestrator connects to Temporal.
3. The orchestrator starts its local workflow worker during live startup.
4. Workflow activities call the Rust API internal MCP endpoint to create sessions, prompt agents, and poll results.
5. Workflow run, status, cancel, signal, and history endpoints are fully owned by the Rust orchestrator.

## Internal Structure

The orchestrator is organized around repository, service, transport, and runtime layers inside `rust/crates/orchestrator/`.

Key modules include:

- `auth/` for participant provisioning and API auth modes
- `task/`, `review/`, `team/`, `metrics/`, `knowledge/`, `workflow/` for domain logic
- `realtime.rs` for orchestration-specific WebSocket fan-out
- `state.rs` for live dependency construction and service wiring

## Realtime Model

The orchestrator exposes `/ws/events` for orchestration updates. This is separate from the Rust API browser-facing `/ws` gateway.

## Dependencies

| Dependency                      | Required                                    | Purpose                                           |
| ------------------------------- | ------------------------------------------- | ------------------------------------------------- |
| Orchestrator PostgreSQL         | Yes in live mode                            | Persistent orchestration state                    |
| Temporal                        | Required for live workflow runtime          | Durable workflow engine                           |
| Rust API MCP endpoint           | Required for live workflow agent activities | Session creation, prompt dispatch, status polling |
| OpenSearch / embedding provider | Optional                                    | Knowledge search and embedding workflows          |

## Historical Note

The former Go orchestrator and helper runtime paths are not part of the default stack. When older design docs mention BullMQ, Go services, or placeholder workflow execution, read them as migration history rather than current architecture.
