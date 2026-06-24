# Host CLI Enrollment Redesign — `agents.runtime_kind` as Single Source of Truth

**Status:** Proposed (revision 2 — incorporates findings from 10-round architectural review + 2-team security review + PM gate)
**Date:** 2026-05-27 (rev 2 amendments same day)
**Author:** Claude (pair with @schorsch888)
**Reviewers:** Distinguished Architect, Staff Rust+PG, Staff Frontend+FSD, Distinguished Engineer (DDD), Staff SRE, Principal PM, Principal AppSec, Principal Platform Security
**Tracking branch:** preserves intent of `codex/local-agent-enrollment` (commit `c29c9ea`); supersedes its schema choice with reconciled naming, ubiquitous-language alignment, and DDD/FSD compliance.

> **Revision 2 changes:** see §15 "Review Decisions" for the full audit trail. Highlights: reuse of existing `agentforge_core::RuntimeKind` instead of duplicate enum; canonical value renamed `host_cli` → `cli` to match `docs/architecture/glossary.md` and the orchestrator's existing literals; rolling-deploy CHECK violation closed by splitting the migration into 062 (schema+backfill) and 063 (CHECK after new app deploys); idempotency, audit event, and threat-model section added; ContainerAgent typestate now wraps the aggregate, not the projection.

---

## 1. Motivation

The redesign is a **foundation investment** dressed up as a small bug fix. We are honest about that posture: only one of the four findings below is user-visible today; the others are tech-debt closure that unlocks future product work (admin runtime filter, per-runtime telemetry, capability routing) and prevents an entire class of cross-runtime confusion incidents.

**User-visible problem (the bug):**

- `AgentContainerLifecyclePolicy::ensure_container_backed` checks only that `cli_tool` is set, so a Host CLI agent passes the gate and fails later when `container_id IS NULL` — surfacing a misleading "stale container reference" error to the operator. The same pattern exists across `start`, `stop`, and container control entry points.

**Latent structural problems (tech-debt closure, internal):**

- **Discriminator-by-prefix.** The application decides "is this a Host CLI agent?" by checking whether `agents.runtime_id` starts with the literal string `"host-"`. The runtime identifier doubles as the type tag.
- **Frontend type is half-implemented.** `AgentInfo.runtimeKind: AgentRuntimeKind` is declared but the backend never serializes that field. The store falls back to the prefix check.
- **No defense-in-depth.** Nothing at the database layer prevents inconsistent states such as `runtime_kind = 'api'` with `cli_tool` set, or a Host CLI row whose `container_id` got populated by mistake.

The redesign promotes the runtime kind from an implicit string-prefix signal to an explicit first-class discriminator with consistent enforcement across database, repository, domain, application, and UI layers. The naming and structure follow FAANG-scale conventions for STI-style polymorphic aggregates, Domain-Driven Design aggregate boundaries, and the project's Feature-Sliced Design frontend rules.

