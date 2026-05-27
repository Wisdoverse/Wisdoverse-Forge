# Host CLI Enrollment Redesign — `agents.runtime_kind` as Single Source of Truth

**Status:** Proposed
**Date:** 2026-05-27
**Author:** Claude (pair with @schorsch888)
**Tracking branch:** preserves intent of `codex/local-agent-enrollment` (commit `c29c9ea`); supersedes its schema choice with reconciled naming and DDD/FSD compliance.

---

## 1. Motivation

The shipped Host CLI enrollment flow (PR #298) lets an operator's local CLI process join the platform via the sidecar, and PR #410 wires the `CreateAgentModal` to it. Two structural problems remain:

1. **Discriminator-by-prefix.** The application decides "is this a Host CLI agent?" by checking whether `agents.runtime_id` starts with the literal string `"host-"`. The runtime identifier doubles as the type tag. Refactoring the identifier format would silently break the discriminator.
2. **Container lifecycle does not reject Host CLI agents explicitly.** `AgentContainerLifecyclePolicy::ensure_container_backed` currently checks only that `cli_tool` is set, so a Host CLI agent passes the gate and fails later when `container_id IS NULL` — surfacing a misleading "stale container reference" error to the user. The same pattern exists across `start`, `stop`, and container control entry points.
3. **Frontend type is half-implemented.** `AgentInfo.runtimeKind: AgentRuntimeKind` is declared in `src/app/shared/model/agents.store.ts` and consulted by `isHostCliAgent`, but the backend never serializes that field. The store falls back to the prefix check.
4. **No defense-in-depth.** Nothing at the database layer prevents inconsistent states such as `runtime_kind = 'api'` with `cli_tool` set, or a Host CLI row whose `container_id` got populated by mistake.

The redesign promotes the runtime kind from an implicit string-prefix signal to an explicit first-class discriminator with consistent enforcement across database, repository, domain, application, and UI layers. The naming and structure follow FAANG/big-tech conventions for STI-style polymorphic aggregates, Domain-Driven Design aggregate boundaries, and the project's Feature-Sliced Design frontend rules.

## 2. Goals and Non-Goals

### Goals

- Make `runtime_kind` a single source of truth for "what kind of execution surface this agent has."
- Enforce coherent `(runtime_kind, cli_tool, container_id)` combinations at the database level via `CHECK` constraints.
- Replace string-prefix magic on `runtime_id` with a typed enum across backend and frontend.
- Reject container lifecycle operations on Host CLI and API agents with operator-facing error messages that explain the correct next action.
- Encode the Agent aggregate's creation invariants in typed factory methods (`AgentDraft::container`, `AgentDraft::host_cli`, `AgentDraft::api`), so a malformed row cannot be constructed in application code.
- Encode the "container-backed" invariant with a typestate wrapper (`ContainerAgent`), so Docker calls cannot compile against a non-container agent.
- Move the Agent domain types/specifications/API client to `src/app/entities/agent/` per the project's FSD rules.

### Non-Goals

- No change to the sidecar handshake protocol (NATS subjects, HMAC scheme, env-var contract).
- No change to the Container CLI agent runtime image or selection algorithm beyond what this redesign incidentally requires.
- No change to the audit trail format other than carrying `runtime_kind` in the event payload where already structured.
- No change to provider/model selection for API agents.
- No reshape of the orchestrator's `runtime_capabilities` table; this redesign concerns the `agents` aggregate, not the orchestration-side capability registry.

## 3. Glossary

These terms become the ubiquitous language across DB, code, docs, runbooks, and UI copy:

- **Container Runtime** (`runtime_kind = 'container'`): platform-spawned Docker container running an Agent Container CLI (`claude`, `codex`, `gemini`, `opencode`) and the sidecar.
- **Host CLI Runtime** (`runtime_kind = 'host_cli'`): operator-managed process on the operator's own machine, sidecar runs locally, joins the control plane via NATS using one-time credentials.
- **API Runtime** (`runtime_kind = 'api'`): provider-backed prompt agent (Anthropic / OpenAI / Google) with no shell, no container, no sidecar.

`docs/architecture/glossary.md` is updated to define these three terms and to point to this spec.

JSON-on-the-wire uses the kebab-case form: `"container"`, `"host-cli"`, `"api"`.
DB-on-disk uses the snake_case form: `'container'`, `'host_cli'`, `'api'`.

## 4. Architecture and Data Model

```
DB (truth)        agents.runtime_kind ∈ {container, host_cli, api}
                    + CHECK enforces (runtime_kind, cli_tool, container_id) coherence
                  ↓
Repository        AgentListItem { runtime_kind: AgentRuntimeKind, ... }
                  ↓
Domain (policy)   AgentRuntimeKind enum + AgentDraft factories + ContainerAgent newtype
                  ↓
Application       Services match AgentRuntimeKind on every lifecycle entry
                  ↓
API (serde)       runtimeKind: "container" | "host-cli" | "api"
                  ↓
Frontend          entities/agent owns the type and specifications
```

### 4.1 Core enum

Added to `rust/crates/core/src/agent_runtime.rs` alongside the existing `CliToolKind`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "kebab-case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum AgentRuntimeKind {
    Container,
    HostCli,
    Api,
}

