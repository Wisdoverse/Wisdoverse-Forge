# Glossary

Canonical terminology used in UI labels, API docs, runbooks, and metrics. Keep this file as the single source of truth — any copy that reaches a user or operator must match a term here.

## Core concepts

| Term                      | Meaning                                                                                                                                                                                                                                                           |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Agent**                 | A managed AI work actor that may be container-backed or provider-backed. The DB table is `agents` (was `sessions` before migration 058).                                                                                                                          |
| **Container CLI**         | The task-execution CLI running **inside** an agent's container (`claude` / `codex` / `gemini` / `opencode`). Persisted as `agents.cli_tool`. Alternative phrasings: _in-container CLI_, _agent CLI runtime_. **Never** just "CLI" on its own — that is ambiguous. |
| **Platform CLI**          | The `agentforge` binary that operators use to manage the platform itself (list agents, start/stop containers, query jobs, run migrations). Built from the `rust/crates/cli` crate. Alternative phrasing: _agentforge CLI_. Not to be confused with Container CLI. |
| **Provider+Prompt agent** | An agent with `cli_tool = NULL` — no container, no terminal. Calls the LLM provider directly via the LLM gateway. The kind selector in `CreateAgentModal` calls this **Provider + Prompt**.                                                                       |
| **Participant**           | An agent registered for A2A orchestration. DB table `participants` (was `agents` pre-rename, hence the participant/agent distinction).                                                                                                                            |
| **Sidecar**               | Per-container NATS bridge that forwards hook events from Container CLIs to the backend. Lives in `rust/bins/sidecar`.                                                                                                                                             |
| **Worker bridge**         | The sidecar's inner event loop that subscribes to NATS assignments and forwards them to the Container CLI.                                                                                                                                                        |

## Runtime modes (Settings page)

| Label                        | Meaning                                                                                                                |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **Host CLI (local process)** | `runtime = cli` — the Container CLI is spawned as a local OS process on the host, not in a container. Legacy dev mode. |
| **API (direct LLM calls)**   | `runtime = api` — used by provider+prompt agents; no CLI involved at all.                                              |
| **Container (Docker)**       | `runtime = container` — default. Container CLI runs inside a Docker container managed by the platform.                 |

## UI copy rules

- **Always qualify "CLI"** in UI strings. Prefer "Container CLI" or "Platform CLI" (or a more specific runtime label) over bare "CLI".
- **Field names stay unchanged**: `cli_tool`, `cliTool`, `CLI_TOOLS` etc. are schema/code identifiers, not display strings. Do not rename them purely for UI clarity — it churns the DB + every client without buying anything.
- **Vendor product names stay as vendors use them**: "Claude Code", "Gemini CLI" — don't prefix with "Container CLI:" in product selectors; the grouping surface (the dropdown's own label) already establishes context.

## When to add a term

Before introducing a new term in UI / docs / metrics: check this file. If the concept exists, use the term from here. If it doesn't, add it here **before** shipping the first caller so we don't create a second synonym.