The shipped Host CLI enrollment flow (PR #298) and `CreateAgentModal` wiring (PR #410) are not changed in their externally observable behavior except for the bug fix above; the redesign is mostly internal correctness work.

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

The redesign adopts the canonical labels and values **already defined** in `docs/architecture/glossary.md` ("Runtime modes" table) and **already used** by the existing `agentforge_core::RuntimeKind` enum and the orchestrator's `runtime_capabilities` table. There is no new vocabulary in this spec — only consistent propagation of the existing terms across layers that currently disagree (frontend uses `'container-cli' | 'host-cli' | 'provider'`; backend uses prefix sniffing; glossary uses `cli | api | container`).

| Display label                | DB value      | JSON wire form | Meaning                                                                                                       |
| ---------------------------- | ------------- | -------------- | ------------------------------------------------------------------------------------------------------------- |
| **Container (Docker)**       | `'container'` | `"container"`  | Platform-spawned Docker container running a Container CLI (`claude`/`codex`/`gemini`/`opencode`) and sidecar. |
| **Host CLI (local process)** | `'cli'`       | `"cli"`        | Operator-managed CLI process on the operator's own machine; sidecar runs locally; joins via NATS.             |
| **API (direct LLM calls)**   | `'api'`       | `"api"`        | Provider-backed prompt agent; no container, no sidecar, no shell.                                             |

**The canonical DB value for the Host CLI runtime is `cli`, not `host_cli`.** Revision 1 of this spec invented `host_cli`; revision 2 reverts to the glossary's `cli` to avoid creating a parallel vocabulary and to reuse the existing `agentforge_core::RuntimeKind` enum (whose variants are `Container | Cli | Api`).

JSON-on-the-wire and DB-on-disk both use the **same** values (`"container" | "cli" | "api"`); no case-style translation. The frontend literal type is renamed in §5.2 to match.

`docs/architecture/glossary.md` requires no new entries — the redesign simply makes the runtime kind visible end-to-end where today it is hidden behind a prefix.

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

### 4.1 Core enum (reused, not duplicated)

`agentforge_core::RuntimeKind` already exists at `rust/crates/core/src/runtime_capability.rs:81` with the exact variants we need:

```rust
pub enum RuntimeKind {
    Container,
    Cli,    // the "Host CLI" runtime; the orchestrator and glossary call this "cli"
    Api,
}
```

The orchestrator's `runtime_capabilities` table and capability registry already use this enum with the literals `"container" | "cli" | "api"`. The redesign **reuses** it. A second parallel enum (`AgentRuntimeKind`) is rejected per DDD ubiquitous-language rules and per the Rust reviewer's finding (see §15 Decision Matrix, Rust C1).

We add (a) a thiserror error type for parse failures, (b) hand-rolled `sqlx::Type<Postgres>` + `Encode` + `Decode` impls so the enum maps to a plain `TEXT` column (not a Postgres ENUM TYPE), and (c) a strict serde Deserialize that rejects unknown values:

```rust
// rust/crates/core/src/runtime_capability.rs (extends existing file)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind { Container, Cli, Api }

#[derive(Debug, thiserror::Error)]
pub enum RuntimeKindError {
    #[error("unknown runtime kind: {0}")]
    Unknown(String),
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self { Self::Container => "container", Self::Cli => "cli", Self::Api => "api" }
    }
    pub fn parse(raw: &str) -> Result<Self, RuntimeKindError> {
        match raw.trim() {
            "container" => Ok(Self::Container),
            "cli"       => Ok(Self::Cli),
            "api"       => Ok(Self::Api),
            other       => Err(RuntimeKindError::Unknown(other.to_string())),
        }
    }
}

impl<'de> serde::Deserialize<'de> for RuntimeKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw: &str = serde::Deserialize::deserialize(d)?;
        Self::parse(raw).map_err(serde::de::Error::custom)
    }
}

// sqlx Postgres TEXT mapping (hand-rolled — derive macro's #[sqlx(type_name = "TEXT")]
// is for ENUM TYPEs, not free-form TEXT columns).
impl sqlx::Type<sqlx::Postgres> for RuntimeKind {
    fn type_info() -> sqlx::postgres::PgTypeInfo { <&str as sqlx::Type<sqlx::Postgres>>::type_info() }
}
impl sqlx::Encode<'_, sqlx::Postgres> for RuntimeKind {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer)
        -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError>
    { <&str as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf) }
}
impl sqlx::Decode<'_, sqlx::Postgres> for RuntimeKind {
    fn decode(v: sqlx::postgres::PgValueRef<'_>) -> Result<Self, sqlx::error::BoxDynError> {
        let raw: &str = <&str as sqlx::Decode<sqlx::Postgres>>::decode(v)?;
        Self::parse(raw).map_err(Into::into)
    }
}

impl From<RuntimeKindError> for crate::AppError {
    fn from(e: RuntimeKindError) -> Self { crate::ErrorKind::Validation(e.to_string()).into() }
}
```

**No legacy aliases.** `parse` is strict: only `"container" | "cli" | "api"` are accepted. There is no `"host_cli"` or `"host-cli"` alias because revision 1's invented value never reached production. AppSec C5 (input-confusion via alias acceptance) and the orchestrator collision concern (Rust C3) are both closed by strict parsing.

**API request structs add `#[serde(deny_unknown_fields)]`** so a malicious client cannot smuggle an unrecognized `runtimeKind` past validation — confirmed via test in §11.2.

### 4.2 Database schema (three-migration sequence)

The migration sequence is split into **three** files to close the rolling-deploy CHECK-violation hole (SRE C4) — an INSERT from an old API instance that does not write `runtime_kind` would otherwise be silently re-tagged via the DEFAULT and then fail the invariant CHECK at row write time.

**Why TEXT + CHECK rather than a native Postgres ENUM TYPE.** ENUM TYPEs add steps for every future variant via `ALTER TYPE ... ADD VALUE`, which cannot run inside a transaction and replicates poorly across logical-replication subscribers. TEXT + CHECK is the convention at FAANG-scale shops because it composes cleanly with online migrations. This paragraph is deliberately load-bearing — do not "improve" the column to a native ENUM in a future PR.

**Migration 062 — `agents_runtime_kind_column_and_backfill.sql`** (no CHECK yet). Lands FIRST. Idempotent. Backfills.

```sql
-- 062: add runtime_kind column, backfill from current shape.
-- CHECK constraints land in 063 AFTER new application code is fully deployed,
-- so a rolling-deploy window where old API code INSERTs without runtime_kind
-- does not crash on CHECK violation. The DEFAULT covers those old-code INSERTs.

SET lock_timeout    = '3s';
SET statement_timeout = '30s';

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS runtime_kind TEXT;

-- Backfill in batches via DO loop to keep WAL bounded on large tables.
DO $$
DECLARE
    batch_size INT := 5000;
    affected   INT;
BEGIN
    LOOP
        WITH targets AS (
            SELECT id FROM agents
            WHERE runtime_kind IS NULL
            LIMIT batch_size
            FOR UPDATE SKIP LOCKED
        )
        UPDATE agents a SET runtime_kind = CASE
            WHEN a.cli_tool IS NULL                                       THEN 'api'
            WHEN a.runtime_id IS NOT NULL AND a.runtime_id LIKE 'host-%'  THEN 'cli'
            ELSE                                                                'container'
        END
        FROM targets WHERE a.id = targets.id;
        GET DIAGNOSTICS affected = ROW_COUNT;
        EXIT WHEN affected = 0;
    END LOOP;
END $$;

-- Pre-flight assertion: any row that would later violate the invariant CHECK
-- must be visible NOW, before NOT NULL is set, so an operator can intervene.
DO $$
DECLARE
    bad_rows INT;
BEGIN
    SELECT COUNT(*) INTO bad_rows FROM agents
    WHERE NOT (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL)
    );
    IF bad_rows > 0 THEN
        RAISE EXCEPTION
            'Migration 062: % rows would violate agents_runtime_kind_invariants. '
            'Inspect with: SELECT id, runtime_kind, cli_tool, container_id, runtime_id '
            'FROM agents WHERE NOT (...invariant...). Resolve before 063 runs.',
            bad_rows;
    END IF;
END $$;

ALTER TABLE agents
    ALTER COLUMN runtime_kind SET NOT NULL,
    ALTER COLUMN runtime_kind SET DEFAULT 'api';
```

**Migration 063 — `agents_runtime_kind_check.sql`** (CHECKs only). Lands AFTER the new application code is fully deployed. Held back from 062 to avoid CHECK-violations on rolling deploys where an old API instance still writes rows without `runtime_kind` (the DEFAULT lands them as `'api'`, but if `cli_tool IS NOT NULL` on that row, the invariant CHECK fires). 063 runs only when every running API instance has the new column-aware code.

```sql
-- 063: add the enum CHECK and the joint-invariant CHECK.
-- Two-phase: NOT VALID lands the constraint instantly under a brief
-- AccessExclusive metadata lock; VALIDATE scans without blocking writes.

SET lock_timeout    = '3s';
SET statement_timeout = '30s';

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_check
    CHECK (runtime_kind IN ('container', 'cli', 'api')) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_invariants
    CHECK (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL)
    ) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;
```

**Migration 064 — `agents_runtime_kind_index.sql`** (index only). Lands at operator's leisure (no functional dependency). Held back from 062/063 so the schema change ships immediately and the index build runs in a separate maintenance window.

```sql
SET lock_timeout = '10s';
CREATE INDEX IF NOT EXISTS idx_agents_runtime_kind ON agents(runtime_kind);

-- runtime_id uniqueness for host_cli agents — closes the AppSec C3 / Rust C7
-- collision concern. Partial UNIQUE: API/container rows may have NULL runtime_id.
CREATE UNIQUE INDEX IF NOT EXISTS uq_agents_runtime_id
    ON agents(runtime_id)
    WHERE runtime_id IS NOT NULL;
```

A schema-contract test (§11.1) validates that fresh-DB-after-all-three matches a snapshot, and that the backfill UPDATE produces the expected post-state on a representative legacy dataset.

### 4.3 Aggregate boundary, factories, and typestate

The Agent is the aggregate root. `runtime_kind` is a value-object attribute of the Agent, constrained jointly with `cli_tool` and `container_id`. Host CLI enrollment is a factory operation on the Agent aggregate, not a separate aggregate, so the enrollment service performs a single atomic INSERT that writes `runtime_kind`, `cli_tool`, `runtime_id`, `hmac_secret`, `nats_connect_password`, **and** an `events` audit row in the same transaction — no follow-up UPDATE.

#### 4.3.1 Write-side: `NewAgent` factories

Per DDD convention (Distinguished Engineer review, DDD C2), the construction intent is named `NewAgent`, not `AgentDraft`. "Draft" implies an editable working copy; `NewAgent` is a validated, immutable creation intent.

```rust
// rust/crates/api/src/domain/agent.rs (new section)

pub struct NewAgent { /* private fields; only constructible via factories below */ }

impl NewAgent {
    pub fn container(scope: &TenantScope, cli_tool: CliToolKind, ...)
        -> AppResult<Self> { /* validates, returns Ok(Self) or Err(Validation) */ }

    pub fn host_cli(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        identity: HostCliIdentity,
        ...
    ) -> AppResult<Self> { /* validates */ }

    pub fn api(scope: &TenantScope, provider: &str, model: &str, ...)
        -> AppResult<Self> { /* validates */ }
}

// HostCliIdentity is generated server-side BEFORE the INSERT, using a
// client-side-generable UUID (v7) for the Agent PK so runtime_id can be
// derived from agent_id at construction time, not after INSERT.
pub struct HostCliIdentity {
    agent_id: Uuid,                  // Uuid::now_v7()
    runtime_id: String,              // FULL agent_id, prefixed: format!("host-{}", agent_id)
    hmac_secret: String,             // Uuid::new_v4().to_string() — 122 bits
    nats_connect_password: String,   // Uuid::new_v4().to_string()
}

impl HostCliIdentity {
    pub fn generate() -> Self {
        let agent_id = Uuid::now_v7();
        Self {
            agent_id,
            runtime_id: format!("host-{agent_id}"),  // FULL UUID, not truncated
            hmac_secret: Uuid::new_v4().to_string(),
            nats_connect_password: Uuid::new_v4().to_string(),
        }
    }
}
```

**`runtime_id` is the full UUID, not a truncated 8-char prefix.** Revision 1's `format!("host-{}", &uuid.to_string()[..8])` gave 32 bits of identity, which the security reviewers showed is insufficient (AppSec C3, Platform C5). Combined with the partial UNIQUE index in migration 064, `runtime_id` collisions are eliminated. Human-friendly short labels for operator UX are produced as a separate display projection, never as the security identifier.

#### 4.3.2 Read-side: separation of aggregate from list projection

The DDD reviewer (C3) flagged that wrapping `AgentListItem` (a JOIN-augmented query projection used for the list endpoint) in `ContainerAgent` conflates an aggregate with a read model. Revision 2 splits these:

- `AgentAggregate` — the write-side aggregate root. Loaded by `AgentRepository::find_aggregate(scope, id)` which does a single-row SELECT without JOINs. Owns invariants. The only type returned to lifecycle/control services.
- `AgentListItem` — the read-side query projection. Unchanged from current code; used by `list_with_owner` and other JOIN endpoints. Read-only.
- `ContainerAgent(AgentAggregate)` — typestate wrapper. Private constructor, can only be obtained via `ContainerAgent::try_from(agent: AgentAggregate)`.

```rust
pub struct ContainerAgent(AgentAggregate);  // private constructor

#[derive(Debug)]
pub enum LifecycleRejection { HostCli, Api }

impl LifecycleRejection {
    pub fn into_app_error(self, action_verb: &str) -> AppError { /* see §7 */ }
}

impl ContainerAgent {
    pub fn try_from(agent: AgentAggregate) -> Result<Self, LifecycleRejection> {
        match agent.runtime_kind() {
            RuntimeKind::Container => Ok(ContainerAgent(agent)),
            RuntimeKind::Cli       => Err(LifecycleRejection::HostCli),
            RuntimeKind::Api       => Err(LifecycleRejection::Api),
        }
    }
    pub fn inner(&self) -> &AgentAggregate { &self.0 }
}
```

`AgentContainerLifecycleService::{restart, start, stop}` take `&ContainerAgent`. A non-container agent cannot reach Docker by construction. Lifecycle services reload the aggregate (not the projection) at the start of each operation so the typestate guard sees the latest committed state, not stale data from a list endpoint.

#### 4.3.3 Repository contract

`AgentRepository::create(scope, new: NewAgent) -> AppResult<AgentAggregate>` is the **only** insertion path. The repository extracts the validated fields and writes one row plus the audit event in a single transaction (see §4.3.4). The previous `set_host_runtime` UPDATE is removed; Host CLI rows are complete on INSERT. Every repository method takes `&TenantScope` (compile-checked by review; see AppSec C6).

`AgentRepository::find_aggregate(scope, id) -> AppResult<AgentAggregate>` returns the aggregate for write-side operations.

`AgentListItem` and JOIN-based read queries (`list_with_owner`, `find_with_owner_by_id`) live in a separate `AgentReadModel` / `AgentQueryService` module per CQRS hygiene (DDD C7). `find_by_runtime_kind` lives in the query service, not the write-side repository.

#### 4.3.4 Atomic enrollment with audit event

Per AppSec C7, host CLI enrollment must emit a forensic audit event (not just a tracing span) **in the same transaction** as the INSERT, so a crash or rollback cannot leave a "row exists, audit missing" or "audit exists, row missing" state:

```rust
// Inside AgentRepository::create, when NewAgent::host_cli:
let mut tx = pool.begin().await?;
sqlx::query("INSERT INTO agents (id, runtime_kind, cli_tool, runtime_id, hmac_secret, nats_connect_password, status, ...) VALUES ($1, 'cli', ...)")
    .execute(&mut *tx).await?;
sqlx::query("INSERT INTO events (event_type, agent_id, actor_user_id, payload, source_ip, user_agent, ...) VALUES ('agent.enrolled', $1, $2, $3, $4, $5, ...)")
    .execute(&mut *tx).await?;
tx.commit().await?;
```

The audit event payload includes `runtime_kind`, `workspace_id`, `project_id`, `cli_tool`, and the source-IP / user-agent that came through the auth middleware. The HMAC secret and NATS password are **not** in the audit payload.

#### 4.3.5 Deferred DDD refinements

The DDD reviewer also recommended (C1) modeling `AgentRuntime` as a sum type where each variant carries its own VO fields (`Container { cli_tool, image }`, `HostCli { runtime_id, hmac_secret }`, `Api { provider, model }`), and (C6) introducing a parallel typestate `EnrolledHostCli` for NATS-bound operations. Both are **deferred to a follow-up phase** as they are sizeable refactors that extend the redesign rather than complete it. They are tracked in §14 (Out of Scope).

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

- `export type AgentRuntimeKind = 'container' | 'cli' | 'api'` — matches the glossary and the Rust core enum. Replaces the existing `'container-cli' | 'host-cli' | 'provider'` literal in `src/app/shared/model/agents.store.ts` (Frontend reviewer C3 + ubiquitous language).
- `interface AgentListItem.runtimeKind?: AgentRuntimeKind` — **optional for one release cycle** to keep old frontend deployments alive during the rolling deploy (Frontend C4 + SRE C4). The next release after backend reliably emits the field promotes it to required.
- `CreateAgentRequest.runtimeKind?: AgentRuntimeKind` (optional, server derives if absent).
- `HostAgentEnrollment.runtimeKind: 'cli'` (typed literal for clarity).

The store's client-side `managedToAgentInfo` mapping in `agents.store.ts` (lines 114–118) keeps a `cliTool ? 'container' : 'api'` fallback for one release so PRs that haven't been redeployed do not display `undefined`.

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

Note: the Zustand `agents.store` moves into the entity layer in rev 2 (Frontend C1). It is single-entity state, not cross-slice, and keeping it in `shared/` would force a shared→entities upward import that the project's FSD checker forbids (Frontend C2).

```
src/app/entities/agent/
  model/
    types.ts            // AgentInfo, AgentRuntimeKind, AgentStatus, CliTool
    runtime-kind.ts     // isHostCliAgent / isContainerAgent / isApiAgent specifications
    agents.store.ts     // Zustand store (moved from src/app/shared/model/)
  api/
    AgentAPI.ts         // moved from src/app/shared/api/legacy/AgentAPI.ts
  index.ts              // public re-exports (barrel)
```

`isHostCliAgent` becomes a one-line specification keyed on the canonical value `'cli'`:

```typescript
// src/app/entities/agent/model/runtime-kind.ts
import type { AgentInfo } from './types'

export const isHostCliAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'cli'
export const isContainerAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'container'
export const isApiAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'api'
```

The `runtimeId.startsWith('host-')` fallback is removed. Old literal `'container-cli' | 'host-cli' | 'provider'` is renamed to `'container' | 'cli' | 'api'` across all callers (search-and-replace).

Files updated to import from `@app/entities/agent`:

- `src/app/features/agents/AgentConfigTab.tsx`
- `src/app/features/agents/AgentControlPanel.tsx`
- `src/app/features/agents/AgentListView.tsx`
- `src/app/features/agents/AgentCard.tsx`
- `src/app/features/agents/AgentKindBadge.tsx`
- `src/app/widgets/agent-detail/AgentDetailView.tsx`
- `src/app/pages/getting-started/ui/GettingStartedView.tsx`
- `src/app/features/agents/CreateAgentModal.tsx`
- all `tests/unit/app/*` files referencing `AgentInfo` / `isHostCliAgent` / the old literals

The legacy `src/app/shared/api/legacy/AgentAPI.ts` and `src/app/shared/model/agents.store.ts` are removed once all imports point at the entity barrel. The custom FSD checker `scripts/check-fsd-boundaries.mjs` will reject any straggler.

**No Zustand persistence:** the current store uses no `persist`/`createJSONStorage`, so no localStorage migration is required. Footnote in the moved store warns future contributors not to add `persist()` without also adding a state-version migration step.

**Vitest config unchanged:** the existing `unit-app` project pattern `tests/unit/app/**/*.test.tsx?` is directory-recursive, so `tests/unit/app/entities/agent/runtime-kind.test.ts` is picked up without config change.

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
             Headers: Authorization: Bearer <jwt>, Idempotency-Key: <uuid>
             (Idempotency-Key required by spec; if missing, server returns 400)
Routes:      routes/agents.rs::enroll_local_agent
             - look up (org_id, user_id, idempotency_key) in enrollment_idempotency
               table; if a prior row exists within 24h TTL, return the original
               response without re-creating (AppSec C2 + Architect C4)
Service:     HostAgentEnrollmentService::enroll
             - validate cli_tool, name, nats_base_url
             - REJECT non-tls:// nats_base_url unless allow_plaintext_host_nats
               org-policy flag is set (Platform C3)
             - resolve workspace mount scope
             - identity = HostCliIdentity::generate()
               (agent_id = Uuid::now_v7(); runtime_id = format!("host-{agent_id}"))
             - new_agent = NewAgent::host_cli(scope, cli_tool, identity, ...)
Repository:  AgentRepository::create(scope, new_agent) — SINGLE TRANSACTION:
             1. INSERT INTO agents (id, runtime_kind, cli_tool, runtime_id,
                  hmac_secret, nats_connect_password, status, ...)
                VALUES ($1, 'cli', 'codex', $2, $3, $4, 'offline', ...)
                  -- runtime_kind set to 'cli' on INSERT before migration 063
                  -- adds the CHECK; rolling-deploy safe.
             2. INSERT INTO events (event_type, agent_id, actor_user_id,
                  workspace_id, payload, source_ip, user_agent, ts)
                VALUES ('agent.enrolled', $1, $5, $6, jsonb_build_object(
                  'runtime_kind', 'cli',
                  'cli_tool',    'codex',
                  'project_id',  $7
                ), $8, $9, NOW())
             3. INSERT INTO enrollment_idempotency (org_id, user_id, key,
                  agent_id, expires_at)
                VALUES ($10, $5, $11, $1, NOW() + INTERVAL '24 hours')
                ON CONFLICT DO NOTHING  -- defense in depth; pre-check already did this
             4. COMMIT
Service:     - AgentContainerEnvPolicy::build(...) — env vars include
               AGENTFORGE_RUNTIME_KIND="cli" as ADVISORY metadata only;
               server NEVER trusts this value back (see Platform C7 in §16)
Response:    Headers: Cache-Control: no-store, Pragma: no-cache  (AppSec C1)
             Body: { ok, agent: {..., runtimeKind: "cli", runtimeId: "host-<full-uuid>"},
                    enrollment: { env, shellExports, sidecarCommand, serverUrl } }
             Access-log filter MUST exclude this path's response body from
             logs / proxy logs / Sentry breadcrumbs (AppSec C1).
```

DB CHECK invariants pass: `runtime_kind = 'cli'` with `cli_tool NOT NULL` and `container_id NULL`. Per AppSec C7, the `agent.enrolled` event row is part of the SAME transaction as the agent INSERT.

### 6.4 Container lifecycle: restart with Host CLI rejection

```
Frontend:    POST /api/v1/agents/:id/restart
Service:     AgentContainerLifecycleService::restart
             - agent = AgentRepository::find_aggregate(scope, id)
                 (write-side aggregate, NOT the AgentListItem projection)
             - container = ContainerAgent::try_from(agent)?
                 // returns Err(LifecycleRejection::HostCli) for runtime_kind='cli'
                 // returns Err(LifecycleRejection::Api) for runtime_kind='api'
             - on Err: map via LifecycleRejection::into_app_error to 422 with
               operator-facing message (see §7)
             - on Ok(container): docker.inspect → docker.stop → docker.start
```

The same pattern applies to `start`, `stop`, `clear_container`, and any other Docker-backed lifecycle method. `ContainerAgent` is the only type accepted by these methods. Per AppSec C8, lifecycle endpoints additionally check per-agent ACL before disclosing runtime kind in the error message — non-owner intra-org callers receive uniform `403 "operation not permitted on this agent"` instead of the runtime-specific guidance, blocking the enumeration vector.

### 6.5 List or read an agent

```
GET /api/v1/agents → SELECT a.*, ..., a.runtime_kind FROM agents a JOIN ...
sqlx::query_as<AgentListItem> — runtime_kind parsed via hand-rolled Decode
                                 into core::RuntimeKind enum
serde → JSON emits "container" | "cli" | "api" (matches glossary)
Frontend store reads agent.runtimeKind, isHostCliAgent returns a single enum match
```

`events` and WebSocket broadcasts continue to use existing payload schemas; the runtime kind ships in the agent object embedded in those payloads.

## 7. Error Handling

All operator-facing rejections are `ErrorKind::Validation` mapped to HTTP 422 with structured `{ ok: false, error: { code: <i18n-key>, message: <fallback English>, detail?: <long form> } }`. PM reviewer C6: previous one-liner messages would overflow toasts and skip i18n. Revision 2: every message has a short toast title (≤60 chars), an i18n key for `en` and `zh` resource bundles, and an expandable long-form `detail` field for the modal description.

| Scenario                                             | i18n key                                             | Toast title (en)                          | Detail (en)                                                                                                                        |
| ---------------------------------------------------- | ---------------------------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| restart on cli runtime                               | `errors.agent.lifecycle.restart_host_cli`            | "Restart the sidecar from your machine"   | The platform does not manage the local sidecar. Re-run the enrollment shell script on the operator machine.                        |
| restart on api runtime                               | `errors.agent.lifecycle.restart_api`                 | "No container to restart"                 | This agent calls the LLM provider directly. Send a new prompt to invoke the model again.                                           |
| start on cli runtime                                 | `errors.agent.lifecycle.start_host_cli`              | "Start the sidecar from your machine"     | Re-run the enrollment shell script on the operator machine to launch the sidecar.                                                  |
| start on api runtime                                 | `errors.agent.lifecycle.start_api`                   | "No container to start"                   | Provider agents have no shell to start.                                                                                            |
| stop on cli runtime                                  | `errors.agent.lifecycle.stop_host_cli`               | "Stop the sidecar from your machine"      | The platform cannot stop a remote sidecar. Stop the process on the operator machine.                                               |
| stop on api runtime                                  | `errors.agent.lifecycle.stop_api`                    | "No container to stop"                    | Provider agents have no shell to stop.                                                                                             |
| create container without cli_tool                    | `errors.agent.create.missing_cli_tool_for_container` | "Choose a CLI tool"                       | Container-backed agents need a Container CLI: claude, codex, gemini, or opencode.                                                  |
| create api with cli_tool                             | `errors.agent.create.api_cannot_have_cli_tool`       | "Provider agent cannot have a CLI tool"   | Remove the CLI tool, or change the runtime to "Container (Docker)".                                                                |
| create cli without cli_tool                          | `errors.agent.create.missing_cli_tool_for_host_cli`  | "Choose a CLI tool"                       | Host CLI enrollment needs a Container CLI: claude, codex, gemini, or opencode.                                                     |
| enrollment missing `Idempotency-Key`                 | `errors.agent.enroll.missing_idempotency_key`        | "Idempotency-Key header required"         | Resend with a fresh UUID in the `Idempotency-Key` header.                                                                          |
| enrollment with plaintext NATS without policy opt-in | `errors.agent.enroll.plaintext_nats_blocked`         | "Plaintext NATS not allowed for Host CLI" | Configure `NATS_AGENT_URL` to use `tls://`, or set the org policy `allow_plaintext_host_nats=true` to permit it.                   |
| non-owner intra-org lifecycle call                   | `errors.agent.lifecycle.not_permitted`               | "Operation not permitted on this agent"   | (Uniform 403, no runtime-kind disclosure — AppSec C8.)                                                                             |
| DB CHECK violation                                   | `errors.internal.db_invariant_violation`             | "Unexpected error"                        | Internal — emits `tracing::error` with agent_id, runtime_kind, cli_tool, container_id (no secrets). Operator opens support ticket. |

Sensitive material (hmac_secret, nats_connect_password) is never logged or echoed in error bodies, audit events, or tracing spans. Tests in §11 assert this via fixture inspection.

`src/app/shared/i18n/locales/en.ts` and `zh.ts` gain the new keys in the same PR. PM reviewer expects exact translations for `zh`; spec assumes the project's existing translation workflow.

## 8. Migration Safety and Rollout

Migration sequence is split into three files (see §4.2). The split is the **critical fix for SRE C4** (rolling-deploy CHECK violation): the joint-invariant CHECK is held back from 062 to 063 so that old API instances still writing rows during the rolling deploy cannot trigger a CHECK violation (the DEFAULT keeps the column populated; CHECK only enforces after every node speaks the new vocabulary).

| Migration                               | Operation                                | Lock                                                             | Mitigation in rev 2                                                                                                                                                                                                                  |
| --------------------------------------- | ---------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 062 — `ADD COLUMN ... TEXT NULL`        | metadata-only column add                 | AccessExclusive (brief)                                          | `SET lock_timeout = '3s'; SET statement_timeout = '30s';` (SRE C1)                                                                                                                                                                   |
| 062 — batched backfill UPDATE           | RowExclusive                             | RowExclusive (per row)                                           | `DO` loop with 5000-row `FOR UPDATE SKIP LOCKED` batches; `agentforge migrate doctor` pre-flight rejects without `--force` if row count exceeds 100k (Architect C3 + SRE C3)                                                         |
| 062 — pre-flight invariant assertion    | SELECT COUNT(\*) on candidate violations | SELECT only                                                      | Migration `RAISE EXCEPTION` and aborts if any row would later fail the invariant CHECK; operator playbook in runbook                                                                                                                 |
| 062 — `SET NOT NULL` + `SET DEFAULT`    | full-table verification                  | AccessExclusive (verification)                                   | `lock_timeout` aborts on contention; runbook describes off-peak retry (SRE C1)                                                                                                                                                       |
| **rolling deploy of new app code**      | _between 062 and 063_                    | n/a                                                              | Every running API instance must speak the new vocabulary before 063 ships. Pre-flight: `kubectl/docker ps` confirms unified version. (SRE C4)                                                                                        |
| 063 — `ADD CHECK ... NOT VALID`         | enum + invariant CHECKs                  | NOT VALID: brief AccessExclusive. VALIDATE: ShareUpdateExclusive | Two-phase per FAANG canon. VALIDATE does not block writes; logical replicas need direct VALIDATE on each subscriber (Architect C6).                                                                                                  |
| 064 — `CREATE INDEX` (regular + UNIQUE) | index build                              | ShareUpdateExclusive                                             | Held back into a separate maintenance window; `CONCURRENTLY` will be added when the project's migration runner supports it (SRE C2). The partial UNIQUE INDEX on `runtime_id` closes the collision concern from AppSec C3 / Rust C7. |

The migration sequence is forward-only per CLAUDE.md "do not edit migrations that have run." Scope reductions land as `065_relax_*.sql` rather than edits to 062/063/064.

A schema-contract test (`rust/crates/api/tests/agents_runtime_kind_constraint.rs`) asserts (a) the migration produces the expected post-state on a fresh database, (b) the backfill maps a representative legacy dataset correctly, and (c) every (runtime_kind × cli_tool × container_id) invalid combination is rejected by the CHECK.

### 8.1 Zero-downtime rollout sequence

1. Deploy 062 (column + backfill, no CHECK). Old + new application instances coexist; the DEFAULT keeps rows valid.
2. Verify every running API instance is on the new release (operator runs `agentforge cluster version`); old API instances retired.
3. Deploy 063 (CHECK constraints). At this point every INSERT writes the correct `runtime_kind` and CHECK passes.
4. Deploy 064 (indexes) in a maintenance window.
5. After confidence, remove the `runtimeKind?:` optional fallback in the frontend (next release cycle).

### 8.2 Pre-flight: `agentforge migrate doctor`

Per SRE C3, the Platform CLI gains a `migrate doctor` subcommand (`rust/bins/cli`) that operators run BEFORE migrating:

- `SELECT COUNT(*) FROM agents` — refuse migration without `--force` if `> 100_000`
- `SELECT COUNT(*) FROM agents WHERE cli_tool IS NOT NULL AND runtime_id NOT LIKE 'host-%' AND container_id IS NULL` — reports rows that the backfill cannot disambiguate
- `SELECT version FROM information_schema.tables WHERE table_name='agents'` — prints PG version
- Estimates the lock duration based on row count
- Verifies all replication subscribers are reachable

### 8.3 Failure-mode playbooks

`docs/runbooks/migration-062-runtime-kind.md` (new) covers:

- **`SET NOT NULL` lock timeout:** migration aborts cleanly; re-run off-peak.
- **`VALIDATE CONSTRAINT` fail on legacy row:** SQL fragment to enumerate offenders, manual remediation steps, then `ALTER TABLE ... VALIDATE CONSTRAINT`.
- **Restore from pre-062 backup into post-062 DB:** drop the two CHECK constraints, restore, re-run backfill UPDATE, re-add the CHECK constraints with NOT VALID + VALIDATE.
- **Partial-application recovery:** `IF NOT EXISTS` + `WHERE runtime_kind IS NULL` guards make re-run safe; runbook documents the resume path.

### 8.4 Rollback

Forward-only. Rollback = redeploy the prior application binary while the column remains. New rows written by the new code retain `runtime_kind` even after the binary is rolled back; the prior binary ignores the column. The sqlx Decode is strict against unknown values, so any _future_ enum addition cannot be deployed without code that knows it — that fail-closed behavior is intentional and documented as a known constraint.

### 8.5 Logical-replication caveat (Architect C6)

`VALIDATE CONSTRAINT` on the primary does **not** automatically validate the same CHECK on logical-replication subscribers. Each subscriber's DBA must run `ALTER TABLE ... VALIDATE CONSTRAINT agents_runtime_kind_check` and `... agents_runtime_kind_invariants` directly. The runbook calls this out.

## 9. Defense-in-Depth Layers

- **Layer 1 — DB CHECK constraints.** Impossible `(runtime_kind, cli_tool, container_id)` combinations cannot be persisted.
- **Layer 2 — sqlx enum decoding.** A row with a value outside the enum (corrupt data or out-of-band write) fails to decode and surfaces as an internal error with full context.
- **Layer 3 — Domain factories + typestate.** `NewAgent` validates intent at construction; `ContainerAgent::try_from` ensures non-container agents cannot reach Docker.
- **Layer 4 — Frontend types.** `AgentRuntimeKind` is the canonical `'container' | 'cli' | 'api'`; TS exhaustive matching surfaces dead cases at compile time.

## 10. Observability

### 10.1 Tracing

- `tracing` spans on lifecycle/enrollment add a structured `agent.runtime_kind` field on the relevant scopes (`agents.restart`, `agents.start`, `agents.stop`, `agents.local-enroll`).
- Sensitive fields (`hmac_secret`, `nats_connect_password`) are `#[serde(skip_serializing)]` and explicitly NOT included in any tracing span fields. A unit test (`tests/unit/api/tracing_redaction_test.rs`) asserts this via fixture inspection (AppSec C1).
- `events` table payloads carrying agent objects pick up `runtime_kind` automatically via the new sqlx serialization; downstream consumers (frontend WebSocket reducers) gain visibility without protocol additions.
- Migration runner emits one INFO log on completion per the existing pattern.

### 10.2 Audit events

Per AppSec C7 + DDD C5, the `events` table gains a new event type `agent.enrolled`, written in the **same transaction** as the agent INSERT. Payload:

```jsonc
{
  "event_type": "agent.enrolled",
  "agent_id": "<uuid>",
  "actor_user_id": "<uuid>",
  "workspace_id": "<uuid>",
  "payload": { "runtime_kind": "cli", "cli_tool": "codex", "project_id": "<uuid|null>" },
  "source_ip": "<auth-middleware-extracted>",
  "user_agent": "<auth-middleware-extracted>",
  "ts": "<iso8601>",
}
```

This audit row is forensic-grade (immutable, append-only, scoped to org) — distinct from the tracing telemetry above. HMAC secret and NATS password are NOT in the payload.

### 10.3 SLO (added in rev 2 per SRE C7)

| Endpoint                                      | SLO                                                                 |
| --------------------------------------------- | ------------------------------------------------------------------- |
| `POST /api/v1/agents` (create container)      | p95 < 500ms, success > 99.5% over 28-day window                     |
| `POST /api/v1/agents/local-enroll`            | p95 < 800ms, success > 99.5% over 28-day window                     |
| `POST /api/v1/agents/:id/restart` (container) | p95 < 2s (Docker bound), success > 99.0%                            |
| Enrollment-to-first-heartbeat funnel          | > 95% of `agent.enrolled` events see a sidecar heartbeat within 60s |

### 10.4 Metrics counters (dashboards deferred per §14)

Exposed via the existing metrics endpoint so a future dashboard PR renders without a backend change:

- `agents_created_total{runtime_kind}` — counter
- `agents_lifecycle_rejected_total{runtime_kind,action}` — counter (host_cli / api rejections)
- `agents_enrolled_total{cli_tool}` — counter
- `agents_check_constraint_violations_total` — counter (alert-on-nonzero)
- `agents_idempotency_replay_total` — counter

## 11. Testing Strategy

### 11.1 Database schema-contract tests

`rust/crates/api/tests/agents_runtime_kind_constraint.rs` (new):

- Boot SQLx test pool, run migrations 062 + 063 + 064.
- Assert all 9 combinations of `(runtime_kind, cli_tool, container_id)`:
  - `('container', NOT NULL, *)` insert OK.
  - `('container', NULL, *)` rejected.
  - `('cli', NOT NULL, NULL)` insert OK.
  - `('cli', NOT NULL, NOT NULL)` rejected.
  - `('cli', NULL, *)` rejected.
  - `('api', NULL, NULL)` insert OK.
  - `('api', NOT NULL, *)` rejected.
  - `('api', NULL, NOT NULL)` rejected.
  - `('bogus', *, *)` rejected by enum CHECK.
- Backfill test: seed pre-migration rows that represent legacy container / `host-` prefix / api shapes, run migrations on a fresh database, assert each row gains the correct `runtime_kind`.
- **Rolling-deploy CHECK-violation scenario test (SRE C4):** apply 062 only; INSERT a row through old-code SQL that omits `runtime_kind` but has `cli_tool` set; assert the row lands with default `'api'` and DOES NOT crash (because 063 has not run yet). Apply 063 and assert the same INSERT now fails with CHECK violation, demonstrating the rolling-deploy hazard was averted.
- **Partial UNIQUE index test (AppSec C3 / Rust C7):** assert two host_cli rows with the same `runtime_id` are rejected; assert two container rows both with `runtime_id IS NULL` are accepted.

### 11.2 Core enum tests

In `rust/crates/core/src/runtime_capability.rs` `#[cfg(test)]` module:

- `parse`: case sensitivity (strict lowercase per FAANG convention), trimming, **rejection** of `"cli"`-adjacent legacy spellings (`"host-cli"`, `"host_cli"`) — they MUST return `RuntimeKindError::Unknown` (AppSec C5 + Rust C3).
- `as_str` round-trip with `parse`.
- Strict serde Deserialize round-trip: `{"runtimeKind": "cli"}` accepts; `{"runtimeKind": "host_cli"}` returns 422.
- `deny_unknown_fields` on `CreateAgentRequest` rejects `{"runtimeKind": "cli", "rogue": "x"}`.
- sqlx encode/decode round-trip through a real `text` column.

### 11.3 Domain policy and factory tests

In `rust/crates/api/src/domain/agent.rs` `#[cfg(test)]`:

- `NewAgent::container` validates cli_tool and rejects empty name beyond 255 chars.
- `NewAgent::host_cli` requires non-empty `HostCliIdentity` fields and rejects empty cli_tool. `runtime_id` is the full UUID, not truncated.
- `NewAgent::api` rejects empty model.
- `ContainerAgent::try_from`: 3 kinds × expected variant (Ok / Err::HostCli / Err::Api). Error variants carry the i18n key text.

### 11.4 Repository tests

In `rust/crates/api/src/repositories/agent/` (extend existing `tests.rs`):

- `create(new: NewAgent)` writes correct `runtime_kind` for each `NewAgent::{container,host_cli,api}` variant.
- For `NewAgent::host_cli`, the INSERT and the `agent.enrolled` event row are committed atomically; a forced rollback yields neither (AppSec C7 + DDD C5).
- `list_with_owner` and `find_with_owner_by_id` return `AgentListItem.runtime_kind` parsed as `core::RuntimeKind`.
- `AgentQueryService::find_by_runtime_kind` returns only matching rows, scoped by tenant (`&TenantScope` required).
- Compile-time test (or doc-test): a repository method without `&TenantScope` in its signature fails to compile under a project-level `clippy` lint or a custom-derived macro guard. (AppSec C6.)

### 11.5 Service / route integration tests

In `rust/crates/api/tests/agent_lifecycle_routes.rs` (extend):

- `POST /api/v1/agents` body `{ cliTool: "codex" }` → response `runtimeKind = "container"`.
- `POST /api/v1/agents` body `{ provider: "anthropic", model: "claude-opus-4-7" }` → response `runtimeKind = "api"`.
- `POST /api/v1/agents/local-enroll` body `{ cliTool: "codex", ... }` with `Idempotency-Key: <uuid>` → response `runtimeKind = "cli"`, `runtimeId = "host-<full-uuid>"`, body contains `shellExports`. Response headers include `Cache-Control: no-store`.
- `POST /api/v1/agents/local-enroll` with the SAME `Idempotency-Key` within 24h → returns the ORIGINAL agent_id; only one row exists in `agents` table; only one `agent.enrolled` event exists.
- `POST /api/v1/agents/local-enroll` WITHOUT the `Idempotency-Key` header → 400 + i18n key `errors.agent.enroll.missing_idempotency_key`.
- `POST /api/v1/agents/local-enroll` with `nats://...` (plaintext) URL and no `allow_plaintext_host_nats` policy → 422 + i18n key `errors.agent.enroll.plaintext_nats_blocked`.
- `POST /api/v1/agents/:id/restart` on `runtime_kind='cli'` agent → 422 + i18n key from §7.
- `POST /api/v1/agents/:id/restart` on `runtime_kind='api'` agent → 422 + i18n key from §7.
- `POST /api/v1/agents/:id/restart` on container agent without `container_id` → existing stale-container behavior, unchanged.
- Tenant-scope isolation: cross-org access to cli agents still 404.
- Intra-org non-owner access to cli/api lifecycle → 403 + uniform i18n key (no runtime-kind disclosure, AppSec C8).
- `POST /api/v1/agents` with `{"runtimeKind": "host_cli"}` (legacy alias) → 422 (strict parse rejects, AppSec C5 + Rust C3).
- `POST /api/v1/agents` with `{"runtimeKind": "cli", "rogue": "x"}` (deny_unknown_fields) → 422.

### 11.6 Frontend tests (Vitest)

- `tests/unit/app/entities/agent/runtime-kind.test.ts` (new): `isHostCliAgent` / `isContainerAgent` / `isApiAgent` exhaustive table. A `runtimeKind: 'container'` agent whose `runtimeId` accidentally starts with `host-` returns `false` (prefix fallback gone). A `runtimeKind: undefined` agent during the one-cycle backward-compat window returns `false` for all three (no crash).
- `tests/unit/app/entities/agent/agents-store.test.ts`: store maps server response `runtimeKind: 'cli'` correctly, falls back to `cliTool ? 'container' : 'api'` when server omits the field (rolling-deploy safety, Frontend C4).
- `tests/unit/app/AgentControlPanel.test.tsx`, `AgentListView.test.tsx`, `AgentCard.test.tsx`, `AgentKindBadge.test.tsx`: fixtures replace `runtimeKind: 'host-cli'` / `'container-cli'` / `'provider'` with the canonical `'cli'` / `'container'` / `'api'`. Add at least one `runtimeKind: 'api'` case verifying that no restart button or terminal tab renders.

### 11.7 End-to-end tests (Playwright)

- `tests/e2e/specs/host-cli-enrollment.spec.ts` (extend or add): create a Host CLI agent via the modal, assert response payload contains `runtimeKind: 'cli'`, navigate to the agent list and confirm the "Host CLI" badge, attempt to use Container CLI restart UI on the new agent and confirm the operator-facing 422 + i18n message renders.

### 11.8 FSD boundary verification

`npm run fsd:check` (custom script `scripts/check-fsd-boundaries.mjs`) must stay green. The CI lint step gates this; CI configuration is unchanged. The move of `agents.store.ts` into `entities/agent/` eliminates the would-be shared→entities upward import.

### 11.9 Manual validation checklist (post-deploy on staging)

- `agentforge migrate doctor` reports row count below threshold and no invariant violations.
- `make prod-ext` brings up the stack with migrations 062 + 063 + 064 applied.
- `psql` query: `SELECT runtime_kind, COUNT(*) FROM agents GROUP BY 1;` returns no NULLs, expected distribution.
- Web UI: create one Container agent, one Host CLI agent (via modal), one Provider agent. Each shows the correct runtime label and the audit trail at `events WHERE event_type='agent.enrolled'` has the matching entry for the host_cli case.
- Web UI: restart attempts on Host CLI and Provider agents render the new operator-facing i18n messages.
- Replay test: copy the `Idempotency-Key` from a successful enrollment request and replay within 24h; assert no second row is created.

## 12. Open Questions (resolved in rev 2)

Per PM C5, open questions are resolved inline before approval — not left dangling.

1. **`HostCliIdentity` location?** Resolved: **stays in `rust/crates/api/src/domain/agent.rs`**. YAGNI applies; promotion to `agentforge-core` waits until a second consumer (Platform CLI, capability registry) needs it. Decision recorded.
2. **Admin filter scope?** Resolved: **defer**. Adding the field to the admin projection without the UI filter is half-shipped (PM C4). Revision 2 holds `admin_agent_response.runtime_kind` BEHIND the UI filter; both ship together in a follow-up PR. The redesign's admin code path is unchanged in v1.

## 13. Acceptance Criteria

1. `agents.runtime_kind` is a NOT NULL column with both CHECK constraints in production, validated. Partial UNIQUE index on `runtime_id` exists.
2. All three runtime kinds are creatable via the API and visible in the UI.
3. Container lifecycle operations on Host CLI and Provider agents return 422 with the i18n-keyed messages in §7. No 5xx, no "stale container reference" misleading errors.
4. Non-owner intra-org callers receive uniform 403 (not the runtime-disclosing 422) — AppSec C8 verified by integration test.
5. Frontend code never calls `runtimeId.startsWith('host-')`. The agent domain types, specifications, **and the Zustand store** live under `src/app/entities/agent/`.
6. The frontend literal `AgentRuntimeKind` is `'container' | 'cli' | 'api'`; all references to `'container-cli' | 'host-cli' | 'provider'` are gone.
7. `POST /api/v1/agents/local-enroll` requires the `Idempotency-Key` header; a duplicate replay within 24h returns the original agent without creating a new row (AppSec C2).
8. The enrollment response carries `Cache-Control: no-store`; the access-log filter excludes the response body of `/api/v1/agents/local-enroll` from logs/Sentry breadcrumbs (AppSec C1).
9. Enrollment INSERT and the `agent.enrolled` audit event row land in the same transaction; if either fails, both roll back (AppSec C7 + DDD C5).
10. Server never trusts the inbound `AGENTFORGE_RUNTIME_KIND` env-var or any client-declared runtime kind for authorization decisions; authorization is keyed on `agents.runtime_kind` from the DB (AppSec C4).
11. `npm run fsd:check`, `npm run lint`, `npm run typecheck`, `cd rust && make ci` all pass on the integration branch.
12. The schema-contract test in §11.1 passes against both a fresh DB and a backfilled-from-legacy DB; rolling-deploy CHECK-violation scenario test passes (SRE C4).
13. The runbook at `docs/runbooks/host-cli-agent-enrollment.md` and the new `docs/runbooks/migration-062-runtime-kind.md` are committed with draft text — see §17 for the deltas.
14. `agentforge migrate doctor` subcommand exists and refuses migrations when row count > 100k without `--force` (SRE C3).

### 13.1 Honest sizing

Per PM C3, the work is **2–3 weeks** of engineering, not 1. Composition:

- Backend: 3-migration sequence + `core::RuntimeKind` extension + `NewAgent` factories + `ContainerAgent` typestate + repository refactor + audit-event INSERT + idempotency table + `migrate doctor` subcommand ≈ 1 week.
- Frontend: literal rename + FSD entity-layer move + import rewrite across 8+ files + tests + Vitest validation ≈ 4 days.
- Tests: schema-contract suite + integration suite + Vitest + Playwright ≈ 3 days.
- Docs + runbook + glossary + i18n keys ≈ 1 day.
- Review + integration + bug-fix cycles ≈ 2-4 days.

## 14. Out of Scope (future work)

> **Tracking:** every item below now has a GitHub issue, indexed in
> [`host-cli-enrollment-deferred-tracking.md`](host-cli-enrollment-deferred-tracking.md).
> Several have already shipped (manifest #454, cosign #452, dashboards #451,
> deprecation/postmortem #453, design specs #450).

Deferred to follow-up PRs, tracked per the index above:

- **Admin UI filter on `runtime_kind`** + the admin-projection field (PM C4) — ship as bundle.
- **Telemetry dashboards split by runtime kind** (SRE C7) — separate observability initiative.
- **Sum-type `AgentRuntime` with per-variant VO fields** (DDD C1) — represents Container/HostCli/Api as `enum` variants carrying their own data; ambitious refactor, separate phase.
- **Parallel typestate `EnrolledHostCli` for NATS-bound operations** (DDD C6) — applies the ContainerAgent pattern to the messaging boundary; separate hardening.
- **NATS subject namespacing by runtime kind** (Platform C7) — `events.ingest.container.<uuid>` vs `events.ingest.cli.<uuid>` and matching subject-pattern in the callout; defense-in-depth at the messaging layer. Design locked in [`docs/architecture/nats-subjects.md`](../../architecture/nats-subjects.md).
- **Hot-path serde benchmark + zero-copy `Decode`** (Architect C7) — measure `parse` cost on the agent-list endpoint and optimize if measurable.
- **Sidecar binary supply-chain hardening** (Platform C2) — Sigstore/cosign signatures, SBOM, `agentforge --verify` flag. Cross-cutting; separate program of work.
- **Migration manifest with SHA-256 checksum verification** (Platform C6) — supply-chain hardening at the DB-migration layer; cross-cutting.
- **HMAC envelope schema + replay window specification** (Platform C4) — sidecar handshake protocol is a stated non-goal of this redesign; tracked separately. Design locked in [`docs/security/hmac-envelope.md`](../../security/hmac-envelope.md).
- **Re-organizing the orchestrator's `runtime_capabilities` table** to flatten or align with the agents-side enum (cross-aggregate refactor; deliberate non-goal here).

## 15. Review Decisions (audit trail)

10-round architectural review (Distinguished Architect, Staff Rust+PG, Staff Frontend+FSD, Distinguished Engineer DDD, Staff SRE, Principal PM) + 2-team security review (Principal AppSec, Principal Platform Security) + final PM gate. Decision = ACCEPT (fixed in rev 2), DEFER (logged in §14), or REJECT (with rationale).

| #   | Reviewer / Finding                                                                                             | Decision | Where applied                                                                                                                                                                                                                                                                                                 |
| --- | -------------------------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Architect E1–E5 / Rust E1–E5 / DDD E1–E4 / SRE E1–E4 / PM E1–E4 / AppSec E1–E4 / Platform E1–E4 (endorsements) | NOTED    | Carried forward in rev 2                                                                                                                                                                                                                                                                                      |
| 2   | Architect C1: `sqlx(type_name="TEXT")` wrong for non-ENUM column                                               | ACCEPT   | §4.1 hand-rolled `sqlx::Type`/`Encode`/`Decode`                                                                                                                                                                                                                                                               |
| 3   | Architect C2: TEXT+CHECK over native ENUM TYPE undefended                                                      | ACCEPT   | §4.2 explicit paragraph                                                                                                                                                                                                                                                                                       |
| 4   | Architect C3 / SRE C3: backfill UPDATE not batched, no row-count guard                                         | ACCEPT   | §4.2 batched `DO` loop + §8.2 `migrate doctor`                                                                                                                                                                                                                                                                |
| 5   | Architect C4 / AppSec C2: idempotency hole on enrollment                                                       | ACCEPT   | §6.3 `Idempotency-Key` required + table                                                                                                                                                                                                                                                                       |
| 6   | Architect C5: backfill enshrines `host-` prefix it set out to kill                                             | ACCEPT   | §4.2 pre-flight invariant assertion                                                                                                                                                                                                                                                                           |
| 7   | Architect C6: logical-replication CHECK timing                                                                 | ACCEPT   | §8.5                                                                                                                                                                                                                                                                                                          |
| 8   | Architect C7: hot-path serde cost unbenchmarked                                                                | DEFER    | §14                                                                                                                                                                                                                                                                                                           |
| 9   | Rust C1: `core::RuntimeKind` already exists; spec would duplicate                                              | ACCEPT   | §4.1 reuses existing enum                                                                                                                                                                                                                                                                                     |
| 10  | Rust C3 / AppSec C5: `parse_legacy` accepting `cli`/`host_cli`/`host-cli` is ambiguous                         | ACCEPT   | §4.1 strict `parse`, no legacy aliases                                                                                                                                                                                                                                                                        |
| 11  | Rust C4: `AgentRuntimeKindError::Unknown` undefined, `From<>` missing                                          | ACCEPT   | §4.1 `RuntimeKindError` defined with `thiserror` + `From`                                                                                                                                                                                                                                                     |
| 12  | Rust C5: `AgentDraft` private fields vs `sqlx::FromRow` conflict                                               | ACCEPT   | §4.3 splits write-side `NewAgent` from read-side `AgentListItem`                                                                                                                                                                                                                                              |
| 13  | Rust C6: `HostCliIdentity::generate(agent_id_seed)` chicken-and-egg                                            | ACCEPT   | §4.3 `Uuid::now_v7()` client-side PK                                                                                                                                                                                                                                                                          |
| 14  | Rust C7 / AppSec C3 / Platform C5: `runtime_id` truncation + non-UNIQUE                                        | ACCEPT   | §4.3 full-UUID `runtime_id` + §4.2 migration 064 partial UNIQUE index                                                                                                                                                                                                                                         |
| 15  | Frontend C1: store-in-shared is incorrect for single-entity slice                                              | ACCEPT   | §5.3 moves `agents.store` into entities layer                                                                                                                                                                                                                                                                 |
| 16  | Frontend C2: would-be shared→entities upward import                                                            | ACCEPT   | resolved by #15                                                                                                                                                                                                                                                                                               |
| 17  | Frontend C3: existing literal `'container-cli' \| 'host-cli' \| 'provider'` mismatch                           | ACCEPT   | §3 + §5.2 + §5.3 rename to `'container' \| 'cli' \| 'api'`                                                                                                                                                                                                                                                    |
| 18  | Frontend C4 / SRE C4: rollout step 1 would 500 FE; rolling-deploy CHECK violation                              | ACCEPT   | §5.2 optional one-cycle + §4.2 / §8 split migration 062/063                                                                                                                                                                                                                                                   |
| 19  | Frontend C5: Zustand persistence (none today, but worth noting)                                                | ACCEPT   | §5.3 footnote                                                                                                                                                                                                                                                                                                 |
| 20  | Frontend C6: Vitest config picks up new path                                                                   | NOTED    | §5.3 notes "no config change needed"                                                                                                                                                                                                                                                                          |
| 21  | Frontend C7: `CreateAgentModal` call shape preserved                                                           | NOTED    | spec respects existing shape                                                                                                                                                                                                                                                                                  |
| 22  | DDD C1: `AgentRuntimeKind` as sum-type with per-variant VOs                                                    | DEFER    | §14                                                                                                                                                                                                                                                                                                           |
| 23  | DDD C2: `AgentDraft` misnamed                                                                                  | ACCEPT   | §4.3 renamed to `NewAgent`                                                                                                                                                                                                                                                                                    |
| 24  | DDD C3: `ContainerAgent` wraps projection not aggregate                                                        | ACCEPT   | §4.3 split write-side aggregate from read-side projection                                                                                                                                                                                                                                                     |
| 25  | DDD C4: glossary already uses different terms                                                                  | ACCEPT   | §3 adopts existing glossary values (`cli`, not `host_cli`)                                                                                                                                                                                                                                                    |
| 26  | DDD C5 / AppSec C7: domain event / forensic audit on enrollment                                                | ACCEPT   | §4.3.4 same-transaction `events` row                                                                                                                                                                                                                                                                          |
| 27  | DDD C6: parallel ACL typestate at NATS boundary                                                                | DEFER    | §14                                                                                                                                                                                                                                                                                                           |
| 28  | DDD C7: `find_by_runtime_kind` belongs in query service                                                        | ACCEPT   | §4.3.3 moved to `AgentQueryService`                                                                                                                                                                                                                                                                           |
| 29  | SRE C1: `SET NOT NULL` lock unbounded                                                                          | ACCEPT   | §4.2 `lock_timeout` + `statement_timeout`                                                                                                                                                                                                                                                                     |
| 30  | SRE C2: index in same migration as DDL                                                                         | ACCEPT   | §4.2 split into migration 064                                                                                                                                                                                                                                                                                 |
| 31  | SRE C3: no staged rollout / pre-flight automation                                                              | ACCEPT   | §8.2 `agentforge migrate doctor` subcommand                                                                                                                                                                                                                                                                   |
| 32  | SRE C5: VALIDATE abort playbook missing                                                                        | ACCEPT   | §8.3 runbook draft in §17                                                                                                                                                                                                                                                                                     |
| 33  | SRE C6: DR runbook breaks silently                                                                             | ACCEPT   | §8.3 + §17 runbook delta                                                                                                                                                                                                                                                                                      |
| 34  | SRE C7: no SLO/metrics/dashboards                                                                              | PARTIAL  | §10 SLO defined inline; dashboards deferred §14                                                                                                                                                                                                                                                               |
| 35  | PM C1: honest motivation framing                                                                               | ACCEPT   | §1 rewritten                                                                                                                                                                                                                                                                                                  |
| 36  | PM C2: FSD migration is scope creep, split to follow-up                                                        | REJECT   | Top-PM gate decision: keep in scope because removing prefix fallback REQUIRES the entity to own the specification; splitting forces a partial-state release that defeats the security improvement (prefix discriminator stays live one release longer). Honest sizing now reflects the 2–3 week cost (§13.1). |
| 37  | PM C3: honest sizing 2–3 weeks                                                                                 | ACCEPT   | §13.1                                                                                                                                                                                                                                                                                                         |
| 38  | PM C4: half-shipped admin field without filter                                                                 | ACCEPT   | §12 resolved: defer both bundle to follow-up                                                                                                                                                                                                                                                                  |
| 39  | PM C5: resolve open questions                                                                                  | ACCEPT   | §12 resolved inline                                                                                                                                                                                                                                                                                           |
| 40  | PM C6: error messages too long for toasts + no i18n                                                            | ACCEPT   | §7 i18n keys + short toast + long detail                                                                                                                                                                                                                                                                      |
| 41  | PM C7: rollback story hand-wavy                                                                                | ACCEPT   | §8.4 explicit                                                                                                                                                                                                                                                                                                 |
| 42  | PM C8: doc draft text missing                                                                                  | ACCEPT   | §17 includes runbook + glossary deltas                                                                                                                                                                                                                                                                        |
| 43  | AppSec C1: plaintext credential return + zero CSRF/logging story (CRITICAL)                                    | ACCEPT   | §6.3 `Cache-Control: no-store`, access-log filter, CSRF header requirement                                                                                                                                                                                                                                    |
| 44  | AppSec C4: `AGENTFORGE_RUNTIME_KIND` unsigned advisory                                                         | ACCEPT   | §6.3 advisory only; §16 server-derive enforcement                                                                                                                                                                                                                                                             |
| 45  | AppSec C6: service-layer bypass blast radius                                                                   | ACCEPT   | §4.3.3 `&TenantScope` on every repository method                                                                                                                                                                                                                                                              |
| 46  | AppSec C8: error enumeration via runtime-kind disclosure                                                       | ACCEPT   | §6.4 + §7 uniform 403 for non-owner intra-org                                                                                                                                                                                                                                                                 |
| 47  | Platform C1: trust boundary asymmetric, Host CLI is adversary territory                                        | ACCEPT   | §16 threat model section                                                                                                                                                                                                                                                                                      |
| 48  | Platform C2: sidecar binary distribution has zero supply-chain controls                                        | DEFER    | §14 (separate hardening initiative)                                                                                                                                                                                                                                                                           |
| 49  | Platform C3: `nats_base_url` returned plaintext, no TLS guidance                                               | ACCEPT   | §6.3 reject non-`tls://` without org policy; §17 runbook delta                                                                                                                                                                                                                                                |
| 50  | Platform C4: HMAC replay window unspecified                                                                    | DEFER    | §14 (sidecar handshake unchanged per non-goal)                                                                                                                                                                                                                                                                |
| 51  | Platform C6: migration runner has no checksum verification                                                     | DEFER    | §14 (cross-cutting hardening initiative)                                                                                                                                                                                                                                                                      |
| 52  | Platform C7: NATS subjects don't embed runtime_kind                                                            | DEFER    | §14                                                                                                                                                                                                                                                                                                           |

**Final PM verdict (revision 2):** **Approve with the amendments above incorporated.** All ACCEPT items are reflected inline. All DEFER items are explicitly logged in §14 with rationale; none are silent.

## 16. Threat Model — Host CLI Trust Boundary

The Host CLI runtime is the only runtime where the sidecar lives **outside** the platform's TCB. This section makes the trust posture explicit (Platform C1).

### 16.1 Asset inventory at the Host CLI boundary

- Per-agent **HMAC secret** (signs result envelopes; the only thing standing between a compromised operator machine and arbitrary result forgery).
- Per-agent **NATS connect password** (grants pub/sub on the agent's scoped subjects).
- The agent's **task payloads** the sidecar receives (may contain workspace context: code, prompts, evidence).
- The agent's **result evidence** the sidecar publishes back (may contain output that other tenants would benefit from seeing).

### 16.2 Threat actors

1. **Compromised operator machine** (malware, family device, lost laptop). Holds all four assets above.
2. **Network MITM** between operator machine and the platform's NATS endpoint (coffee shop Wi-Fi, hostile ISP).
3. **Replay of an in-flight enrollment** (Architect C4 / AppSec C2).
4. **Insider operator** with legitimate platform access who enrolls a Host CLI agent in a workspace they should not access for exfiltration.
5. **Stolen JWT session** that lets an attacker enumerate runtime kinds across the org for target prioritization (AppSec C8).

### 16.3 Mitigations in rev 2

| Threat | Mitigation in rev 2                                                                                                                                                                                                                                         | Where        |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| 1      | Compromised operator machine reveals one agent's creds — but **callout authorization is keyed on `agents.runtime_kind` in DB**, not on the env var or any client-declared value. A compromised host_cli sidecar cannot escalate to container-only subjects. | §10 + §16.4  |
| 2      | Enrollment service **rejects non-`tls://` NATS URLs** for host_cli unless `allow_plaintext_host_nats` org policy is set explicitly. Runbook requires TLS cert pin or CA bundle path.                                                                        | §6.3, §17    |
| 3      | `Idempotency-Key` header is required; duplicate replays return the original response, not a second credential set. 24h TTL, scoped to `(org_id, user_id, key)`.                                                                                             | §6.3 + table |
| 4      | Forensic audit event (`event_type='agent.enrolled'`) lands in the SAME transaction as the agent INSERT and includes `actor_user_id`, `source_ip`, `user_agent`. Audit-driven detection can spot unusual enrollments.                                        | §4.3.4 + §10 |
| 5      | Non-owner intra-org callers on lifecycle endpoints get uniform 403, not runtime-kind-disclosing 422. Enumeration vector closed.                                                                                                                             | §6.4 + §7    |

### 16.4 Server-derived authorization (the load-bearing rule)

For every server-side decision that branches on runtime kind — choosing a code path, choosing a NATS subject, applying a policy, accepting a hook — the value MUST come from `SELECT runtime_kind FROM agents WHERE id = :agent_id AND org_id = :tenant`, NOT from:

- the inbound `AGENTFORGE_RUNTIME_KIND` env var (sidecar can lie),
- the inbound JWT claims (token claims are tenant-scoped but not runtime-typed),
- the inbound HMAC envelope (could be forged before the new envelope schema is specified — see Platform C4),
- any client-supplied request body.

The env-var is **advisory metadata** the operator uses to debug their own enrollment script; the platform never reads it back from a request boundary.

### 16.5 Residual risks (deferred — see §14)

- **Sidecar binary supply chain.** A trojaned `agentforge-sidecar` binary still gives an attacker full agent privilege. Mitigated only by future Sigstore/SBOM work (Platform C2).
- **NATS subject namespacing.** Today, host_cli and container subjects share the same prefix; defense at the messaging layer requires future subject-pattern changes (Platform C7).
- **HMAC envelope replay window.** Today, the envelope's replay-protection contract is implicit. Future work specifies nonce + monotonic timestamp + 5-minute window (Platform C4).
- **Migration supply chain.** Today, the migration runner does not verify checksums against a committed manifest. Future work adds `MANIFEST.sha256` (Platform C6).

These are real risks. Revision 2 ships the foundation that makes them tractable to fix; the fixes themselves are separate, sized, and tracked.

## 17. Documentation deltas (draft text)

### 17.1 `docs/architecture/glossary.md` (no rename, but cross-link to this spec)

Add to the "Runtime modes (Settings page)" table footnote:

> The DB column `agents.runtime_kind` and the Rust enum `agentforge_core::RuntimeKind` use the values in the "DB value" column above. See `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` for the discriminator design.

### 17.2 `docs/runbooks/host-cli-agent-enrollment.md` — add three sections

**Verify (new step 6, before "If the agent stays offline…")**

```text
6. (Optional) Confirm the platform recorded the enrollment as Host CLI:
   - Web UI: the agent's detail page shows the "Host CLI" badge.
   - DB: `SELECT runtime_kind FROM agents WHERE id = '<agent-id>';` returns 'cli'.
   - Audit: `SELECT * FROM events WHERE agent_id = '<agent-id>' AND event_type = 'agent.enrolled';`
     should return one row with your user_id and source IP.
```

**TLS (new "Network" section before "Revoke")**

```text
## Network

Host CLI enrollment requires `NATS_AGENT_URL` to use TLS (`tls://`) by default.
If your deployment runs NATS without TLS (lab/sandbox only), an organization
admin must set the policy flag `allow_plaintext_host_nats = true` before
enrollment will succeed. Production deployments should never set this flag.
```

**Idempotency (under "Enroll")**

```text
The `agentforge agents enroll-local` command generates an `Idempotency-Key`
header automatically. If you re-run the same command within 24 hours, the
platform returns the same agent rather than creating a duplicate. To force a
fresh enrollment, pass `--new-key` or wait for the 24-hour window to expire.
```

### 17.3 `docs/runbooks/migration-062-runtime-kind.md` (new)

```text
# Migration 062 / 063 / 064 — `agents.runtime_kind` discriminator

## Before you migrate

Run `agentforge migrate doctor` first. It will:
- count agent rows and refuse without `--force` if > 100,000
- report any agent row that would fail the post-062 invariant CHECK
- estimate lock duration
- verify replication subscribers are reachable

## During migration

The three migrations land in order:
1. 062 — column add + backfill (no CHECK yet). Old + new application instances coexist.
2. (Operator step) Confirm every API instance is on the new release.
3. 063 — CHECK constraints. From this point, INSERTs without `runtime_kind`
   that should be `'container'` (have `cli_tool`) will be rejected.
4. 064 — indexes. Schedule in a maintenance window.

## If 062's pre-flight assertion fails

The migration aborts with a row count of agents that would later violate
the invariant. To find the offenders:

    SELECT id, runtime_kind, cli_tool, container_id, runtime_id
    FROM agents
    WHERE NOT (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL)
    );

Fix each row manually (most commonly: an api row that has cli_tool from a
pre-018-rename era — clear cli_tool and set runtime_kind='api'). Then re-run.

## Restore from a pre-062 backup into a post-062 schema

1. `ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_invariants;`
2. `ALTER TABLE agents DROP CONSTRAINT agents_runtime_kind_check;`
3. Restore.
4. Re-run the backfill UPDATE from 062 against the restored rows.
5. Re-add both constraints with `NOT VALID`, then `VALIDATE`.

## Logical replication subscribers

`VALIDATE CONSTRAINT` on the primary does NOT validate the same CHECK on
logical-replication subscribers. Each subscriber's DBA must run:

    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;
    ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;

directly against the subscriber after 063 ships.
```