impl AgentRuntimeKind {
    pub fn as_str(self) -> &'static str { /* snake_case literal for DB */ }
    pub fn parse_legacy(raw: &str) -> Result<Self, AgentRuntimeKindError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "container" => Ok(Self::Container),
            "host_cli" | "host-cli" | "cli" => Ok(Self::HostCli),
            "api" => Ok(Self::Api),
            other => Err(AgentRuntimeKindError::Unknown(other.to_string())),
        }
    }
}
```

`parse_legacy` accepts the historical `"cli"` literal used by the abandoned `c29c9ea` design so a deployment that ran an early variant of that migration can still be read.

### 4.2 Database schema

`rust/crates/db/migrations/062_agents_runtime_kind.sql`:

```sql
-- Track the execution surface for every managed agent.
--
-- 'container' is the platform-spawned Docker-backed Container CLI runtime.
-- 'host_cli'  is a user-managed CLI on the operator's own machine that
--             joins the control plane through the sidecar via NATS.
-- 'api'       is a provider-backed prompt agent with no shell runtime.

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS runtime_kind TEXT;

UPDATE agents SET runtime_kind = CASE
    WHEN cli_tool IS NULL                                       THEN 'api'
    WHEN runtime_id IS NOT NULL AND runtime_id LIKE 'host-%'    THEN 'host_cli'
    ELSE                                                              'container'
END
WHERE runtime_kind IS NULL;

ALTER TABLE agents
    ALTER COLUMN runtime_kind SET NOT NULL,
    ALTER COLUMN runtime_kind SET DEFAULT 'api';

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_check
    CHECK (runtime_kind IN ('container', 'host_cli', 'api')) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_invariants
    CHECK (
      (runtime_kind = 'container' AND cli_tool IS NOT NULL)
      OR
      (runtime_kind = 'host_cli'  AND cli_tool IS NOT NULL AND container_id IS NULL)
      OR
      (runtime_kind = 'api'       AND cli_tool IS NULL)
    ) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;

CREATE INDEX IF NOT EXISTS idx_agents_runtime_kind
    ON agents(runtime_kind);
```

The migration is idempotent (`IF NOT EXISTS`, `WHERE runtime_kind IS NULL`) so a partial application can be safely re-run, matching the project's existing idempotency convention.

The `NOT VALID` then `VALIDATE` pattern is used per CLAUDE.md's "tolerate existing production drift" rule: `NOT VALID` finishes immediately, then `VALIDATE` scans without blocking writes.

### 4.3 Aggregate boundary and DDD posture

The Agent is the aggregate root. `runtime_kind` is an attribute of Agent and constrained jointly with `cli_tool` and `container_id`. Host CLI enrollment is a factory operation on the Agent aggregate, not a separate aggregate, so the enrollment service performs a single atomic INSERT that writes runtime_kind, cli_tool, runtime_id, hmac_secret, nats_connect_password together — no follow-up UPDATE.

Three typed factories on the aggregate root replace the current plain `CreateAgentParams` struct:

```rust
// rust/crates/api/src/domain/agent.rs (new section)

pub struct AgentDraft { /* private fields */ }

impl AgentDraft {
    pub fn container(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
        system_prompt: Option<&str>,
    ) -> AppResult<Self> { /* validates and returns */ }

    pub fn host_cli(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        identity: HostCliIdentity, // bundles runtime_id, hmac_secret, nats_connect_password
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> { /* validates and returns */ }

    pub fn api(
        scope: &TenantScope,
        provider: &str,
        model: &str,
        name: Option<&str>,
        system_prompt: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> { /* validates and returns */ }
}
```

`AgentRepository::create(scope, draft: AgentDraft)` is the only insertion path. The repository extracts the validated fields from `AgentDraft` and writes one row. The previous `set_host_runtime` UPDATE pathway is removed; Host CLI rows are complete on INSERT.

A typestate wrapper guards Docker calls:

```rust
pub struct ContainerAgent(AgentListItem); // private constructor

#[derive(Debug)]
pub enum LifecycleRejection {
    HostCli,
    Api,
}

impl LifecycleRejection {
    pub fn into_app_error(self, action: &str) -> AppError {
        let message = match self {
            Self::HostCli => format!(
                "Host CLI agent: the platform does not manage the local container \
                 lifecycle. {} the sidecar on the operator machine using the \
                 enrollment script.",
                action
            ),
            Self::Api => format!(
                "API/provider agent has no container to {}.",
                action.to_lowercase()
            ),
        };
        AppError::from(ErrorKind::Validation(message))
    }
}

impl ContainerAgent {
    pub fn try_from_agent(agent: AgentListItem) -> Result<Self, LifecycleRejection> {
        match agent.runtime_kind {
            AgentRuntimeKind::Container => Ok(ContainerAgent(agent)),
            AgentRuntimeKind::HostCli  => Err(LifecycleRejection::HostCli),
            AgentRuntimeKind::Api      => Err(LifecycleRejection::Api),
        }
    }
    pub fn inner(&self) -> &AgentListItem { &self.0 }
}
```

`AgentContainerLifecycleService::{restart, start, stop}` take `&ContainerAgent` instead of `&AgentListItem`. A non-container agent cannot reach Docker by construction. The service layer converts `LifecycleRejection` to `AppError::Validation` via `into_app_error("Restart" | "Start" | "Stop")`, which the HTTP layer maps to 422 with the operator-facing message body. The exact final messages live in §7, generated from these templates.

## 5. Component Inventory

### 5.1 Rust workspace

**`rust/crates/core/`**

| File                         | Change                                                                                                                  |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `src/agent_runtime.rs` (new) | `AgentRuntimeKind` enum with `parse_legacy`, `as_str`, sqlx and serde derives. Submodule re-exported from `src/lib.rs`. |

**`rust/crates/db/`**

| File                                           | Change                                                                                    |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `migrations/062_agents_runtime_kind.sql` (new) | Adds column, backfills, sets NOT NULL, adds two CHECK constraints, adds index.            |
| `src/entities.rs`                              | Adds `runtime_kind: String` (sqlx leaves it as text; the API crate parses into the enum). |

**`rust/crates/api/`**

| File                                                                        | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/repositories/agent/mod.rs`                                             | `AgentListItem.runtime_kind: AgentRuntimeKind` (sqlx maps directly). SELECTs add `runtime_kind`. `create` accepts `AgentDraft`. `set_host_runtime` removed in favor of single-INSERT Host CLI creation. New `find_by_runtime_kind` helper.                                                                                                                                                                                                               |
| `src/domain/agent.rs`                                                       | New `AgentDraft` aggregate factory. `AgentContainerLifecyclePolicy::ensure_container_backed` replaced by `ContainerAgent::try_from_agent`. `HostAgentEnrollmentPolicy::runtime_id` stays but `set_host_runtime` removed. `runtime_id.starts_with("host-")` deleted from all application code.                                                                                                                                                            |
| `src/services/agent.rs`                                                     | `AgentService::create` accepts a typed intent (`CreateContainerIntent` / `CreateApiIntent`) and constructs the matching `AgentDraft`. Repository receives validated draft. Host CLI creation does **not** go through this method; it has its own service entry point at `HostAgentEnrollmentService::enroll` because enrollment needs the per-agent identity tuple (`runtime_id`, `hmac_secret`, `nats_connect_password`) and the env-script projection. |
| `src/services/agent_enrollment.rs`                                          | `HostAgentEnrollmentService::enroll` builds `HostCliIdentity` (uuid-derived runtime_id, freshly generated hmac/nats password), calls `AgentDraft::host_cli`, persists via repository, emits `HostAgentEnrollment` response.                                                                                                                                                                                                                              |
| `src/services/agent_container_lifecycle.rs`                                 | Entry points take `agent_id`, fetch `AgentListItem`, call `ContainerAgent::try_from_agent`. On `Err(HostCli)`/`Err(Api)`, return 422 with operator-facing message.                                                                                                                                                                                                                                                                                       |
| `src/services/agent_container_control.rs`                                   | Same pattern as lifecycle.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `src/routes/agents.rs`                                                      | `CreateAgentRequest` accepts optional `runtimeKind`; service layer derives if absent. Response includes `runtimeKind` via `AgentListItem` serialization.                                                                                                                                                                                                                                                                                                 |
| `src/services/admin.rs`, `src/repositories/admin.rs`, `src/domain/admin.rs` | Admin agent projection adds `runtime_kind` so the admin UI sees runtime split.                                                                                                                                                                                                                                                                                                                                                                           |

**`rust/crates/cli/`**

| File                             | Change                                                                                                                                                       |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/cmd/agents/enroll_local.rs` | Reads `runtimeKind` from response (already exposed), emits a `# runtime_kind: host_cli` header comment in the exported shell script for operator visibility. |

### 5.2 Shared TypeScript contracts

**`shared/types/agent.ts`**

- `export type AgentRuntimeKind = 'container' | 'host-cli' | 'api'`
- `interface AgentListItem.runtimeKind: AgentRuntimeKind` (required, not optional)
- `CreateAgentRequest.runtimeKind?: AgentRuntimeKind` (optional, server derives if absent)
- `HostAgentEnrollment.runtimeKind: 'host-cli'` (typed literal for clarity)

### 5.3 Frontend (FSD-compliant)

New entity layer:

```
src/app/entities/agent/
  model/
    types.ts            // AgentInfo, AgentRuntimeKind, AgentStatus, CliTool
    runtime-kind.ts     // isHostCliAgent / isContainerAgent / isApiAgent specifications
  api/
    AgentAPI.ts         // moved from src/app/shared/api/legacy/AgentAPI.ts
  index.ts              // public re-exports (barrel)
```

`isHostCliAgent` becomes a one-line specification:

```typescript
// src/app/entities/agent/model/runtime-kind.ts
import type { AgentInfo } from './types'

export function isHostCliAgent(agent: Pick<AgentInfo, 'runtimeKind'>): boolean {
  return agent.runtimeKind === 'host-cli'
}

export function isContainerAgent(agent: Pick<AgentInfo, 'runtimeKind'>): boolean {
  return agent.runtimeKind === 'container'
}

export function isApiAgent(agent: Pick<AgentInfo, 'runtimeKind'>): boolean {
  return agent.runtimeKind === 'api'
}
```

The `runtimeId.startsWith('host-')` fallback is removed.

Files updated to import from `@app/entities/agent` instead of `@app/shared/model/agents.store`:

- `src/app/features/agents/AgentConfigTab.tsx`
- `src/app/features/agents/AgentControlPanel.tsx`
- `src/app/features/agents/AgentListView.tsx`
- `src/app/features/agents/AgentCard.tsx`
- `src/app/widgets/agent-detail/AgentDetailView.tsx`
- `src/app/pages/getting-started/ui/GettingStartedView.tsx`
- `src/app/features/agents/CreateAgentModal.tsx`
- any test file that imports `AgentInfo` / `isHostCliAgent`

`src/app/shared/model/agents.store.ts` stays in place per CLAUDE.md's "cross-slice stores under shared" guidance, but it imports types and specifications from `@app/entities/agent`. The legacy `src/app/shared/api/legacy/AgentAPI.ts` is removed once all imports point at the entity barrel.

`npm run fsd:check` must remain green after the move. The lint step in CI gates the FSD boundary; the rewrite must not push any new violation.

## 6. Data Flow

### 6.1 Create a Container Runtime agent

```
Frontend:    POST /api/v1/agents { cliTool: "codex", workspaceId }
Routes:      routes/agents.rs::create_agent → AgentService::create
Service:     CreateContainerIntent { cli_tool: CliToolKind::Codex, ... }
             → AgentDraft::container(...)
Repository:  INSERT INTO agents (..., runtime_kind, cli_tool, ...)
             VALUES (..., 'container', 'codex', ...)
DB CHECK:    agents_runtime_kind_invariants passes (container + cli_tool NOT NULL)
Response:    { ok: true, agent: { ..., runtimeKind: "container", cliTool: "codex" } }
```

### 6.2 Create an API Runtime agent

```
Frontend:    POST /api/v1/agents { provider, model, systemPrompt }
Service:     CreateApiIntent { provider, model, system_prompt }
             → AgentDraft::api(...)
Repository:  INSERT runtime_kind='api', cli_tool=NULL, container_id=NULL
DB CHECK:    passes (api + cli_tool NULL)
```

### 6.3 Enroll a Host CLI Runtime agent

```
Frontend:    POST /api/v1/agents/local-enroll { cliTool: "codex", workspaceId, ... }
Routes:      routes/agents.rs::enroll_local_agent
Service:     HostAgentEnrollmentService::enroll
             - validate cli_tool, name, nats_base_url
             - resolve workspace mount scope
             - identity = HostCliIdentity::generate(agent_id_seed)
             - AgentDraft::host_cli(scope, cli_tool, identity, ...)
Repository:  AgentRepository::create(scope, draft)
             - single INSERT writes runtime_kind='host_cli', cli_tool=codex,
               runtime_id='host-abc12345', hmac_secret=..., nats_connect_password=...
DB CHECK:    passes (host_cli + cli_tool NOT NULL + container_id NULL)
Service:     AgentContainerEnvPolicy::build(...) + AGENTFORGE_RUNTIME_KIND=host_cli
Response:    { ok, agent: { ..., runtimeKind: "host-cli", runtimeId: "host-abc12345" },
               enrollment: { env, shellExports, sidecarCommand, serverUrl } }
```

### 6.4 Container lifecycle: restart with Host CLI rejection

```
Frontend:    POST /api/v1/agents/:id/restart
Service:     AgentContainerLifecycleService::restart
             - agent = repo.get(scope, id)
             - container = ContainerAgent::try_from_agent(agent)?
                 // returns Err(LifecycleRejection::HostCli) for host_cli
                 // returns Err(LifecycleRejection::Api) for api
             - on Err: map to 422 with operator-facing message (see §7)
             - on Ok(container): docker.inspect → docker.stop → docker.start
```

The same pattern applies to `start`, `stop`, `clear_container`, and any other Docker-backed lifecycle method. `ContainerAgent` is the only type accepted by these methods.

### 6.5 List or read an agent

```
GET /api/v1/agents → SELECT a.*, ..., a.runtime_kind FROM agents a JOIN ...
sqlx::query_as<AgentListItem> — runtime_kind parsed directly into AgentRuntimeKind
serde rename_all = kebab-case → JSON emits "container" | "host-cli" | "api"
Frontend store reads agent.runtimeKind, isHostCliAgent returns a single enum match
```

`events` and WebSocket broadcasts continue to use existing payload schemas; the runtime kind ships in the agent object embedded in those payloads.

## 7. Error Handling

All operator-facing rejections are `ErrorKind::Validation` mapped to HTTP 422 with structured `{ ok: false, error: <message> }` bodies. Messages follow CLAUDE.md's "operators as first-time users" rule: each one names the runtime kind, says what failed, and points at the correct next action.

| Scenario                                                             | Message                                                                                                                                                  |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| restart on host_cli                                                  | `"Host CLI agent: the platform does not manage the local container lifecycle. Restart the sidecar on the operator machine using the enrollment script."` |
| restart on api                                                       | `"API/provider agent has no container to restart. Send a new prompt to invoke the model again."`                                                         |
| start on host_cli                                                    | `"Host CLI agent: re-run the enrollment shell script on the operator machine to start the sidecar."`                                                     |
| start on api                                                         | `"API/provider agent has no container to start."`                                                                                                        |
| stop on host_cli                                                     | `"Host CLI agent: stop the sidecar process on the operator machine. The platform cannot stop it remotely."`                                              |
| stop on api                                                          | `"API/provider agent has no container to stop."`                                                                                                         |
| create with runtime_kind=container but cli_tool missing              | `"cli_tool is required for container-backed agent (one of: claude, codex, gemini, opencode)."`                                                           |
| create with runtime_kind=api but cli_tool set                        | `"api/provider agent cannot have cli_tool. Remove cli_tool or change runtimeKind."`                                                                      |
| create with runtime_kind=host_cli but cli_tool missing               | `"cli_tool is required for Host CLI enrollment (one of: claude, codex, gemini, opencode)."`                                                              |
| DB CHECK violation (defense-in-depth, should never reach this point) | Maps to `AppError::Internal`, emits `tracing::error` with agent_id, runtime_kind, cli_tool, container_id.                                                |

Sensitive material (hmac_secret, nats_connect_password) is never logged or echoed in error bodies.

## 8. Migration Safety

| Step                                                                   | Lock                                                             | Notes                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ADD COLUMN IF NOT EXISTS runtime_kind TEXT` (no default)              | AccessExclusive (metadata only)                                  | Safe on PG 11+, no table rewrite.                                                                                                                                                                                                                                                                                                                                                                                            |
| `UPDATE agents SET runtime_kind = CASE ... WHERE runtime_kind IS NULL` | RowExclusive                                                     | Idempotent via `WHERE runtime_kind IS NULL`. A single statement is acceptable at the current production row count; if a future deployment shows the table has grown into the millions, this statement should be split into batches before deploying the migration. Before promoting the migration to production, the on-call operator should `SELECT COUNT(*) FROM agents;` and either confirm the size or split the update. |
| `ALTER COLUMN runtime_kind SET NOT NULL`                               | AccessExclusive (full validation)                                | Post-backfill there are zero NULLs; validation is fast at current scale.                                                                                                                                                                                                                                                                                                                                                     |
| `ADD CONSTRAINT ... CHECK ... NOT VALID` then `VALIDATE`               | NOT VALID: brief AccessExclusive. VALIDATE: ShareUpdateExclusive | Adding `NOT VALID` is instant. `VALIDATE` does not block writes; it blocks other DDL.                                                                                                                                                                                                                                                                                                                                        |
| `CREATE INDEX IF NOT EXISTS idx_agents_runtime_kind`                   | ShareUpdateExclusive (no CONCURRENTLY)                           | Project's migration runner does not support CONCURRENTLY. Plain `CREATE INDEX` blocks writes until completion; instantaneous at current size.                                                                                                                                                                                                                                                                                |

The migration is forward-only per CLAUDE.md "do not edit migrations that have run." A future scope reduction would land as `063_relax_agents_runtime_kind.sql` rather than an edit to 062.

A schema-contract test (`rust/crates/api/tests/agents_runtime_kind_constraint.rs`) asserts both that the migration produces the expected post-state on a fresh database and that the backfill maps a representative set of legacy rows correctly. This matches the existing schema-contract testing pattern referenced in CLAUDE.md.

Zero-downtime rollout sequence:

1. Deploy migration 062 with current application code. The new column exists but is unused; old code keeps writing rows whose `runtime_kind` is filled by the migration default.
2. Deploy the new application code, which writes and reads `runtime_kind` end-to-end.
3. (Optional) After confidence, remove any legacy compatibility branches that were left for safety during step 1–2.

## 9. Defense-in-Depth Layers

- **Layer 1 — DB CHECK constraints.** Impossible `(runtime_kind, cli_tool, container_id)` combinations cannot be persisted.
- **Layer 2 — sqlx enum decoding.** A row with a value outside the enum (corrupt data or out-of-band write) fails to decode and surfaces as an internal error with full context.
- **Layer 3 — Domain factories + typestate.** `AgentDraft` validates intent at construction; `ContainerAgent::try_from_agent` ensures non-container agents cannot reach Docker.
- **Layer 4 — Frontend types.** `AgentRuntimeKind` is required, not optional. TS exhaustive matching surfaces dead cases at compile time.

## 10. Observability

- `tracing` spans on lifecycle/enrollment add a structured `agent.runtime_kind` field on the relevant scopes (`agents.restart`, `agents.start`, `agents.stop`, `agents.local-enroll`).
- `events` table payloads carrying agent objects pick up `runtime_kind` for free via the new sqlx serialization; downstream consumers (frontend WebSocket reducers) gain visibility without protocol additions.
- Migration emits one INFO log on completion per the existing migration runner pattern.

## 11. Testing Strategy

### 11.1 Database schema-contract tests

`rust/crates/api/tests/agents_runtime_kind_constraint.rs` (new):

- Boot SQLx test pool, run migrations including 062.
- Assert all 9 combinations of `(runtime_kind, cli_tool, container_id)`:
  - `('container', NOT NULL, *)` insert OK.
  - `('container', NULL, *)` rejected.
  - `('host_cli', NOT NULL, NULL)` insert OK.
  - `('host_cli', NOT NULL, NOT NULL)` rejected.
  - `('host_cli', NULL, *)` rejected.
  - `('api', NULL, NULL)` insert OK.
  - `('api', NOT NULL, *)` rejected.
  - `('api', NULL, NOT NULL)` rejected.
  - `('bogus', *, *)` rejected by enum CHECK.
- Backfill test: seed pre-migration rows that represent legacy container / `host-` prefix / api shapes, run the migration on a fresh database, assert each row gains the correct `runtime_kind`.

### 11.2 Core enum tests

In `rust/crates/core/src/agent_runtime.rs` `#[cfg(test)]` module:

- `parse_legacy`: case-insensitivity, whitespace trimming, accept of `"cli"` legacy literal, rejection of unknown values.
- `as_str` round-trip with `parse_legacy`.
- serde JSON round-trip emits `"host-cli"` form.
- sqlx encode/decode round-trip.

### 11.3 Domain policy and factory tests

In `rust/crates/api/src/domain/agent.rs` `#[cfg(test)]`:

- `AgentDraft::container` validates cli_tool and rejects empty name beyond 255 chars.
- `AgentDraft::host_cli` requires non-empty `HostCliIdentity` fields and rejects empty cli_tool.
- `AgentDraft::api` rejects empty model.
- `ContainerAgent::try_from_agent`: 3 kinds × expected variant (Ok / Err::HostCli / Err::Api). Error variants carry the operator-facing message text.

### 11.4 Repository tests

In `rust/crates/api/src/repositories/agent/` (extend existing `tests.rs`):

- `create(draft: AgentDraft)` writes correct `runtime_kind` for each draft kind.
- `list_with_owner` and `find_with_owner_by_id` return `AgentListItem.runtime_kind` parsed as enum.
- `find_by_runtime_kind` (new method) returns only matching rows, scoped by tenant.

### 11.5 Service / route integration tests

In `rust/crates/api/tests/agent_lifecycle_routes.rs` (extend):

- `POST /api/v1/agents` with body `{ cliTool: "codex" }` → response `runtimeKind = "container"`.
- `POST /api/v1/agents` with body `{ provider: "anthropic", model: "claude-opus-4-7" }` → response `runtimeKind = "api"`.
- `POST /api/v1/agents/local-enroll` with body `{ cliTool: "codex", ... }` → response `runtimeKind = "host-cli"`, `runtimeId` starts with `"host-"`, body contains `shellExports`.
- `POST /api/v1/agents/:id/restart` on host_cli agent → 422 with exact rejection message from §7.
- `POST /api/v1/agents/:id/restart` on api agent → 422 with exact rejection message from §7.
- `POST /api/v1/agents/:id/restart` on container agent without `container_id` → existing stale-container behavior, unchanged.
- Tenant scope isolation: cross-org access to host_cli agents still 404.

### 11.6 Frontend tests (Vitest)

- `tests/unit/app/entities/agent/runtime-kind.test.ts` (new): `isHostCliAgent` / `isContainerAgent` / `isApiAgent` exhaustive table; verify the prefix fallback is removed (a `runtimeKind: 'container'` agent whose `runtimeId` accidentally starts with `host-` returns `false`).
- `tests/unit/app/AgentControlPanel.test.tsx`, `AgentListView.test.tsx`, `AgentCard.test.tsx`: fixtures replace `runtimeId: 'host-...'` with `runtimeKind: 'host-cli'`; add at least one `runtimeKind: 'api'` case verifying that no restart button or terminal tab renders.

### 11.7 End-to-end tests (Playwright)

- `tests/e2e/specs/host-cli-enrollment.spec.ts` (extend or add): creates a Host CLI agent via the modal, asserts response payload, navigates to the agent list and confirms the "Host CLI" badge, attempts to use Container CLI restart UI on the new agent and confirms the operator-facing 422 message renders.

### 11.8 FSD boundary verification

`npm run fsd:check` must stay green. The lint step in CI gates this; CI configuration is unchanged.

### 11.9 Manual validation checklist (post-deploy on staging)

- `make prod-ext` brings up the stack with migration 062 applied.
- `psql` query: `SELECT runtime_kind, COUNT(*) FROM agents GROUP BY 1;` returns no NULLs and the expected distribution.
- Web UI: create one Container CLI agent, one Host CLI agent (via modal), one Provider agent. Each shows the correct runtime label.
- Web UI: restart attempts on Host CLI and Provider agents render the new operator-facing error messages.

## 12. Open Questions

1. Should `HostCliIdentity` live in `agentforge-core` (cross-crate value object) or stay private to `rust/crates/api`? Initial recommendation: stay in `rust/crates/api` until a second consumer (e.g., the CLI crate) requires it.
2. Should the eventual `RuntimeKind` admin filter ship as part of this work, or be tracked as a follow-up? Initial recommendation: ship the field in the admin projection; UI filter follow-up.

## 13. Acceptance Criteria

1. `agents.runtime_kind` is a NOT NULL column with both CHECK constraints in production, validated.
2. All three runtime kinds are creatable via the API and visible in the UI.
3. Container lifecycle operations on Host CLI and Provider agents return 422 with the operator-facing messages in §7. No 5xx, no "stale container reference" misleading errors.
4. Frontend code never calls `runtimeId.startsWith('host-')`. The agent domain types and specifications live under `src/app/entities/agent/`.
5. `npm run fsd:check`, `npm run lint`, `npm run typecheck`, `cd rust && make ci` all pass on the integration branch.
6. The schema-contract test in §11.1 passes against both a fresh DB and a backfilled-from-legacy DB.
7. The runbook at `docs/runbooks/host-cli-agent-enrollment.md` and the glossary at `docs/architecture/glossary.md` are updated to use the new ubiquitous-language terms.

## 14. Out of Scope (future work)

- Admin UI filter on `runtime_kind`.
- Telemetry dashboards split by runtime kind.
- Re-organizing the orchestrator's `runtime_capabilities` table to align with the agent runtime kind vocabulary.
- Migrating `src/app/shared/model/agents.store.ts` itself into the entity layer; the project chose to keep cross-slice stores in `shared/` per CLAUDE.md, and this redesign respects that.
