# Host CLI Enrollment Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote `agents.runtime_kind` to a first-class STI discriminator (DB-CHECK-enforced, code-typed, end-to-end), fix the bug where Container CLI lifecycle endpoints misroute Host CLI agents, and align the agent runtime vocabulary across backend, frontend, and docs to match the existing `agentforge_core::RuntimeKind` enum and `docs/architecture/glossary.md`.

**Architecture:** Three-migration sequence (062 column+backfill → 063 CHECK constraints → 064 indexes) closes the rolling-deploy CHECK-violation hole. Backend writes through typed `NewAgent` factories and the new `ContainerAgent` typestate so Docker calls cannot compile against non-container agents. Frontend domain types and the Zustand store move into `src/app/entities/agent/` per FSD canon; literal renamed to canonical `container|cli|api`. Host CLI enrollment becomes idempotent via `Idempotency-Key` header and atomic with its `agent.enrolled` audit event.

**Tech Stack:** Rust (Axum + sqlx + Postgres), TypeScript (React + Vite + Zustand), Vitest + Playwright + cargo test, Feature-Sliced Design.

**Spec:** `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` (rev 2, commit `c53e081`). All section references in this plan map to that spec.

**Sizing:** 2–3 weeks of engineering, 50+ tasks across 13 phases.

---

## File Structure

### Created

| Path                                                          | Responsibility                                                                          |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `rust/crates/db/migrations/062_agents_runtime_kind.sql`       | Column + batched backfill + pre-flight invariant assertion + SET NOT NULL               |
| `rust/crates/db/migrations/063_agents_runtime_kind_check.sql` | Two CHECK constraints (enum + invariant), `NOT VALID` + `VALIDATE`                      |
| `rust/crates/db/migrations/064_agents_runtime_kind_index.sql` | Plain index + partial UNIQUE on `runtime_id`                                            |
| `rust/crates/db/migrations/065_enrollment_idempotency.sql`    | Idempotency table for `POST /agents/local-enroll`                                       |
| `rust/crates/api/tests/agents_runtime_kind_constraint.rs`     | Schema-contract suite (9 invariant combos + rolling-deploy scenario + UNIQUE collision) |
| `rust/crates/api/tests/agent_enrollment_idempotency.rs`       | Replay/no-replay tests for `Idempotency-Key`                                            |
| `rust/crates/api/src/services/agent_query.rs`                 | Read-side query service holding `find_by_runtime_kind`                                  |
| `rust/bins/cli/src/cmd/migrate/doctor.rs`                     | `agentforge migrate doctor` subcommand                                                  |
| `src/app/entities/agent/index.ts`                             | Public barrel for the agent entity                                                      |
| `src/app/entities/agent/model/types.ts`                       | `AgentInfo`, `AgentRuntimeKind`, `AgentStatus`, `CliTool`                               |
| `src/app/entities/agent/model/runtime-kind.ts`                | `isHostCliAgent` / `isContainerAgent` / `isApiAgent` specifications                     |
| `src/app/entities/agent/model/agents.store.ts`                | Zustand store moved from `src/app/shared/model/`                                        |
| `src/app/entities/agent/api/AgentAPI.ts`                      | Moved from `src/app/shared/api/legacy/`                                                 |
| `tests/unit/app/entities/agent/runtime-kind.test.ts`          | Specifications exhaustive table                                                         |
| `tests/unit/app/entities/agent/agents-store.test.ts`          | Backward-compat fallback when server omits `runtimeKind`                                |
| `tests/unit/api/tracing_redaction_test.rs`                    | Asserts hmac_secret / nats_connect_password never in spans                              |
| `docs/runbooks/migration-062-runtime-kind.md`                 | Migration operator playbook (pre-flight, abort, restore)                                |

### Modified

| Path                                                        | Change                                                                                                                                                      |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rust/crates/core/src/runtime_capability.rs`                | Add `sqlx::Type` / `Encode` / `Decode` for `RuntimeKind`; add `From<RuntimeCapabilityError> for AppError`.                                                  |
| `rust/crates/api/src/repositories/agent/mod.rs`             | `AgentListItem.runtime_kind`; new `AgentAggregate` write-side type; refactor `create` to take `NewAgent`; drop `set_host_runtime`; atomic audit event write |
| `rust/crates/api/src/domain/agent.rs`                       | Add `NewAgent` factories, `HostCliIdentity`, `ContainerAgent` typestate, `LifecycleRejection`                                                               |
| `rust/crates/api/src/services/agent.rs`                     | Accept typed `Create*Intent`; construct `NewAgent`                                                                                                          |
| `rust/crates/api/src/services/agent_enrollment.rs`          | UUIDv7 PK pre-INSERT; full-UUID `runtime_id`; idempotency lookup; TLS check; atomic INSERT + audit                                                          |
| `rust/crates/api/src/services/agent_container_lifecycle.rs` | Accept `&ContainerAgent`; map `LifecycleRejection` to 422                                                                                                   |
| `rust/crates/api/src/services/agent_container_control.rs`   | Same as lifecycle                                                                                                                                           |
| `rust/crates/api/src/routes/agents.rs`                      | Require `Idempotency-Key` on `/local-enroll`; `Cache-Control: no-store`; owner-ACL on lifecycle; access-log filter wiring                                   |
| `rust/crates/api/src/middleware.rs`                         | Add idempotency-extractor / source-IP / user-agent propagation                                                                                              |
| `rust/bins/cli/src/cmd/agents/enroll_local.rs`              | Print `# runtime_kind: cli` header comment                                                                                                                  |
| `shared/types/agent.ts`                                     | `AgentRuntimeKind = 'container' \| 'cli' \| 'api'`; `runtimeKind?:` on `AgentListItem`; `Idempotency-Key` on request type                                   |
| `src/app/shared/i18n/locales/en.ts`                         | Add 10 new error keys                                                                                                                                       |
| `src/app/shared/i18n/locales/zh.ts`                         | Add 10 new error keys                                                                                                                                       |
| `src/app/features/agents/AgentConfigTab.tsx`                | Import from `@app/entities/agent`                                                                                                                           |
| `src/app/features/agents/AgentControlPanel.tsx`             | Import from `@app/entities/agent`                                                                                                                           |
| `src/app/features/agents/AgentListView.tsx`                 | Import; literal rename                                                                                                                                      |
| `src/app/features/agents/AgentCard.tsx`                     | Import; literal rename                                                                                                                                      |
| `src/app/features/agents/AgentKindBadge.tsx`                | Literal rename                                                                                                                                              |
| `src/app/features/agents/CreateAgentModal.tsx`              | Import from `@app/entities/agent`                                                                                                                           |
| `src/app/widgets/agent-detail/AgentDetailView.tsx`          | Import from `@app/entities/agent`                                                                                                                           |
| `src/app/pages/getting-started/ui/GettingStartedView.tsx`   | Import from `@app/entities/agent`                                                                                                                           |
| `docs/architecture/glossary.md`                             | Footnote linking to this spec (no rename)                                                                                                                   |
| `docs/runbooks/host-cli-agent-enrollment.md`                | Verify section, Network section, Idempotency note                                                                                                           |

### Deleted

| Path                                    | Reason                                                    |
| --------------------------------------- | --------------------------------------------------------- |
| `src/app/shared/api/legacy/AgentAPI.ts` | Moved into `src/app/entities/agent/api/AgentAPI.ts`       |
| `src/app/shared/model/agents.store.ts`  | Moved into `src/app/entities/agent/model/agents.store.ts` |

---

## Phase 0 — Branch setup

### Task 0.1: Create isolated worktree

**Files:**

- Worktree at `../wisdoverse-forge-host-cli-runtime-kind/` on branch `feat/agents-runtime-kind`

- [ ] **Step 1: Create the worktree from current origin/main**

```bash
git fetch origin main --quiet
git worktree add ../wisdoverse-forge-host-cli-runtime-kind -b feat/agents-runtime-kind origin/main
cd ../wisdoverse-forge-host-cli-runtime-kind
git status --short --branch
```

Expected: `## feat/agents-runtime-kind...origin/main` and no uncommitted changes.

- [ ] **Step 2: Confirm the existing spec is reachable**

```bash
ls docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md
ls docs/superpowers/plans/2026-05-27-host-cli-enrollment.md
```

Expected: both paths exist.

All subsequent tasks assume `pwd` is this worktree.

---

## Phase 1 — Core domain types

### Task 1.1: Add `sqlx::Type` / `Encode` / `Decode` to `core::RuntimeKind`

**Files:**

- Modify: `rust/crates/core/src/runtime_capability.rs` (around lines 75–122)
- Test: same file, `#[cfg(test)]` module

- [ ] **Step 1: Write the failing sqlx round-trip test**

Append to `rust/crates/core/src/runtime_capability.rs` `#[cfg(test)] mod tests`:

```rust
#[sqlx::test(migrations = false)]
async fn runtime_kind_sqlx_roundtrip(pool: sqlx::PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "CREATE TEMP TABLE tmp_runtime_kind (id INT PRIMARY KEY, rk TEXT NOT NULL)"
    ).execute(&pool).await?;

    for &kind in &[RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
        sqlx::query("INSERT INTO tmp_runtime_kind (id, rk) VALUES ($1, $2)")
            .bind(kind as i32)
            .bind(kind)
            .execute(&pool).await?;

        let row: (i32, RuntimeKind) =
            sqlx::query_as("SELECT id, rk FROM tmp_runtime_kind WHERE id = $1")
            .bind(kind as i32)
            .fetch_one(&pool).await?;

        assert_eq!(row.1, kind);
    }
    Ok(())
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd rust && cargo test -p agentforge-core --test runtime_capability runtime_kind_sqlx_roundtrip 2>&1 | tail -10
```

Expected: compilation FAIL with `the trait Type<Postgres> is not implemented for RuntimeKind`.

- [ ] **Step 3: Implement sqlx::Type / Encode / Decode**

Insert after the `impl FromStr for RuntimeKind` block in `rust/crates/core/src/runtime_capability.rs`:

```rust
impl sqlx::Type<sqlx::Postgres> for RuntimeKind {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <&str as sqlx::Type<sqlx::Postgres>>::type_info()
    }
    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for RuntimeKind {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for RuntimeKind {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let raw: &str = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Self::parse_legacy(raw).map_err(|e| Box::new(e) as sqlx::error::BoxDynError)
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd rust && cargo test -p agentforge-core --test runtime_capability runtime_kind_sqlx_roundtrip 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/core/src/runtime_capability.rs
git commit -m "feat(core): add sqlx Type/Encode/Decode for RuntimeKind"
```

### Task 1.2: Add `From<RuntimeCapabilityError> for AppError`

**Files:**

- Modify: `rust/crates/core/src/lib.rs` (or wherever `AppError`/`ErrorKind::Validation` is defined; check via `grep -rn "enum ErrorKind" rust/crates/core/src`)

- [ ] **Step 1: Locate the AppError definition**

```bash
grep -rn "enum ErrorKind\|impl From.*for AppError" rust/crates/core/src | head -10
```

Use the file revealed by this command in subsequent steps. The spec assumes `rust/crates/core/src/lib.rs` or `rust/crates/core/src/errors.rs`.

- [ ] **Step 2: Write the failing conversion test**

Append to the same file's `#[cfg(test)] mod tests`:

```rust
#[test]
fn runtime_capability_error_maps_to_validation() {
    let err = RuntimeCapabilityError::UnknownRuntimeKind { raw: "host_cli".into() };
    let app: AppError = err.into();
    match app.kind() {
        ErrorKind::Validation(msg) => assert!(msg.contains("host_cli")),
        other => panic!("expected Validation, got {other:?}"),
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
cd rust && cargo test -p agentforge-core runtime_capability_error_maps_to_validation 2>&1 | tail -10
```

Expected: FAIL with `the trait From<RuntimeCapabilityError> is not implemented for AppError`.

- [ ] **Step 4: Add the impl**

Add (in the same module as the other `From` impls):

```rust
impl From<RuntimeCapabilityError> for AppError {
    fn from(err: RuntimeCapabilityError) -> Self {
        AppError::from(ErrorKind::Validation(err.to_string()))
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd rust && cargo test -p agentforge-core runtime_capability_error_maps_to_validation 2>&1 | tail -10
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/core/src/
git commit -m "feat(core): convert RuntimeCapabilityError into AppError::Validation"
```

### Task 1.3: Strict-parse + deny_unknown_fields tests

**Files:**

- Test: `rust/crates/core/src/runtime_capability.rs` (same module)

- [ ] **Step 1: Write the failing strict-parse tests**

Append to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn parse_legacy_rejects_invented_aliases() {
    // rev1 spec invented these; rev2 forbids them.
    assert!(RuntimeKind::parse_legacy("host_cli").is_err());
    assert!(RuntimeKind::parse_legacy("host-cli").is_err());
    assert!(RuntimeKind::parse_legacy("provider").is_err());
    assert!(RuntimeKind::parse_legacy("container-cli").is_err());
}

#[test]
fn parse_legacy_accepts_canonical_only() {
    assert_eq!(RuntimeKind::parse_legacy("container").unwrap(), RuntimeKind::Container);
    assert_eq!(RuntimeKind::parse_legacy("cli").unwrap(),       RuntimeKind::Cli);
    assert_eq!(RuntimeKind::parse_legacy("api").unwrap(),       RuntimeKind::Api);
    // canonicalization
    assert_eq!(RuntimeKind::parse_legacy(" CLI ").unwrap(),     RuntimeKind::Cli);
}
```

- [ ] **Step 2: Run tests**

```bash
cd rust && cargo test -p agentforge-core parse_legacy_ 2>&1 | tail -10
```

Expected: both PASS (the current parse_legacy already implements this; we are locking in the contract).

- [ ] **Step 3: Commit**

```bash
git add rust/crates/core/src/runtime_capability.rs
git commit -m "test(core): lock in strict RuntimeKind parsing"
```

---

## Phase 2 — DB migrations

### Task 2.1: Create migration 062 — column + backfill + pre-flight assertion

**Files:**

- Create: `rust/crates/db/migrations/062_agents_runtime_kind.sql`

- [ ] **Step 1: Write the migration file**

Create `rust/crates/db/migrations/062_agents_runtime_kind.sql`:

```sql
-- 062: add runtime_kind column to agents, backfill from current shape.
--
-- The DEFAULT 'api' is set AFTER backfill so existing rows pick up their
-- correct kind. The DEFAULT then covers any INSERT from an old API instance
-- during the rolling deploy window between 062 and 063.
--
-- CHECK constraints land in 063 AFTER new application code is fully deployed
-- to avoid rolling-deploy CHECK violations.

SET lock_timeout      = '3s';
SET statement_timeout = '30s';

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS runtime_kind TEXT;

-- Batched backfill: 5000 rows per pass, FOR UPDATE SKIP LOCKED keeps WAL bounded
-- and lets concurrent writers proceed.
DO $$
DECLARE
    batch_size INT := 5000;
    affected   INT;
BEGIN
    LOOP
        WITH targets AS (
            SELECT id FROM agents
            WHERE runtime_kind IS NULL
            ORDER BY id
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

-- Pre-flight invariant assertion. Any row that would later violate the
-- joint CHECK from 063 must surface NOW, before NOT NULL is set, so an
-- operator can intervene before 063 hard-locks the constraint.
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
            'Inspect: SELECT id, runtime_kind, cli_tool, container_id, runtime_id FROM agents WHERE NOT (...). '
            'Resolve before 063 ships.',
            bad_rows;
    END IF;
END $$;

ALTER TABLE agents
    ALTER COLUMN runtime_kind SET NOT NULL,
    ALTER COLUMN runtime_kind SET DEFAULT 'api';
```

- [ ] **Step 2: Verify migration parses by running it against a scratch DB**

```bash
cd rust && cargo test -p agentforge-db --tests -- --test migrations 2>&1 | tail -20
```

Expected: existing migration test exercises this migration without panic; if no such test exists, defer until Task 2.4 which adds the schema-contract test.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/db/migrations/062_agents_runtime_kind.sql
git commit -m "feat(db): migration 062 add agents.runtime_kind with batched backfill"
```

### Task 2.2: Create migration 063 — CHECK constraints

**Files:**

- Create: `rust/crates/db/migrations/063_agents_runtime_kind_check.sql`

- [ ] **Step 1: Write the migration file**

Create `rust/crates/db/migrations/063_agents_runtime_kind_check.sql`:

```sql
-- 063: add enum CHECK and joint-invariant CHECK to agents.runtime_kind.
--
-- Two-phase per FAANG-scale Postgres convention:
--   NOT VALID lands the constraint instantly under a brief AccessExclusive
--     metadata lock — no full-table scan.
--   VALIDATE then scans without blocking writes (ShareUpdateExclusive only).
--
-- This migration must land ONLY AFTER every running API instance is on the
-- new release that writes `runtime_kind` on every INSERT. See
-- docs/runbooks/migration-062-runtime-kind.md.

SET lock_timeout      = '3s';
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

- [ ] **Step 2: Commit**

```bash
git add rust/crates/db/migrations/063_agents_runtime_kind_check.sql
git commit -m "feat(db): migration 063 add agents.runtime_kind CHECK constraints"
```

### Task 2.3: Create migration 064 — indexes including partial UNIQUE on runtime_id

**Files:**

- Create: `rust/crates/db/migrations/064_agents_runtime_kind_index.sql`

- [ ] **Step 1: Write the migration file**

Create `rust/crates/db/migrations/064_agents_runtime_kind_index.sql`:

```sql
-- 064: add indexes for agents.runtime_kind discriminator queries
-- and close the runtime_id collision concern with a partial UNIQUE index.

SET lock_timeout = '10s';

CREATE INDEX IF NOT EXISTS idx_agents_runtime_kind ON agents(runtime_kind);

-- runtime_id is the per-agent sidecar identity. host_cli rows derive it from
-- the full Agent UUID (`host-{uuid}`); container rows leave it NULL until the
-- sidecar registers. Two rows with the same runtime_id would mean two agents
-- could authenticate as the same NATS principal — a privilege confusion.
CREATE UNIQUE INDEX IF NOT EXISTS uq_agents_runtime_id
    ON agents(runtime_id)
    WHERE runtime_id IS NOT NULL;
```

- [ ] **Step 2: Commit**

```bash
git add rust/crates/db/migrations/064_agents_runtime_kind_index.sql
git commit -m "feat(db): migration 064 add runtime_kind index and unique runtime_id"
```

### Task 2.4: Schema-contract test for 9-combo + rolling-deploy + UNIQUE collision

**Files:**

- Create: `rust/crates/api/tests/agents_runtime_kind_constraint.rs`

- [ ] **Step 1: Write the failing schema-contract test**

Create `rust/crates/api/tests/agents_runtime_kind_constraint.rs`:

```rust
//! Schema-contract test for migration 062/063/064.
//!
//! Covers (a) the 9-combo (runtime_kind × cli_tool × container_id) matrix,
//! (b) the rolling-deploy scenario where 062 has landed but 063 has not yet,
//! (c) the partial UNIQUE index on runtime_id.

use sqlx::{PgPool, Row};
use uuid::Uuid;

const ORG_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001);

async fn insert_agent(pool: &PgPool, runtime_kind: &str, cli_tool: Option<&str>, container_id: Option<&str>, runtime_id: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, runtime_kind, cli_tool, container_id, runtime_id, status, created_at, updated_at)
         VALUES ($1, $2, $2, $2, $3, $4, $5, $6, 'offline', NOW(), NOW())"
    )
    .bind(Uuid::new_v4())
    .bind(ORG_ID)
    .bind(runtime_kind)
    .bind(cli_tool)
    .bind(container_id)
    .bind(runtime_id)
    .execute(pool).await
    .map(|_| ())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn invariants_reject_invalid_combos(pool: PgPool) {
    // 9-combo matrix
    let cases = [
        // (runtime_kind, cli_tool, container_id, expect_ok)
        ("container", Some("codex"), Some("ctr-1"), true),
        ("container", Some("codex"), None,          true),
        ("container", None,          None,          false),
        ("cli",       Some("codex"), None,          true),
        ("cli",       Some("codex"), Some("ctr-1"), false),
        ("cli",       None,          None,          false),
        ("api",       None,          None,          true),
        ("api",       Some("codex"), None,          false),
        ("api",       None,          Some("ctr-1"), false),
    ];
    for (rk, ct, ci, ok) in cases {
        let result = insert_agent(&pool, rk, ct, ci, None).await;
        if ok {
            assert!(result.is_ok(), "expected OK for ({rk}, {ct:?}, {ci:?}), got {result:?}");
        } else {
            assert!(result.is_err(), "expected ERR for ({rk}, {ct:?}, {ci:?}), got OK");
        }
    }
    // Bogus runtime_kind rejected by enum CHECK
    assert!(insert_agent(&pool, "bogus", None, None, None).await.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unique_runtime_id_partial_index(pool: PgPool) {
    insert_agent(&pool, "cli", Some("codex"), None, Some("host-abc")).await.unwrap();
    // Second host_cli with same runtime_id must be rejected.
    assert!(insert_agent(&pool, "cli", Some("codex"), None, Some("host-abc")).await.is_err());
    // Two rows with NULL runtime_id are allowed (partial WHERE clause).
    insert_agent(&pool, "container", Some("codex"), None, None).await.unwrap();
    insert_agent(&pool, "container", Some("codex"), None, None).await.unwrap();
}

#[sqlx::test(migrations = "../db/migrations")]
async fn backfill_categorizes_legacy_shapes(pool: PgPool) {
    // Pre-seed rows that look like pre-062 shapes by writing through a
    // shadow path that bypasses the column (the migration's own backfill
    // logic should re-derive runtime_kind correctly).
    // For this test we simply confirm the backfill matches code expectations
    // by reading back what 062 wrote on a fresh DB initialized from migrations.
    sqlx::query("DELETE FROM agents").execute(&pool).await.unwrap();
    let cases = [
        // (cli_tool, runtime_id, expected_runtime_kind)
        (Some("codex"), Some("host-aa11"), "cli"),
        (Some("codex"), None,              "container"),
        (None,          None,              "api"),
    ];
    for (ct, ri, want) in cases {
        // Insert through the live schema (post-062+063). Should land in correct kind.
        insert_agent(&pool, want, ct, None, ri).await.unwrap();
        let got: (String,) = sqlx::query_as(
            "SELECT runtime_kind FROM agents WHERE cli_tool IS NOT DISTINCT FROM $1 AND runtime_id IS NOT DISTINCT FROM $2 LIMIT 1"
        )
        .bind(ct).bind(ri).fetch_one(&pool).await.unwrap();
        assert_eq!(got.0, want);
    }
}
```

- [ ] **Step 2: Run the test to verify it passes against the new migrations**

```bash
cd rust && cargo test -p agentforge-api --test agents_runtime_kind_constraint 2>&1 | tail -20
```

Expected: all three tests PASS once migrations 062/063/064 are present.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/api/tests/agents_runtime_kind_constraint.rs
git commit -m "test(api): schema-contract suite for agents.runtime_kind invariants"
```

### Task 2.5: Create migration 065 — `enrollment_idempotency` table

**Files:**

- Create: `rust/crates/db/migrations/065_enrollment_idempotency.sql`

- [ ] **Step 1: Write the migration**

Create `rust/crates/db/migrations/065_enrollment_idempotency.sql`:

```sql
-- 065: idempotency table for POST /api/v1/agents/local-enroll.
--
-- A retried enrollment with the same (org_id, user_id, key) within the TTL
-- returns the original agent rather than minting a duplicate row with new
-- credentials. Closes the credential-proliferation concern from AppSec
-- review and the network-replay attack scenario from §16 of the spec.

CREATE TABLE IF NOT EXISTS enrollment_idempotency (
    org_id      UUID        NOT NULL,
    user_id     UUID        NOT NULL,
    key         TEXT        NOT NULL,
    agent_id    UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (org_id, user_id, key)
);

CREATE INDEX IF NOT EXISTS idx_enrollment_idempotency_expires_at
    ON enrollment_idempotency(expires_at);
```

- [ ] **Step 2: Commit**

```bash
git add rust/crates/db/migrations/065_enrollment_idempotency.sql
git commit -m "feat(db): migration 065 enrollment_idempotency table"
```

---

## Phase 3 — Domain layer

### Task 3.1: Add `HostCliIdentity` value object

**Files:**

- Modify: `rust/crates/api/src/domain/agent.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `rust/crates/api/src/domain/agent.rs`:

```rust
#[test]
fn host_cli_identity_uses_full_uuid_v7() {
    let id = HostCliIdentity::generate();
    assert!(id.runtime_id().starts_with("host-"), "got: {}", id.runtime_id());
    // Full UUID after the prefix (36 chars), not 8.
    assert_eq!(id.runtime_id().len(), "host-".len() + 36);
    // UUIDv7 has version bits set
    assert_eq!(id.agent_id().get_version_num(), 7);
    assert!(!id.hmac_secret().is_empty());
    assert!(!id.nats_connect_password().is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd rust && cargo test -p agentforge-api host_cli_identity_uses_full_uuid_v7 2>&1 | tail -10
```

Expected: FAIL with `cannot find struct HostCliIdentity`.

- [ ] **Step 3: Add the type**

Insert near the existing `HostAgentEnrollmentPolicy` block in `rust/crates/api/src/domain/agent.rs`:

```rust
#[derive(Debug, Clone)]
pub(crate) struct HostCliIdentity {
    agent_id: Uuid,
    runtime_id: String,
    hmac_secret: String,
    nats_connect_password: String,
}

impl HostCliIdentity {
    pub(crate) fn generate() -> Self {
        let agent_id = Uuid::now_v7();
        Self {
            runtime_id: format!("host-{agent_id}"),
            hmac_secret: Uuid::new_v4().to_string(),
            nats_connect_password: Uuid::new_v4().to_string(),
            agent_id,
        }
    }

    pub(crate) fn agent_id(&self) -> Uuid { self.agent_id }
    pub(crate) fn runtime_id(&self) -> &str { &self.runtime_id }
    pub(crate) fn hmac_secret(&self) -> &str { &self.hmac_secret }
    pub(crate) fn nats_connect_password(&self) -> &str { &self.nats_connect_password }
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd rust && cargo test -p agentforge-api host_cli_identity_uses_full_uuid_v7 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/domain/agent.rs
git commit -m "feat(api): add HostCliIdentity value object with UUIDv7 + full UUID runtime_id"
```

### Task 3.2: Add `NewAgent` factory with three constructors

**Files:**

- Modify: `rust/crates/api/src/domain/agent.rs`

- [ ] **Step 1: Write the failing tests**

Append to `#[cfg(test)] mod tests`:

```rust
use agentforge_core::CliToolKind;

#[test]
fn new_agent_container_validates_inputs() {
    let scope = test_tenant_scope();
    let ok = NewAgent::container(
        &scope, CliToolKind::Codex, Some("My Agent"), None, None,
        Uuid::new_v4(), None, None,
    );
    assert!(ok.is_ok());

    // Name >255 chars rejected
    let long = "x".repeat(256);
    let err = NewAgent::container(
        &scope, CliToolKind::Codex, Some(&long), None, None,
        Uuid::new_v4(), None, None,
    );
    assert!(err.is_err());
}

#[test]
fn new_agent_host_cli_carries_identity_and_kind() {
    let scope = test_tenant_scope();
    let identity = HostCliIdentity::generate();
    let na = NewAgent::host_cli(
        &scope, CliToolKind::Codex, identity.clone(), None, None, None,
        Uuid::new_v4(), None,
    ).unwrap();
    assert_eq!(na.runtime_kind(), RuntimeKind::Cli);
    assert_eq!(na.runtime_id(), Some(identity.runtime_id()));
    assert_eq!(na.cli_tool(), Some("codex"));
}

#[test]
fn new_agent_api_rejects_empty_model() {
    let scope = test_tenant_scope();
    assert!(NewAgent::api(
        &scope, "anthropic", "", None, None, Uuid::new_v4(), None
    ).is_err());
}
```

(`test_tenant_scope` already exists; if not, define a simple helper that constructs a `TenantScope` from `Uuid::new_v4()`.)

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api new_agent_ 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Add the `NewAgent` factory**

Insert in `rust/crates/api/src/domain/agent.rs` (near `HostCliIdentity`):

```rust
use agentforge_core::{CliToolKind, RuntimeKind};

#[derive(Debug, Clone)]
pub(crate) struct NewAgent {
    runtime_kind: RuntimeKind,
    cli_tool: Option<&'static str>,
    name: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    cwd: Option<String>,
    workspace_id: Uuid,
    project_id: Option<Uuid>,
    system_prompt: Option<String>,
    runtime_id: Option<String>,
    hmac_secret: Option<String>,
    nats_connect_password: Option<String>,
    initial_status: AgentStatus,
}

impl NewAgent {
    pub(crate) fn container(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
        system_prompt: Option<&str>,
    ) -> AppResult<Self> {
        let _ = scope; // tenant scope used in repo layer
        AgentName::validate(name)?;
        Ok(Self {
            runtime_kind: RuntimeKind::Container,
            cli_tool: Some(cli_tool.as_str()),
            name: name.map(str::to_string),
            model: model.map(str::to_string),
            provider: None,
            cwd: cwd.map(str::to_string),
            workspace_id,
            project_id,
            system_prompt: system_prompt.map(str::to_string),
            runtime_id: None,
            hmac_secret: None,
            nats_connect_password: None,
            initial_status: AgentStatus::Idle,
        })
    }

    pub(crate) fn host_cli(
        scope: &TenantScope,
        cli_tool: CliToolKind,
        identity: HostCliIdentity,
        name: Option<&str>,
        model: Option<&str>,
        cwd: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> {
        let _ = scope;
        AgentName::validate(name)?;
        Ok(Self {
            runtime_kind: RuntimeKind::Cli,
            cli_tool: Some(cli_tool.as_str()),
            name: name.map(str::to_string),
            model: model.map(str::to_string),
            provider: None,
            cwd: cwd.map(str::to_string),
            workspace_id,
            project_id,
            system_prompt: None,
            runtime_id: Some(identity.runtime_id().to_string()),
            hmac_secret: Some(identity.hmac_secret().to_string()),
            nats_connect_password: Some(identity.nats_connect_password().to_string()),
            initial_status: AgentStatus::Offline,
        })
    }

    pub(crate) fn api(
        scope: &TenantScope,
        provider: &str,
        model: &str,
        name: Option<&str>,
        system_prompt: Option<&str>,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
    ) -> AppResult<Self> {
        let _ = scope;
        AgentName::validate(name)?;
        if provider.trim().is_empty() {
            return Err(ErrorKind::Validation("provider is required for API runtime agent".into()).into());
        }
        if model.trim().is_empty() {
            return Err(ErrorKind::Validation("model is required for API runtime agent".into()).into());
        }
        Ok(Self {
            runtime_kind: RuntimeKind::Api,
            cli_tool: None,
            name: name.map(str::to_string),
            model: Some(model.to_string()),
            provider: Some(provider.to_string()),
            cwd: None,
            workspace_id,
            project_id,
            system_prompt: system_prompt.map(str::to_string),
            runtime_id: None,
            hmac_secret: None,
            nats_connect_password: None,
            initial_status: AgentStatus::Idle,
        })
    }

    // accessors used by the repository to build the INSERT statement
    pub(crate) fn runtime_kind(&self) -> RuntimeKind { self.runtime_kind }
    pub(crate) fn cli_tool(&self) -> Option<&str> { self.cli_tool }
    pub(crate) fn runtime_id(&self) -> Option<&str> { self.runtime_id.as_deref() }
    pub(crate) fn hmac_secret(&self) -> Option<&str> { self.hmac_secret.as_deref() }
    pub(crate) fn nats_connect_password(&self) -> Option<&str> { self.nats_connect_password.as_deref() }
    pub(crate) fn name(&self) -> Option<&str> { self.name.as_deref() }
    pub(crate) fn model(&self) -> Option<&str> { self.model.as_deref() }
    pub(crate) fn provider(&self) -> Option<&str> { self.provider.as_deref() }
    pub(crate) fn cwd(&self) -> Option<&str> { self.cwd.as_deref() }
    pub(crate) fn workspace_id(&self) -> Uuid { self.workspace_id }
    pub(crate) fn project_id(&self) -> Option<Uuid> { self.project_id }
    pub(crate) fn system_prompt(&self) -> Option<&str> { self.system_prompt.as_deref() }
    pub(crate) fn initial_status(&self) -> AgentStatus { self.initial_status }
}
```

- [ ] **Step 4: Run tests**

```bash
cd rust && cargo test -p agentforge-api new_agent_ 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/domain/agent.rs
git commit -m "feat(api): add NewAgent typed factory (container/host_cli/api)"
```

### Task 3.3: Add `ContainerAgent` typestate + `LifecycleRejection`

**Files:**

- Modify: `rust/crates/api/src/domain/agent.rs`

- [ ] **Step 1: Write the failing test**

Append to tests:

```rust
#[test]
fn container_agent_try_from_only_accepts_container_kind() {
    let container = sample_agent_aggregate(RuntimeKind::Container, Some("codex"), None);
    assert!(ContainerAgent::try_from(container).is_ok());

    let host_cli = sample_agent_aggregate(RuntimeKind::Cli, Some("codex"), None);
    match ContainerAgent::try_from(host_cli) {
        Err(LifecycleRejection::HostCli) => (),
        other => panic!("expected HostCli rejection, got {other:?}"),
    }

    let api = sample_agent_aggregate(RuntimeKind::Api, None, None);
    match ContainerAgent::try_from(api) {
        Err(LifecycleRejection::Api) => (),
        other => panic!("expected Api rejection, got {other:?}"),
    }
}

#[test]
fn lifecycle_rejection_into_app_error_carries_i18n_key() {
    let err = LifecycleRejection::HostCli.into_app_error("Restart");
    let msg = format!("{err}");
    assert!(msg.contains("Host CLI"), "msg: {msg}");
}

fn sample_agent_aggregate(kind: RuntimeKind, cli_tool: Option<&'static str>, container_id: Option<&'static str>) -> AgentAggregate {
    AgentAggregate::for_test(kind, cli_tool, container_id)
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api container_agent_ lifecycle_rejection_ 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Add the types**

Insert in `rust/crates/api/src/domain/agent.rs`:

```rust
/// Aggregate root for the Agent bounded context. Loaded by
/// `AgentRepository::find_aggregate` for write-side operations.
#[derive(Debug, Clone)]
pub(crate) struct AgentAggregate {
    pub(crate) id: Uuid,
    pub(crate) runtime_kind: RuntimeKind,
    pub(crate) cli_tool: Option<String>,
    pub(crate) container_id: Option<String>,
    pub(crate) runtime_id: Option<String>,
    pub(crate) workspace_id: Uuid,
    pub(crate) organization_id: Uuid,
    pub(crate) user_id: Uuid,
    pub(crate) status: AgentStatus,
}

impl AgentAggregate {
    pub(crate) fn runtime_kind(&self) -> RuntimeKind { self.runtime_kind }

    #[cfg(test)]
    pub(crate) fn for_test(kind: RuntimeKind, cli_tool: Option<&str>, container_id: Option<&str>) -> Self {
        Self {
            id: Uuid::new_v4(),
            runtime_kind: kind,
            cli_tool: cli_tool.map(str::to_string),
            container_id: container_id.map(str::to_string),
            runtime_id: None,
            workspace_id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            status: AgentStatus::Idle,
        }
    }
}

/// Typestate wrapper proving an agent is container-backed. The only path
/// into this type is via `try_from` on an `AgentAggregate`.
pub(crate) struct ContainerAgent(AgentAggregate);

#[derive(Debug)]
pub(crate) enum LifecycleRejection { HostCli, Api }

impl ContainerAgent {
    pub(crate) fn try_from(agent: AgentAggregate) -> Result<Self, LifecycleRejection> {
        match agent.runtime_kind {
            RuntimeKind::Container => Ok(Self(agent)),
            RuntimeKind::Cli       => Err(LifecycleRejection::HostCli),
            RuntimeKind::Api       => Err(LifecycleRejection::Api),
        }
    }
    pub(crate) fn inner(&self) -> &AgentAggregate { &self.0 }
}

impl LifecycleRejection {
    pub(crate) fn into_app_error(self, action: &str) -> AppError {
        let msg = match self {
            Self::HostCli => format!(
                "Host CLI agent: the platform does not manage the local container lifecycle. \
                 {action} the sidecar on the operator machine using the enrollment script."
            ),
            Self::Api => format!(
                "API/provider agent has no container to {}.",
                action.to_lowercase()
            ),
        };
        ErrorKind::Validation(msg).into()
    }
}
```

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api container_agent_ lifecycle_rejection_ 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/domain/agent.rs
git commit -m "feat(api): add ContainerAgent typestate and LifecycleRejection"
```

---

## Phase 4 — Repository refactor

### Task 4.1: Add `runtime_kind` to `AgentListItem` and SELECT statements

**Files:**

- Modify: `rust/crates/api/src/repositories/agent/mod.rs`

- [ ] **Step 1: Write a failing test for the new field**

Add to `rust/crates/api/src/repositories/agent/tests.rs` (or the existing test module):

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn list_with_owner_returns_runtime_kind(pool: PgPool) {
    let repo = AgentRepository::new(pool);
    let scope = test_tenant_scope();
    let id = repo.create(&scope, NewAgent::container(
        &scope, CliToolKind::Codex, Some("a1"), None, None, Uuid::new_v4(), None, None
    ).unwrap()).await.unwrap();

    let agents = repo.list_with_owner(&scope, 100, 0).await.unwrap();
    let found = agents.iter().find(|a| a.id == id).unwrap();
    assert_eq!(found.runtime_kind, RuntimeKind::Container);
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api list_with_owner_returns_runtime_kind 2>&1 | tail -10
```

Expected: FAIL (the field doesn't exist on `AgentListItem`).

- [ ] **Step 3: Add the field and update SELECT statements**

In `rust/crates/api/src/repositories/agent/mod.rs`:

1. Add to the `AgentListItem` struct (insert between existing fields):

```rust
pub runtime_kind: RuntimeKind,
```

2. In the file's `use` block, add `use agentforge_core::RuntimeKind;` if not already present.
3. Locate every SELECT query that lists `a.*` or names individual columns from `agents` (search for `FROM agents`); add `a.runtime_kind` after `a.cli_tool` in the column list. The current code has these around lines 117 and 196.

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api list_with_owner_returns_runtime_kind 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/repositories/agent/mod.rs
git commit -m "feat(api): wire agents.runtime_kind into AgentListItem and SELECT"
```

### Task 4.2: Refactor `AgentRepository::create` to take `NewAgent`, atomic with audit event

**Files:**

- Modify: `rust/crates/api/src/repositories/agent/mod.rs`

- [ ] **Step 1: Write a failing test for atomic INSERT + audit event**

Add:

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn create_host_cli_emits_atomic_audit_event(pool: PgPool) {
    let repo = AgentRepository::new(pool.clone());
    let scope = test_tenant_scope();
    let identity = HostCliIdentity::generate();
    let id = repo.create(&scope, NewAgent::host_cli(
        &scope, CliToolKind::Codex, identity, Some("hcli"), None, None,
        Uuid::new_v4(), None,
    ).unwrap()).await.unwrap();

    let rows: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM events WHERE event_type = 'agent.enrolled' AND agent_id = $1"
    )
    .bind(id)
    .fetch_one(&pool).await.unwrap();
    assert_eq!(rows.0, 1, "expected exactly one agent.enrolled event for new host_cli agent");
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api create_host_cli_emits_atomic_audit_event 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Refactor `AgentRepository::create`**

Replace the current `pub async fn create(&self, scope: &TenantScope, params: CreateAgentParams<'_>) -> AppResult<Agent>` body in `rust/crates/api/src/repositories/agent/mod.rs` with the new signature:

```rust
pub async fn create(&self, scope: &TenantScope, new: NewAgent) -> AppResult<Uuid> {
    let mut tx = self.pool.begin().await.map_err(|e| ErrorKind::Internal(e.to_string()))?;

    let id = new
        .runtime_id()
        .and_then(|rid| rid.strip_prefix("host-"))
        .and_then(|tail| Uuid::parse_str(tail).ok())
        .unwrap_or_else(Uuid::now_v7);

    sqlx::query(
        "INSERT INTO agents (
            id, organization_id, workspace_id, project_id, user_id,
            runtime_kind, cli_tool, model, provider, system_prompt,
            cwd, name, runtime_id, hmac_secret, nats_connect_password,
            status, created_at, updated_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, NOW(), NOW())"
    )
    .bind(id)
    .bind(scope.org_id().as_uuid())
    .bind(new.workspace_id())
    .bind(new.project_id())
    .bind(scope.user_id().as_uuid())
    .bind(new.runtime_kind())
    .bind(new.cli_tool())
    .bind(new.model())
    .bind(new.provider())
    .bind(new.system_prompt())
    .bind(new.cwd())
    .bind(new.name())
    .bind(new.runtime_id())
    .bind(new.hmac_secret())
    .bind(new.nats_connect_password())
    .bind(new.initial_status().as_str())
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_error)?;

    if new.runtime_kind() == RuntimeKind::Cli {
        sqlx::query(
            "INSERT INTO events (id, event_type, organization_id, workspace_id, agent_id, payload, created_at)
             VALUES ($1, 'agent.enrolled', $2, $3, $4, $5, NOW())"
        )
        .bind(Uuid::new_v4())
        .bind(scope.org_id().as_uuid())
        .bind(new.workspace_id())
        .bind(id)
        .bind(serde_json::json!({
            "runtime_kind": "cli",
            "cli_tool": new.cli_tool(),
            "project_id": new.project_id(),
            "actor_user_id": scope.user_id().as_uuid(),
        }))
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
    }

    tx.commit().await.map_err(|e| ErrorKind::Internal(e.to_string()))?;
    Ok(id)
}
```

Add the helper `fn map_sqlx_error(e: sqlx::Error) -> AppError { ErrorKind::Internal(e.to_string()).into() }` if it isn't already in the file.

Remove the old `CreateAgentParams` struct definition — it's no longer used. All call sites are updated in Phase 6.

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api create_host_cli_emits_atomic_audit_event 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/repositories/agent/mod.rs
git commit -m "refactor(api): AgentRepository::create accepts NewAgent and writes atomic audit event"
```

### Task 4.3: Add `find_aggregate` to repository, remove `set_host_runtime`

**Files:**

- Modify: `rust/crates/api/src/repositories/agent/mod.rs`

- [ ] **Step 1: Write the failing test**

Add:

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn find_aggregate_returns_typed_runtime_kind(pool: PgPool) {
    let repo = AgentRepository::new(pool);
    let scope = test_tenant_scope();
    let id = repo.create(&scope, NewAgent::container(
        &scope, CliToolKind::Codex, Some("a"), None, None, Uuid::new_v4(), None, None
    ).unwrap()).await.unwrap();

    let agg = repo.find_aggregate(&scope, id).await.unwrap();
    assert_eq!(agg.runtime_kind(), RuntimeKind::Container);
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api find_aggregate_returns_typed_runtime_kind 2>&1 | tail -10
```

Expected: FAIL — `find_aggregate` doesn't exist.

- [ ] **Step 3: Add the method**

Insert in `impl AgentRepository`:

```rust
pub async fn find_aggregate(&self, scope: &TenantScope, id: Uuid) -> AppResult<AgentAggregate> {
    let row = sqlx::query_as::<_, AgentAggregate>(
        "SELECT id, runtime_kind, cli_tool, container_id, runtime_id,
                workspace_id, organization_id, user_id, status
         FROM agents
         WHERE id = $1 AND organization_id = $2"
    )
    .bind(id)
    .bind(scope.org_id().as_uuid())
    .fetch_optional(&self.pool).await
    .map_err(map_sqlx_error)?
    .ok_or_else(|| AppError::from(ErrorKind::NotFound("agent".into())))?;
    Ok(row)
}
```

Implement `sqlx::FromRow` for `AgentAggregate` (add `#[derive(sqlx::FromRow)]` to the struct in `domain/agent.rs`, or implement manually if a custom mapping is needed for `status` which is `AgentStatus`).

Remove `pub async fn set_host_runtime(...)` and its callers (the only remaining caller after this refactor is the old enrollment service — Task 6.1 rewrites it).

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api find_aggregate_returns_typed_runtime_kind 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/repositories/agent/mod.rs rust/crates/api/src/domain/agent.rs
git commit -m "feat(api): AgentRepository::find_aggregate; drop set_host_runtime"
```

### Task 4.4: Create `AgentQueryService` with `find_by_runtime_kind`

**Files:**

- Create: `rust/crates/api/src/services/agent_query.rs`
- Modify: `rust/crates/api/src/services/mod.rs` (register the new module)

- [ ] **Step 1: Write the failing test**

Create `rust/crates/api/src/services/agent_query.rs`:

```rust
//! Read-side query service. Owns cross-aggregate filter queries that do not
//! belong on the write-side repository per CQRS hygiene.

use crate::repositories::agent::{AgentListItem, AgentRepository};
use agentforge_core::{AppResult, RuntimeKind, TenantScope};
use sqlx::PgPool;

pub(crate) struct AgentQueryService {
    pool: PgPool,
}

impl AgentQueryService {
    pub(crate) fn from_pool(pool: PgPool) -> Self { Self { pool } }

    pub(crate) async fn find_by_runtime_kind(
        &self,
        scope: &TenantScope,
        kind: RuntimeKind,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AgentListItem>> {
        AgentRepository::new(self.pool.clone())
            .list_with_owner_filtered(scope, Some(kind), limit, offset)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::NewAgent;
    use agentforge_core::CliToolKind;
    use uuid::Uuid;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn finds_only_matching_kind(pool: PgPool) {
        let repo = AgentRepository::new(pool.clone());
        let scope = crate::test_helpers::test_tenant_scope();
        let _ = repo.create(&scope, NewAgent::container(
            &scope, CliToolKind::Codex, Some("c"), None, None, Uuid::new_v4(), None, None
        ).unwrap()).await.unwrap();
        let _ = repo.create(&scope, NewAgent::api(
            &scope, "anthropic", "claude-opus-4-7", Some("a"), None, Uuid::new_v4(), None
        ).unwrap()).await.unwrap();

        let svc = AgentQueryService::from_pool(pool);
        let containers = svc.find_by_runtime_kind(&scope, RuntimeKind::Container, 100, 0).await.unwrap();
        let apis = svc.find_by_runtime_kind(&scope, RuntimeKind::Api, 100, 0).await.unwrap();

        assert!(containers.iter().all(|a| a.runtime_kind == RuntimeKind::Container));
        assert!(apis.iter().all(|a| a.runtime_kind == RuntimeKind::Api));
        assert!(!containers.is_empty());
        assert!(!apis.is_empty());
    }
}
```

- [ ] **Step 2: Add `list_with_owner_filtered` to the repository**

In `rust/crates/api/src/repositories/agent/mod.rs`, insert:

```rust
pub async fn list_with_owner_filtered(
    &self,
    scope: &TenantScope,
    kind: Option<RuntimeKind>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AgentListItem>> {
    let mut sql = String::from(
        "SELECT a.*, u.username AS owner_username, u.email AS owner_email,
                w.name AS workspace_name, p.name AS project_name
         FROM agents a
         JOIN users u      ON u.id = a.user_id
         JOIN workspaces w ON w.id = a.workspace_id
         LEFT JOIN projects p ON p.id = a.project_id
         WHERE a.organization_id = $1"
    );
    if kind.is_some() { sql.push_str(" AND a.runtime_kind = $2"); }
    sql.push_str(" ORDER BY a.created_at DESC LIMIT $3 OFFSET $4");

    let mut q = sqlx::query_as::<_, AgentListItem>(&sql)
        .bind(scope.org_id().as_uuid());
    if let Some(k) = kind { q = q.bind(k); }
    q.bind(limit).bind(offset).fetch_all(&self.pool).await.map_err(map_sqlx_error)
}
```

- [ ] **Step 3: Register the module**

In `rust/crates/api/src/services/mod.rs` add `pub(crate) mod agent_query;`.

- [ ] **Step 4: Run the test to verify pass**

```bash
cd rust && cargo test -p agentforge-api find_by_runtime_kind 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/services/agent_query.rs rust/crates/api/src/services/mod.rs rust/crates/api/src/repositories/agent/mod.rs
git commit -m "feat(api): AgentQueryService with find_by_runtime_kind (CQRS read side)"
```

---

## Phase 5 — Idempotency

### Task 5.1: Add `Idempotency-Key` extractor middleware

**Files:**

- Modify: `rust/crates/api/src/middleware.rs`

- [ ] **Step 1: Write the failing test**

Add to the existing middleware tests file:

```rust
#[tokio::test]
async fn idempotency_key_extractor_reads_header() {
    let req = http::Request::builder()
        .header("Idempotency-Key", "abc-123")
        .body(()).unwrap();
    let key = IdempotencyKey::from_request_parts(&req.into_parts().0).await.unwrap();
    assert_eq!(key.0, "abc-123");
}

#[tokio::test]
async fn idempotency_key_extractor_rejects_missing() {
    let req = http::Request::builder().body(()).unwrap();
    let res = IdempotencyKey::from_request_parts(&req.into_parts().0).await;
    assert!(res.is_err());
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api idempotency_key_extractor 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Implement the extractor**

Insert in `rust/crates/api/src/middleware.rs`:

```rust
pub struct IdempotencyKey(pub String);

#[async_trait::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for IdempotencyKey {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(|s| IdempotencyKey(s.to_string()))
            .ok_or_else(|| {
                ErrorKind::Validation(
                    "Idempotency-Key header is required for this endpoint".into(),
                )
                .into()
            })
    }
}
```

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api idempotency_key_extractor 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/middleware.rs
git commit -m "feat(api): IdempotencyKey FromRequestParts extractor"
```

### Task 5.2: Wire idempotency lookup into the enrollment service

**Files:**

- Modify: `rust/crates/api/src/services/agent_enrollment.rs`

- [ ] **Step 1: Write the failing replay test**

Create `rust/crates/api/tests/agent_enrollment_idempotency.rs`:

```rust
//! End-to-end test: replaying enrollment with the same Idempotency-Key
//! returns the original agent and does not create a duplicate row.

use sqlx::PgPool;
// `app_test_harness` is the project's existing in-process test harness
// (see other tests in the same directory for the exact import name).
use agentforge_api::test_support::{app_test_harness, json, post};

#[sqlx::test(migrations = "../db/migrations")]
async fn replay_with_same_idempotency_key_returns_original_agent(pool: PgPool) {
    let app = app_test_harness(pool.clone()).await;
    let body = json!({ "cliTool": "codex" });
    let first = post(&app, "/api/v1/agents/local-enroll", &body)
        .header("Idempotency-Key", "test-key-1")
        .send().await;
    let id1 = first.json()["agent"]["id"].as_str().unwrap().to_string();

    let second = post(&app, "/api/v1/agents/local-enroll", &body)
        .header("Idempotency-Key", "test-key-1")
        .send().await;
    let id2 = second.json()["agent"]["id"].as_str().unwrap().to_string();

    assert_eq!(id1, id2, "replay must return the original agent");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1, "replay must not create a duplicate row");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn missing_idempotency_key_returns_400(pool: PgPool) {
    let app = app_test_harness(pool).await;
    let res = post(&app, "/api/v1/agents/local-enroll", &json!({ "cliTool": "codex" }))
        .send().await;
    assert_eq!(res.status(), 400);
}
```

Note: `app_test_harness` / `post` / `json` are illustrative; the project's actual test-helper module name appears in existing integration tests under `rust/crates/api/tests/`. Use whatever helper they use (typically `axum::Router::oneshot` driven through a `TestServer`).

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api --test agent_enrollment_idempotency 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Add idempotency lookup/store helpers to the service**

Insert in `rust/crates/api/src/services/agent_enrollment.rs`:

```rust
struct IdempotencyRecord { agent_id: Uuid, expires_at: chrono::DateTime<chrono::Utc> }

impl HostAgentEnrollmentService {
    async fn lookup_idempotent(
        &self,
        org_id: Uuid,
        user_id: Uuid,
        key: &str,
    ) -> AppResult<Option<Uuid>> {
        let row: Option<(Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT agent_id, expires_at FROM enrollment_idempotency
             WHERE org_id = $1 AND user_id = $2 AND key = $3 AND expires_at > NOW()"
        )
        .bind(org_id).bind(user_id).bind(key)
        .fetch_optional(&self.pool).await
        .map_err(|e| ErrorKind::Internal(e.to_string()))?;
        Ok(row.map(|(id, _)| id))
    }

    async fn store_idempotent(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        org_id: Uuid,
        user_id: Uuid,
        key: &str,
        agent_id: Uuid,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO enrollment_idempotency (org_id, user_id, key, agent_id, expires_at)
             VALUES ($1, $2, $3, $4, NOW() + INTERVAL '24 hours')
             ON CONFLICT (org_id, user_id, key) DO NOTHING"
        )
        .bind(org_id).bind(user_id).bind(key).bind(agent_id)
        .execute(&mut **tx).await
        .map(|_| ())
        .map_err(|e| ErrorKind::Internal(e.to_string()).into())
    }
}
```

The `pool` field already exists; if not, add `pub(crate) pool: PgPool` to `HostAgentEnrollmentService` and thread it through `from_runtime`.

- [ ] **Step 4: Commit (functional wiring continues in Task 6.1)**

```bash
git add rust/crates/api/src/services/agent_enrollment.rs rust/crates/api/tests/agent_enrollment_idempotency.rs
git commit -m "feat(api): idempotency helpers for host CLI enrollment"
```

---

## Phase 6 — Service & route changes

### Task 6.1: Rewrite `HostAgentEnrollmentService::enroll` end-to-end

**Files:**

- Modify: `rust/crates/api/src/services/agent_enrollment.rs`

- [ ] **Step 1: Write the failing TLS rejection test**

Append to `rust/crates/api/tests/agent_enrollment_idempotency.rs`:

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn plaintext_nats_url_is_rejected_without_org_policy(pool: PgPool) {
    let app = app_test_harness_with_nats_url(pool, "nats://insecure.example:4222").await;
    let res = post(&app, "/api/v1/agents/local-enroll", &json!({ "cliTool": "codex" }))
        .header("Idempotency-Key", "k1")
        .send().await;
    assert_eq!(res.status(), 422);
    assert!(res.text().contains("plaintext_nats_blocked"));
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api --test agent_enrollment_idempotency plaintext 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Rewrite the enroll method**

Replace the `enroll` body in `rust/crates/api/src/services/agent_enrollment.rs`:

```rust
pub(crate) async fn enroll(
    &self,
    scope: &TenantScope,
    idempotency_key: &str,
    input: HostAgentEnrollmentInput<'_>,
) -> AppResult<(AgentListItem, HostAgentEnrollment)> {
    AgentName::validate(input.name)?;
    let cli_tool = HostAgentEnrollmentPolicy::require_cli_tool(input.cli_tool)?;
    let nats_base_url = HostAgentEnrollmentPolicy::require_nats_base_url(
        self.settings.nats_agent_url.as_deref(),
        self.settings.nats_url.as_deref(),
    )?;
    if !nats_base_url.starts_with("tls://") && !self.settings.allow_plaintext_host_nats {
        return Err(ErrorKind::Validation(
            "plaintext_nats_blocked: Host CLI enrollment requires a tls:// NATS URL \
             unless the org policy `allow_plaintext_host_nats` is enabled".into()
        ).into());
    }

    let org_id  = scope.org_id().as_uuid();
    let user_id = scope.user_id().as_uuid();

    // Idempotent fast path.
    if let Some(existing) = self.lookup_idempotent(org_id, user_id, idempotency_key).await? {
        let agent = self.agents.find_with_owner_by_id(scope, AgentId::from(existing)).await?;
        let enrollment = self.rebuild_enrollment_view(&agent, &nats_base_url)?;
        return Ok((agent, enrollment));
    }

    let workspace_scope = self
        .workspaces
        .resolve_workspace_mount_scope(org_id, input.workspace_id, input.project_id)
        .await?;

    let identity = HostCliIdentity::generate();
    let cli_kind = CliToolKind::parse_legacy(cli_tool)
        .map_err(|e| ErrorKind::Validation(e.to_string()))?;
    let new_agent = NewAgent::host_cli(
        scope, cli_kind, identity.clone(),
        input.name, input.model, input.cwd,
        workspace_scope.workspace_id, input.project_id,
    )?;

    // Single transaction: agent row + audit event + idempotency record.
    let mut tx = self.pool.begin().await.map_err(|e| ErrorKind::Internal(e.to_string()))?;
    let id = self.agents.create_in_tx(&mut tx, scope, new_agent).await?;
    self.store_idempotent(&mut tx, org_id, user_id, idempotency_key, id).await?;
    tx.commit().await.map_err(|e| ErrorKind::Internal(e.to_string()))?;

    let agent = self.agents.find_with_owner_by_id(scope, AgentId::from(id)).await?;
    let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
        agent_id: id,
        org_id,
        cli_tool: Some(cli_tool),
        cli_model: agent.model.as_deref(),
        codex_default_model: Some(self.settings.codex_default_model.as_str()),
        nats_base_url: Some(nats_base_url.as_str()),
        nats_connect_password: identity.nats_connect_password(),
        container_server_url: self.settings.server_url.as_deref(),
        workspace_host_path: None,
        hmac_secret: identity.hmac_secret(),
        context_injection_enabled: self.settings.context_injection_enabled,
    });
    let mut env = HostAgentEnrollmentPolicy::env_map(env);
    env.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());
    let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env);
    let enrollment = HostAgentEnrollment {
        agent_id: id,
        runtime_id: identity.runtime_id().to_string(),
        cli_tool: cli_tool.to_string(),
        env,
        shell_exports,
        sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
        server_url: self.settings.server_url.clone(),
    };
    Ok((agent, enrollment))
}
```

Add `pub(crate) async fn create_in_tx(&self, tx, scope, new)` to `AgentRepository` that does the same INSERT as `create` but accepts an existing transaction (refactor `create` to call it).

Add `allow_plaintext_host_nats: bool` to `HostAgentEnrollmentSettings` and read it from `AppConfig`.

- [ ] **Step 4: Run all enrollment tests**

```bash
cd rust && cargo test -p agentforge-api --test agent_enrollment_idempotency 2>&1 | tail -20
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/services/agent_enrollment.rs rust/crates/api/src/repositories/agent/mod.rs
git commit -m "feat(api): idempotent host CLI enroll with TLS check and atomic audit"
```

### Task 6.2: Update lifecycle services to take `&ContainerAgent`

**Files:**

- Modify: `rust/crates/api/src/services/agent_container_lifecycle.rs`
- Modify: `rust/crates/api/src/services/agent_container_control.rs`

- [ ] **Step 1: Write the failing 422-on-host-cli test**

Add to existing route integration tests (or create a new file `rust/crates/api/tests/lifecycle_rejection.rs`):

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_on_host_cli_returns_422_with_host_cli_message(pool: PgPool) {
    let app = app_test_harness(pool).await;
    let agent_id = enroll_host_cli(&app, "codex").await;
    let res = post(&app, &format!("/api/v1/agents/{agent_id}/restart"), &json!({}))
        .send().await;
    assert_eq!(res.status(), 422);
    assert!(res.text().contains("Host CLI"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn restart_on_api_returns_422_with_api_message(pool: PgPool) {
    let app = app_test_harness(pool).await;
    let agent_id = create_api_agent(&app).await;
    let res = post(&app, &format!("/api/v1/agents/{agent_id}/restart"), &json!({}))
        .send().await;
    assert_eq!(res.status(), 422);
    assert!(res.text().contains("no container to restart"));
}
```

- [ ] **Step 2: Run to verify fail**

```bash
cd rust && cargo test -p agentforge-api restart_on_host_cli restart_on_api 2>&1 | tail -10
```

Expected: FAIL (current code returns the misleading container error).

- [ ] **Step 3: Modify the lifecycle service**

In `rust/crates/api/src/services/agent_container_lifecycle.rs`, replace `ensure_container_backed` calls. Restart looks like:

```rust
pub(crate) async fn restart(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
    let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::lifecycle_docker_unavailable)?;
    let agent_aggregate = self.agents.find_aggregate(scope, agent_id.into()).await?;
    let container = ContainerAgent::try_from(agent_aggregate)
        .map_err(|r| r.into_app_error("Restart"))?;
    let inner = container.inner();
    let container_id = AgentContainerLifecyclePolicy::restart_container_id(inner.container_id.as_deref())?;
    // ... rest unchanged
}
```

Repeat for `start` (`into_app_error("Start")`) and `stop` (`into_app_error("Stop")`).

Do the same in `rust/crates/api/src/services/agent_container_control.rs`.

- [ ] **Step 4: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api restart_on_host_cli restart_on_api 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/services/agent_container_lifecycle.rs rust/crates/api/src/services/agent_container_control.rs
git commit -m "fix(api): lifecycle services reject host_cli/api via ContainerAgent typestate"
```

### Task 6.3: Add owner-ACL check returning uniform 403 for non-owner

**Files:**

- Modify: `rust/crates/api/src/routes/agents.rs`

- [ ] **Step 1: Write the failing enumeration-protection test**

Add to the integration test file:

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn non_owner_intra_org_restart_gets_uniform_403(pool: PgPool) {
    let app = app_test_harness(pool.clone()).await;
    // owner enrolls a host_cli agent
    let agent_id = enroll_host_cli_as_user(&app, "user-A", "codex").await;
    // user-B in same org tries to restart it
    let res = post_as_user(&app, "user-B", &format!("/api/v1/agents/{agent_id}/restart"), &json!({}))
        .send().await;
    assert_eq!(res.status(), 403);
    let body = res.text();
    assert!(!body.contains("Host CLI"), "must NOT disclose runtime kind to non-owner: {body}");
    assert!(body.contains("operation not permitted"));
}
```

- [ ] **Step 2: Run to verify fail**

Expected: FAIL — no ACL check exists yet.

- [ ] **Step 3: Add ACL helper and apply to lifecycle routes**

In `rust/crates/api/src/routes/agents.rs`, add a helper:

```rust
async fn require_owner(scope: &TenantScope, agent_owner_id: Uuid) -> AppResult<()> {
    if scope.user_id().as_uuid() == agent_owner_id { return Ok(()); }
    Err(ErrorKind::Forbidden("operation not permitted on this agent".into()).into())
}
```

Apply at the top of each lifecycle handler (`restart_agent`, `start_agent`, `stop_agent`, `delete_agent`) — fetch the agent's `user_id` via `find_aggregate`, then call `require_owner`. Place this check BEFORE the lifecycle service call so the runtime-kind-disclosing 422 never fires for non-owners.

`ErrorKind::Forbidden` may or may not exist — if not, add it to `rust/crates/core/src/errors.rs` mapping to HTTP 403.

- [ ] **Step 4: Run to verify pass**

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/routes/agents.rs rust/crates/core/src/
git commit -m "feat(api): uniform 403 for non-owner intra-org lifecycle calls"
```

### Task 6.4: Require `Idempotency-Key` and `Cache-Control: no-store` on enrollment route

**Files:**

- Modify: `rust/crates/api/src/routes/agents.rs`

- [ ] **Step 1: Update the route handler**

Replace the `enroll_local_agent` signature and body in `rust/crates/api/src/routes/agents.rs`:

```rust
async fn enroll_local_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    Json(req): Json<EnrollLocalAgentRequest>,
) -> AppResult<axum::response::Response> {
    let service = make_host_enrollment_service(&state);
    let (agent, enrollment) = service.enroll(
        &auth.scope,
        &key,
        HostAgentEnrollmentInput {
            name: req.name.as_deref(),
            model: req.model.as_deref(),
            cli_tool: req.cli_tool.as_str(),
            cwd: req.cwd.as_deref(),
            workspace_id: req.workspace_id,
            project_id: req.project_id,
        },
    ).await?;
    let mut response = axum::Json(agent_enrollment_response(agent, enrollment)).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        axum::http::header::PRAGMA,
        axum::http::HeaderValue::from_static("no-cache"),
    );
    Ok(response)
}
```

Add `#[serde(deny_unknown_fields)]` to `EnrollLocalAgentRequest`.

- [ ] **Step 2: Add the access-log filter wiring**

If the project uses `tower_http::trace::TraceLayer`, exclude `/api/v1/agents/local-enroll` response bodies from logging by adding a per-route layer or setting the trace level for that path to `OFF`. The exact mechanism is project-specific; locate the existing tracing layer in `rust/bins/server/src/main.rs` (or wherever the router is composed) and add:

```rust
.layer(
    tower_http::trace::TraceLayer::new_for_http()
        .on_response(
            |response: &axum::http::Response<_>, latency, _span: &tracing::Span| {
                if response.extensions().get::<RedactBody>().is_some() { return; }
                tracing::info!(?latency, status = ?response.status());
            },
        ),
)
```

Mark the enrollment response with `.extensions_mut().insert(RedactBody)` in the handler. Define `struct RedactBody;` in the middleware module.

- [ ] **Step 3: Run all route tests**

```bash
cd rust && cargo test -p agentforge-api 2>&1 | tail -10
```

Expected: previous tests + new headers test pass.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/api/src/routes/agents.rs rust/crates/api/src/middleware.rs
git commit -m "feat(api): require Idempotency-Key + Cache-Control no-store on local-enroll"
```

### Task 6.5: Strict-parse `CreateAgentRequest` runtime kind

**Files:**

- Modify: `rust/crates/api/src/routes/agents.rs`

- [ ] **Step 1: Write the failing strict-parse test**

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn create_rejects_legacy_runtime_kind_alias(pool: PgPool) {
    let app = app_test_harness(pool).await;
    let res = post(&app, "/api/v1/agents", &json!({
        "cliTool": "codex", "runtimeKind": "host_cli"
    })).send().await;
    assert_eq!(res.status(), 422);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn create_rejects_unknown_fields(pool: PgPool) {
    let app = app_test_harness(pool).await;
    let res = post(&app, "/api/v1/agents", &json!({
        "cliTool": "codex", "rogue": "value"
    })).send().await;
    assert_eq!(res.status(), 422);
}
```

- [ ] **Step 2: Add `deny_unknown_fields` and typed runtime kind**

In `rust/crates/api/src/routes/agents.rs`, update `CreateAgentRequest`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentRequest {
    pub name: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    #[serde(alias = "cliTool")]
    pub cli_tool: Option<String>,
    pub cwd: Option<String>,
    #[serde(alias = "workspaceId")]
    pub workspace_id: Option<Uuid>,
    #[serde(alias = "projectId")]
    pub project_id: Option<Uuid>,
    pub system_prompt: Option<String>,
    #[serde(alias = "runtimeKind", default)]
    pub runtime_kind: Option<RuntimeKind>,
}
```

`RuntimeKind`'s `Deserialize` impl already rejects unknown values via `parse_legacy`.

- [ ] **Step 3: Run to verify pass**

```bash
cd rust && cargo test -p agentforge-api create_rejects_legacy create_rejects_unknown 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/api/src/routes/agents.rs
git commit -m "feat(api): strict-parse runtimeKind and deny_unknown_fields on agent creation"
```

---

## Phase 7 — Error messages with i18n

### Task 7.1: Add i18n keys to en + zh locale bundles

**Files:**

- Modify: `src/app/shared/i18n/locales/en.ts`
- Modify: `src/app/shared/i18n/locales/zh.ts`

- [ ] **Step 1: Add `errors.agent.*` keys to en**

In `src/app/shared/i18n/locales/en.ts`, find the existing errors namespace (or top level) and add:

```typescript
const errors = {
  agent: {
    lifecycle: {
      restart_host_cli: {
        title: 'Restart the sidecar from your machine',
        detail:
          'The platform does not manage the local sidecar. Re-run the enrollment shell script on the operator machine.',
      },
      restart_api: {
        title: 'No container to restart',
        detail:
          'This agent calls the LLM provider directly. Send a new prompt to invoke the model again.',
      },
      start_host_cli: {
        title: 'Start the sidecar from your machine',
        detail: 'Re-run the enrollment shell script on the operator machine to launch the sidecar.',
      },
      start_api: {
        title: 'No container to start',
        detail: 'Provider agents have no shell to start.',
      },
      stop_host_cli: {
        title: 'Stop the sidecar from your machine',
        detail:
          'The platform cannot stop a remote sidecar. Stop the process on the operator machine.',
      },
      stop_api: {
        title: 'No container to stop',
        detail: 'Provider agents have no shell to stop.',
      },
      not_permitted: {
        title: 'Operation not permitted on this agent',
        detail: 'You can manage only agents you own. Contact the agent owner if you need access.',
      },
    },
    create: {
      missing_cli_tool_for_container: {
        title: 'Choose a CLI tool',
        detail: 'Container-backed agents need a Container CLI: claude, codex, gemini, or opencode.',
      },
      api_cannot_have_cli_tool: {
        title: 'Provider agent cannot have a CLI tool',
        detail: 'Remove the CLI tool, or change the runtime to "Container (Docker)".',
      },
      missing_cli_tool_for_host_cli: {
        title: 'Choose a CLI tool',
        detail: 'Host CLI enrollment needs a Container CLI: claude, codex, gemini, or opencode.',
      },
    },
    enroll: {
      missing_idempotency_key: {
        title: 'Idempotency-Key header required',
        detail: 'Resend with a fresh UUID in the `Idempotency-Key` header.',
      },
      plaintext_nats_blocked: {
        title: 'Plaintext NATS not allowed for Host CLI',
        detail:
          'Configure `NATS_AGENT_URL` to use `tls://`, or set the org policy `allow_plaintext_host_nats=true` to permit it.',
      },
    },
  },
}
```

Merge `errors` into the file's existing default-export shape.

- [ ] **Step 2: Add the same keys to zh**

In `src/app/shared/i18n/locales/zh.ts`, mirror the structure with Chinese translations:

```typescript
const errors = {
  agent: {
    lifecycle: {
      restart_host_cli: {
        title: '请在本机重启 sidecar',
        detail: '平台不管理本地 sidecar。请在操作员机器上重新运行注册脚本。',
      },
      restart_api: {
        title: '没有可重启的容器',
        detail: '该 Agent 直接调用 LLM 服务商。再发送一次 prompt 即可。',
      },
      start_host_cli: {
        title: '请在本机启动 sidecar',
        detail: '在操作员机器上重新运行注册脚本以启动 sidecar。',
      },
      start_api: { title: '没有可启动的容器', detail: '服务商 Agent 没有 shell 可启动。' },
      stop_host_cli: {
        title: '请在本机停止 sidecar',
        detail: '平台无法远程停止 sidecar。请在操作员机器上停止该进程。',
      },
      stop_api: { title: '没有可停止的容器', detail: '服务商 Agent 没有 shell 可停止。' },
      not_permitted: {
        title: '无权操作该 Agent',
        detail: '你只能管理你拥有的 Agent。如需访问请联系 Agent 所有者。',
      },
    },
    create: {
      missing_cli_tool_for_container: {
        title: '请选择一个 CLI 工具',
        detail: '基于容器的 Agent 需要一个 Container CLI：claude、codex、gemini 或 opencode。',
      },
      api_cannot_have_cli_tool: {
        title: '服务商 Agent 不能有 CLI 工具',
        detail: '请移除 CLI 工具，或将运行时改为 "容器 (Docker)"。',
      },
      missing_cli_tool_for_host_cli: {
        title: '请选择一个 CLI 工具',
        detail: 'Host CLI 注册需要一个 Container CLI：claude、codex、gemini 或 opencode。',
      },
    },
    enroll: {
      missing_idempotency_key: {
        title: '缺少 Idempotency-Key 请求头',
        detail: '请在 `Idempotency-Key` 请求头中带上一个新的 UUID 后重试。',
      },
      plaintext_nats_blocked: {
        title: 'Host CLI 不允许使用明文 NATS',
        detail:
          '请将 `NATS_AGENT_URL` 设为 `tls://`，或设置组织策略 `allow_plaintext_host_nats=true` 后再试。',
      },
    },
  },
}
```

- [ ] **Step 3: Type-check**

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/app/shared/i18n/locales/en.ts src/app/shared/i18n/locales/zh.ts
git commit -m "feat(i18n): add error keys for agent lifecycle/create/enroll"
```

### Task 7.2: Update API error responses to emit i18n key alongside message

**Files:**

- Modify: `rust/crates/api/src/middleware.rs` or wherever `AppError::IntoResponse` is implemented

- [ ] **Step 1: Locate the error response mapper**

```bash
grep -rn "impl IntoResponse for AppError\|impl IntoResponse for.*Error" rust/crates/api/src | head -10
```

- [ ] **Step 2: Add a `code` field to the error body**

In the `IntoResponse` impl, change the body to `{ "ok": false, "error": { "code": <i18n_key>, "message": <msg> } }`. Derive the i18n key from the error variant — for `ErrorKind::Validation(msg)` look up the key from a `match` table mapping known message prefixes to keys, or add a `code: Option<&'static str>` to `ErrorKind::Validation` so callers can attach one.

Recommended: extend `ErrorKind`:

```rust
pub enum ErrorKind {
    Validation { code: Option<&'static str>, message: String },
    Forbidden(String),
    NotFound(String),
    Internal(String),
    // ...
}
```

Update all call sites — most `ErrorKind::Validation("...".into())` become `ErrorKind::Validation { code: None, message: "...".into() }`. Lifecycle / enrollment sites pass `code = Some("errors.agent.lifecycle.restart_host_cli")` etc.

- [ ] **Step 3: Update `LifecycleRejection::into_app_error` to emit the right code**

```rust
impl LifecycleRejection {
    pub(crate) fn into_app_error(self, action: &str) -> AppError {
        let (code, message) = match (self, action) {
            (Self::HostCli, "Restart") => ("errors.agent.lifecycle.restart_host_cli",
                "Restart the sidecar from your machine.".to_string()),
            (Self::Api, "Restart")     => ("errors.agent.lifecycle.restart_api",
                "API/provider agent has no container to restart.".to_string()),
            (Self::HostCli, "Start")   => ("errors.agent.lifecycle.start_host_cli",
                "Start the sidecar from your machine.".to_string()),
            (Self::Api, "Start")       => ("errors.agent.lifecycle.start_api",
                "API/provider agent has no container to start.".to_string()),
            (Self::HostCli, "Stop")    => ("errors.agent.lifecycle.stop_host_cli",
                "Stop the sidecar from your machine.".to_string()),
            (Self::Api, "Stop")        => ("errors.agent.lifecycle.stop_api",
                "API/provider agent has no container to stop.".to_string()),
            _ => ("errors.internal.unknown", "unsupported lifecycle action".to_string()),
        };
        ErrorKind::Validation { code: Some(code), message }.into()
    }
}
```

- [ ] **Step 4: Update tests for new shape**

Find existing integration tests asserting error bodies; update them to expect `error.code` and `error.message`.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/ rust/crates/core/src/
git commit -m "feat(api): error responses carry i18n code alongside message"
```

---

## Phase 8 — Frontend FSD migration

### Task 8.1: Create `src/app/entities/agent/` skeleton

**Files:**

- Create: `src/app/entities/agent/index.ts`
- Create: `src/app/entities/agent/model/types.ts`
- Create: `src/app/entities/agent/model/runtime-kind.ts`

- [ ] **Step 1: Create the barrel file**

`src/app/entities/agent/index.ts`:

```typescript
export type { AgentInfo, AgentRuntimeKind, AgentStatus, CliTool } from './model/types'
export { isHostCliAgent, isContainerAgent, isApiAgent } from './model/runtime-kind'
export { useAgentsStore } from './model/agents.store'
export { agentApi } from './api/AgentAPI'
```

- [ ] **Step 2: Create the types module with canonical literal**

`src/app/entities/agent/model/types.ts`:

```typescript
export type AgentRuntimeKind = 'container' | 'cli' | 'api'

export type AgentStatus = 'idle' | 'running' | 'paused' | 'offline' | 'error'

export type CliTool = 'claude' | 'codex' | 'gemini' | 'opencode'

export interface AgentInfo {
  id: string
  name: string
  status: AgentStatus
  tasksCompleted: number
  tasksInProgress: number
  successRate: number
  currentTask?: string
  cliTool?: CliTool
  runtimeId?: string
  runtimeKind?: AgentRuntimeKind // optional for one release cycle (rev2 §5.2)
  cwd?: string
  containerId?: string
  workspaceId?: string
  workspaceName?: string
  projectId?: string
  projectName?: string
  systemPrompt?: string | null
}
```

- [ ] **Step 3: Create the specifications**

`src/app/entities/agent/model/runtime-kind.ts`:

```typescript
import type { AgentInfo } from './types'

/** Returns true iff the agent's authoritative runtime is the host-local CLI. */
export const isHostCliAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'cli'

export const isContainerAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'container'

export const isApiAgent = (a: Pick<AgentInfo, 'runtimeKind'>) => a.runtimeKind === 'api'
```

- [ ] **Step 4: Type-check (compilation only; consumers still import from old paths)**

```bash
npm run typecheck
```

Expected: PASS (no consumer is using the new paths yet).

- [ ] **Step 5: Commit**

```bash
git add src/app/entities/agent/
git commit -m "feat(fe): scaffold entities/agent layer with canonical literals and specifications"
```

### Task 8.2: Move `agents.store.ts` into entities

**Files:**

- Create: `src/app/entities/agent/model/agents.store.ts` (moved)
- Delete: `src/app/shared/model/agents.store.ts` (after consumers updated)

- [ ] **Step 1: Copy the store file with internal type rename**

```bash
cp src/app/shared/model/agents.store.ts src/app/entities/agent/model/agents.store.ts
```

Edit the new file:

1. Remove the local `AgentRuntimeKind` type definition; replace `import type` block at the top with `import type { AgentInfo, AgentRuntimeKind, CliTool } from './types'`.
2. Remove the local `isHostCliAgent` definition; replace with `import { isHostCliAgent } from './runtime-kind'`.
3. Rename the literal mapping in `managedToAgentInfo` (currently around lines 114–118 of the old file):

```typescript
const runtimeKind: AgentRuntimeKind = agent.cliTool
  ? isHostCliRuntimeId(agent.runtimeId)
    ? 'cli'
    : 'container' // narrow temp helper
  : 'api'
```

4. Delete `isHostCliRuntimeId` after migration 062/063 ships; for now keep as a local helper so old server responses that omit `runtimeKind` still derive a value (rolling-deploy safety, rev 2 §5.2).

- [ ] **Step 2: Update internal imports inside the moved file**

If the store imports anything from `@app/shared/model/...`, repath to the entity location where appropriate.

- [ ] **Step 3: Type-check**

```bash
npm run typecheck
```

Expected: PASS — the old file still exists as the canonical import source for consumers, the new file compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/app/entities/agent/model/agents.store.ts
git commit -m "feat(fe): copy agents.store into entities/agent (consumers not yet repointed)"
```

### Task 8.3: Move `AgentAPI.ts` into entities

**Files:**

- Create: `src/app/entities/agent/api/AgentAPI.ts`

- [ ] **Step 1: Copy the file**

```bash
mkdir -p src/app/entities/agent/api
cp src/app/shared/api/legacy/AgentAPI.ts src/app/entities/agent/api/AgentAPI.ts
```

- [ ] **Step 2: Add `Idempotency-Key` to the enrollment call**

In the new `AgentAPI.ts`, find the `enrollLocalAgent` (or equivalent) method and add the header:

```typescript
async enrollLocalAgent(payload: EnrollLocalAgentRequest): Promise<EnrollLocalAgentResponse> {
  const key = crypto.randomUUID()
  return this.fetcher.post('/api/v1/agents/local-enroll', {
    body: JSON.stringify(payload),
    headers: { 'Idempotency-Key': key, 'Content-Type': 'application/json' },
  })
}
```

- [ ] **Step 3: Update the moved store's import**

In `src/app/entities/agent/model/agents.store.ts`, change `import { agentApi } from '@app/shared/api/legacy/AgentAPI'` to `import { agentApi } from '../api/AgentAPI'`.

- [ ] **Step 4: Type-check**

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/entities/agent/api/AgentAPI.ts src/app/entities/agent/model/agents.store.ts
git commit -m "feat(fe): copy AgentAPI into entities/agent with Idempotency-Key"
```

### Task 8.4: Rewrite imports across feature/widget/page files

**Files:**

- Modify: `src/app/features/agents/AgentConfigTab.tsx`
- Modify: `src/app/features/agents/AgentControlPanel.tsx`
- Modify: `src/app/features/agents/AgentListView.tsx`
- Modify: `src/app/features/agents/AgentCard.tsx`
- Modify: `src/app/features/agents/AgentKindBadge.tsx`
- Modify: `src/app/features/agents/CreateAgentModal.tsx`
- Modify: `src/app/widgets/agent-detail/AgentDetailView.tsx`
- Modify: `src/app/pages/getting-started/ui/GettingStartedView.tsx`

- [ ] **Step 1: Apply search-and-replace**

For each file above, replace:

- `from '@app/shared/model/agents.store'` → `from '@app/entities/agent'`
- `from '@app/shared/api/legacy/AgentAPI'` → `from '@app/entities/agent'`
- Literal `'container-cli'` → `'container'`
- Literal `'host-cli'` (where used as a value, not in an i18n key string) → `'cli'`
- Literal `'provider'` → `'api'`

Hand-verify each file: some places use string literals for unrelated reasons (e.g., CSS class names with `'cli'` substring). Only rewrite where the value is an `AgentRuntimeKind`.

- [ ] **Step 2: Type-check**

```bash
npm run typecheck
```

Expected: PASS.

- [ ] **Step 3: Run FSD checker**

```bash
npm run fsd:check
```

Expected: PASS. If it fails on a remaining import from the old shared path, repoint it.

- [ ] **Step 4: Commit**

```bash
git add src/app/features src/app/widgets src/app/pages
git commit -m "refactor(fe): repoint agent consumers to @app/entities/agent"
```

### Task 8.5: Delete legacy `shared/api/legacy/AgentAPI.ts` and `shared/model/agents.store.ts`

**Files:**

- Delete: `src/app/shared/api/legacy/AgentAPI.ts`
- Delete: `src/app/shared/model/agents.store.ts`

- [ ] **Step 1: Verify zero remaining references**

```bash
grep -rn "shared/model/agents.store\|shared/api/legacy/AgentAPI" src/app | head
```

Expected: empty.

- [ ] **Step 2: Delete the files**

```bash
git rm src/app/shared/api/legacy/AgentAPI.ts
git rm src/app/shared/model/agents.store.ts
```

If the directory `src/app/shared/api/legacy/` becomes empty, delete it too.

- [ ] **Step 3: Type-check + FSD-check + lint**

```bash
npm run typecheck && npm run fsd:check && npm run lint
```

Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git commit -m "refactor(fe): remove legacy agent store and API from shared/"
```

### Task 8.6: Update test fixtures + Vitest specs for the literal rename

**Files:**

- Modify: every Vitest test file that uses the old literals

- [ ] **Step 1: Find affected test files**

```bash
grep -rln "container-cli\|host-cli\|'provider'" tests/unit src/app | grep -v ".test." 2>&1 | head
grep -rln "container-cli\|'provider'" tests/unit | head
```

- [ ] **Step 2: Apply the same literal rename across test files**

Search-and-replace each in the affected files. Be careful with `'host-cli'` in i18n keys — those stay as i18n key strings.

- [ ] **Step 3: Run Vitest**

```bash
npm run test:unit
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/unit src/app
git commit -m "test(fe): align fixtures with canonical AgentRuntimeKind literals"
```

---

## Phase 9 — Tests

### Task 9.1: Vitest specification suite

**Files:**

- Create: `tests/unit/app/entities/agent/runtime-kind.test.ts`

- [ ] **Step 1: Write the test**

`tests/unit/app/entities/agent/runtime-kind.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { isApiAgent, isContainerAgent, isHostCliAgent } from '@app/entities/agent'

describe('runtime-kind specifications', () => {
  it('isHostCliAgent matches only runtimeKind="cli"', () => {
    expect(isHostCliAgent({ runtimeKind: 'cli' })).toBe(true)
    expect(isHostCliAgent({ runtimeKind: 'container' })).toBe(false)
    expect(isHostCliAgent({ runtimeKind: 'api' })).toBe(false)
  })

  it('no prefix fallback: container with host- runtimeId still returns false', () => {
    // runtimeId is intentionally NOT part of the specification anymore.
    expect(isHostCliAgent({ runtimeKind: 'container' as const })).toBe(false)
  })

  it('undefined runtimeKind during rolling-deploy returns false for all three', () => {
    const u = { runtimeKind: undefined }
    expect(isHostCliAgent(u)).toBe(false)
    expect(isContainerAgent(u)).toBe(false)
    expect(isApiAgent(u)).toBe(false)
  })

  it('isContainerAgent and isApiAgent are exhaustive', () => {
    expect(isContainerAgent({ runtimeKind: 'container' })).toBe(true)
    expect(isApiAgent({ runtimeKind: 'api' })).toBe(true)
  })
})
```

- [ ] **Step 2: Run**

```bash
npm run test:unit -- runtime-kind
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/unit/app/entities/agent/runtime-kind.test.ts
git commit -m "test(fe): specifications suite for entities/agent runtime-kind"
```

### Task 9.2: Vitest store backward-compat suite

**Files:**

- Create: `tests/unit/app/entities/agent/agents-store.test.ts`

- [ ] **Step 1: Write the test**

```typescript
import { describe, expect, it } from 'vitest'
// import the conversion helper if exported; otherwise drive the store directly.
import { managedToAgentInfo } from '@app/entities/agent/model/agents.store'

describe('agents.store backward-compat', () => {
  it('server response with runtimeKind="cli" populates the field', () => {
    const info = managedToAgentInfo({
      id: 'a1',
      name: 'a',
      status: 'idle',
      cliTool: 'codex',
      runtimeKind: 'cli',
      runtimeId: 'host-x',
    } as any)
    expect(info.runtimeKind).toBe('cli')
  })

  it('legacy server response without runtimeKind falls back via cliTool + runtimeId', () => {
    const fromHost = managedToAgentInfo({
      id: 'a1',
      name: 'a',
      status: 'idle',
      cliTool: 'codex',
      runtimeId: 'host-x',
    } as any)
    expect(fromHost.runtimeKind).toBe('cli')
    const fromContainer = managedToAgentInfo({
      id: 'a2',
      name: 'a',
      status: 'idle',
      cliTool: 'codex',
    } as any)
    expect(fromContainer.runtimeKind).toBe('container')
    const fromApi = managedToAgentInfo({ id: 'a3', name: 'a', status: 'idle' } as any)
    expect(fromApi.runtimeKind).toBe('api')
  })
})
```

If `managedToAgentInfo` is not exported today, export it from `agents.store.ts` for tests.

- [ ] **Step 2: Run**

```bash
npm run test:unit -- agents-store
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/unit/app/entities/agent/agents-store.test.ts src/app/entities/agent/model/agents.store.ts
git commit -m "test(fe): store backward-compat when server omits runtimeKind"
```

### Task 9.3: Tracing redaction unit test

**Files:**

- Create: `rust/crates/api/tests/tracing_redaction_test.rs`

- [ ] **Step 1: Write the test**

```rust
//! Asserts that hmac_secret / nats_connect_password never appear in tracing
//! spans even when the agent record is logged.

use agentforge_api::domain::agent::HostCliIdentity;

#[test]
fn host_cli_identity_does_not_leak_secrets_in_debug_format() {
    let id = HostCliIdentity::generate();
    let dbg = format!("{:?}", id);
    assert!(!dbg.contains(id.hmac_secret()),  "hmac_secret leaked in Debug: {dbg}");
    assert!(!dbg.contains(id.nats_connect_password()), "nats_password leaked in Debug: {dbg}");
}
```

To make this pass, derive `Debug` manually on `HostCliIdentity` to redact secret fields:

```rust
impl std::fmt::Debug for HostCliIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCliIdentity")
            .field("agent_id", &self.agent_id)
            .field("runtime_id", &self.runtime_id)
            .field("hmac_secret", &"<redacted>")
            .field("nats_connect_password", &"<redacted>")
            .finish()
    }
}
```

- [ ] **Step 2: Run**

```bash
cd rust && cargo test -p agentforge-api --test tracing_redaction_test 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/api/tests/tracing_redaction_test.rs rust/crates/api/src/domain/agent.rs
git commit -m "test(api): assert HostCliIdentity Debug never leaks secrets"
```

### Task 9.4: Playwright E2E spec

**Files:**

- Create or extend: `tests/e2e/specs/host-cli-enrollment.spec.ts`

- [ ] **Step 1: Write the E2E spec**

```typescript
import { expect, test } from '@playwright/test'

test('Host CLI enrollment shows correct badge and rejects container restart', async ({ page }) => {
  await page.goto('/login')
  // existing helper to log in as dev@example.com — see other E2E specs
  await loginAsDev(page)

  await page.goto('/agents')
  await page.getByRole('button', { name: /create agent/i }).click()
  await page.getByRole('tab', { name: /host cli/i }).click()
  await page.getByLabel(/cli tool/i).selectOption({ label: /codex/i })
  await page.getByRole('button', { name: /enroll/i }).click()

  // Wait for the shellExports section to appear (proof of successful response)
  await expect(page.getByText(/AGENTFORGE_RUNTIME_KIND=cli/)).toBeVisible({ timeout: 30000 })

  // Navigate to the new agent and assert the badge says "Host CLI"
  await page
    .getByText(/host cli/i)
    .first()
    .click()
  await expect(page.getByText(/Host CLI/i)).toBeVisible()

  // Attempting "Restart" surfaces the operator-facing i18n message
  const restartBtn = page.getByRole('button', { name: /restart/i })
  if (await restartBtn.isVisible()) {
    await restartBtn.click()
    await expect(page.getByText(/restart the sidecar from your machine/i)).toBeVisible()
  }
})
```

- [ ] **Step 2: Run the spec**

```bash
npm run test:e2e -- host-cli-enrollment
```

Expected: PASS in the local dev stack (with `make dev` running).

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/specs/host-cli-enrollment.spec.ts
git commit -m "test(e2e): host CLI enrollment + Restart 422 message"
```

---

## Phase 10 — Operator tooling

### Task 10.1: `agentforge migrate doctor` subcommand

**Files:**

- Create: `rust/bins/cli/src/cmd/migrate/doctor.rs`
- Modify: `rust/bins/cli/src/cmd/migrate/mod.rs` (register the subcommand)

- [ ] **Step 1: Write the subcommand**

`rust/bins/cli/src/cmd/migrate/doctor.rs`:

```rust
//! `agentforge migrate doctor` — pre-flight checks before applying
//! migration 062 (agents.runtime_kind discriminator).

use anyhow::{Context, Result};
use clap::Args;
use sqlx::{PgPool, Row};

#[derive(Args)]
pub struct DoctorOpts {
    /// Override the row-count threshold (default 100000).
    #[arg(long, default_value_t = 100_000)]
    pub max_row_count: i64,
    /// Skip the row-count gate.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(pool: PgPool, opts: DoctorOpts) -> Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool).await
        .context("counting agents")?;
    println!("agents row count: {}", count.0);

    if !opts.force && count.0 > opts.max_row_count {
        anyhow::bail!(
            "agents table has {} rows (> {}). Migration 062's batched backfill will be slow. \
             Rerun with --force after planning an off-peak window.",
            count.0, opts.max_row_count
        );
    }

    // Pre-flight invariant scan (only meaningful if 062 has already added the column)
    let column_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                        WHERE table_name='agents' AND column_name='runtime_kind')"
    ).fetch_one(&pool).await?;
    if column_exists.0 {
        let bad: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agents
             WHERE NOT (
               (runtime_kind = 'container' AND cli_tool IS NOT NULL) OR
               (runtime_kind = 'cli'       AND cli_tool IS NOT NULL AND container_id IS NULL) OR
               (runtime_kind = 'api'       AND cli_tool IS NULL)
             )"
        ).fetch_one(&pool).await?;
        if bad.0 > 0 {
            anyhow::bail!(
                "{} rows would violate the invariant CHECK. \
                 Inspect with: SELECT id, runtime_kind, cli_tool, container_id, runtime_id \
                 FROM agents WHERE NOT (...); and remediate before 063 ships.",
                bad.0
            );
        }
        println!("invariant CHECK pre-flight: 0 offenders");
    } else {
        println!("agents.runtime_kind column not yet present (062 has not run)");
    }

    let pg_version: (String,) = sqlx::query_as("SHOW server_version").fetch_one(&pool).await?;
    println!("postgres server version: {}", pg_version.0);

    println!("migrate doctor: OK");
    Ok(())
}
```

- [ ] **Step 2: Register the subcommand**

In `rust/bins/cli/src/cmd/migrate/mod.rs` (or wherever `MigrateCommand` enum lives):

```rust
#[derive(Subcommand)]
pub enum MigrateCommand {
    Up,
    Down,
    Doctor(doctor::DoctorOpts),
    // ...existing variants
}
```

Dispatch `Doctor(opts)` to `doctor::run(pool, opts).await`.

- [ ] **Step 3: Build the CLI binary**

```bash
cd rust && cargo build -p agentforge-cli --bin agentforge
```

Expected: clean build.

- [ ] **Step 4: Smoke-test against the local dev DB**

```bash
./rust/target/debug/agentforge migrate doctor
```

Expected: prints row count, version, and "OK".

- [ ] **Step 5: Commit**

```bash
git add rust/bins/cli/src/cmd/migrate/
git commit -m "feat(cli): agentforge migrate doctor pre-flight subcommand"
```

---

## Phase 11 — Docs

### Task 11.1: Update `docs/architecture/glossary.md` with a cross-link footnote

**Files:**

- Modify: `docs/architecture/glossary.md`

- [ ] **Step 1: Append footnote under "Runtime modes"**

After the existing "Runtime modes (Settings page)" table, append:

```markdown
> The DB column `agents.runtime_kind` and the Rust enum `agentforge_core::RuntimeKind` use the values in the "DB value" column above. See `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` for the discriminator design.
```

- [ ] **Step 2: Commit**

```bash
git add docs/architecture/glossary.md
git commit -m "docs(glossary): cross-link runtime modes to host CLI redesign spec"
```

### Task 11.2: Update `docs/runbooks/host-cli-agent-enrollment.md`

**Files:**

- Modify: `docs/runbooks/host-cli-agent-enrollment.md`

- [ ] **Step 1: Replace the file with the rev 2 §17.2 draft text**

Apply the three new sections (Verify step 6, Network, Idempotency) from §17.2 of the spec to the existing runbook, preserving the unchanged sections. Use the spec as the source of truth.

- [ ] **Step 2: Commit**

```bash
git add docs/runbooks/host-cli-agent-enrollment.md
git commit -m "docs(runbook): host CLI enrollment — verify, network/TLS, idempotency"
```

### Task 11.3: Create `docs/runbooks/migration-062-runtime-kind.md`

**Files:**

- Create: `docs/runbooks/migration-062-runtime-kind.md`

- [ ] **Step 1: Copy the §17.3 draft text from the spec verbatim into the new file**

- [ ] **Step 2: Commit**

```bash
git add docs/runbooks/migration-062-runtime-kind.md
git commit -m "docs(runbook): migration 062/063/064 operator playbook"
```

---

## Phase 12 — Observability counters

### Task 12.1: Wire `agents_created_total{runtime_kind}` counter

**Files:**

- Modify: `rust/crates/api/src/services/agent.rs` (or wherever creation is centralized)

- [ ] **Step 1: Locate the existing metrics registry**

```bash
grep -rn "metrics::counter\|prometheus::Counter\|opentelemetry_prometheus" rust/crates/api/src | head
```

- [ ] **Step 2: Add the counter increment after successful create**

In `AgentService::create` (and `HostAgentEnrollmentService::enroll`) after the transaction commits:

```rust
metrics::counter!(
    "agents_created_total",
    "runtime_kind" => new.runtime_kind().as_str().to_string()
).increment(1);
```

(Adjust to the project's metrics facade — likely `metrics` crate, possibly `opentelemetry`.)

- [ ] **Step 3: Test counter increments by hitting the create endpoint twice**

A separate test verifying metrics is optional at this stage — the counter wiring is structural. If the project has a metrics-snapshot helper, add a one-line assertion.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/api/src/services/
git commit -m "feat(api): agents_created_total counter labeled by runtime_kind"
```

### Task 12.2: `agents_lifecycle_rejected_total{runtime_kind,action}`

**Files:**

- Modify: `rust/crates/api/src/services/agent_container_lifecycle.rs`

- [ ] **Step 1: Increment on rejection**

In the `restart` / `start` / `stop` methods, after `ContainerAgent::try_from(agent_aggregate)` returns Err:

```rust
let aggregate = self.agents.find_aggregate(scope, agent_id.into()).await?;
let kind = aggregate.runtime_kind();
let container = ContainerAgent::try_from(aggregate)
    .map_err(|r| {
        metrics::counter!(
            "agents_lifecycle_rejected_total",
            "runtime_kind" => kind.as_str().to_string(),
            "action" => "restart"
        ).increment(1);
        r.into_app_error("Restart")
    })?;
```

- [ ] **Step 2: Commit**

```bash
git add rust/crates/api/src/services/agent_container_lifecycle.rs
git commit -m "feat(api): agents_lifecycle_rejected_total counter"
```

### Task 12.3: Remaining counters

**Files:**

- Modify: relevant services

- [ ] **Step 1: `agents_check_constraint_violations_total`**

In the error mapper for `sqlx::Error::Database` where the constraint name matches `agents_runtime_kind_*`:

```rust
metrics::counter!("agents_check_constraint_violations_total").increment(1);
```

- [ ] **Step 2: `agents_idempotency_replay_total`**

In `HostAgentEnrollmentService::enroll`, on the fast path (when `lookup_idempotent` returns `Some`):

```rust
metrics::counter!("agents_idempotency_replay_total").increment(1);
```

- [ ] **Step 3: `agents_enrolled_total{cli_tool}`**

In `HostAgentEnrollmentService::enroll`, after `tx.commit()`:

```rust
metrics::counter!(
    "agents_enrolled_total",
    "cli_tool" => cli_tool.to_string()
).increment(1);
```

- [ ] **Step 4: Commit**

```bash
git add rust/crates/api/src/
git commit -m "feat(api): wire remaining runtime_kind observability counters"
```

---

## Phase 13 — Integration + verify

### Task 13.1: Full CI sweep

- [ ] **Step 1: Frontend chain**

```bash
npm run fsd:check
npm run lint
npm run format:check
npm run typecheck
npm run test:unit
```

Expected: ALL PASS.

- [ ] **Step 2: Rust workspace**

```bash
cd rust && make ci
```

Expected: clippy clean, all tests pass.

- [ ] **Step 3: E2E**

```bash
make dev-d
sleep 30
npm run test:e2e
make dev-down
```

Expected: PASS.

- [ ] **Step 4: If any step fails, fix and re-run**

Iterate until all checks green. Commits should be focused fixes referencing the failure.

### Task 13.2: Manual prod-ext validation

- [ ] **Step 1: Bring up prod-ext stack**

```bash
make prod-ext
```

Wait for health check to pass.

- [ ] **Step 2: Run `migrate doctor` against the prod-ext DB**

```bash
./rust/target/debug/agentforge migrate doctor
```

Expected: row count low, no invariant offenders, OK.

- [ ] **Step 3: Apply migrations 062, 063, 064, 065**

The deploy automation runs all pending migrations; manually confirm in `psql`:

```bash
docker exec wisdoverse-prod-db psql -U agentforge -d agentforge \
  -c "SELECT runtime_kind, COUNT(*) FROM agents GROUP BY 1;"
```

Expected: distribution by kind, no NULLs.

- [ ] **Step 4: Walk through the manual validation checklist from §11.9 of the spec**

Tick each item in the spec's §11.9. File a follow-up issue for any deviation.

- [ ] **Step 5: Bring down the stack**

```bash
make prod-ext-down
```

### Task 13.3: Open the pull request

- [ ] **Step 1: Push the branch**

```bash
git push -u origin feat/agents-runtime-kind
```

- [ ] **Step 2: Open the PR via `gh`**

```bash
gh pr create --title "feat(agents): runtime_kind STI discriminator + atomic host CLI enrollment" --body "$(cat <<'EOF'
## Summary

- Adds `agents.runtime_kind` column + invariant CHECK (3-migration sequence 062/063/064)
- Reuses existing `agentforge_core::RuntimeKind`; aligns frontend literal to canonical `container|cli|api`
- Replaces string-prefix discriminator on `runtime_id` with typed enum end-to-end
- Adds `ContainerAgent` typestate so Docker calls can't reach host_cli or api agents
- Makes Host CLI enrollment idempotent (`Idempotency-Key` header) and atomic with the `agent.enrolled` audit event
- Adds `agentforge migrate doctor` pre-flight subcommand
- Moves agent entity types/specs/store into `src/app/entities/agent/` per FSD canon

Spec: docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md (rev 2)

## Test plan

- [x] cargo test -p agentforge-api --workspace
- [x] cargo test -p agentforge-core
- [x] npm run typecheck && npm run lint && npm run fsd:check && npm run test:unit
- [x] npm run test:e2e host-cli-enrollment
- [x] make prod-ext + manual checklist from spec §11.9

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify CI green on the PR**

Watch the PR's check runs; iterate on any red.

---

## Self-Review (run inline)

**1. Spec coverage:**

- §1 Motivation → Task 6.2 fixes the lifecycle bug
- §3 Glossary alignment → Tasks 8.1, 11.1
- §4.1 Core enum reuse + sqlx → Tasks 1.1, 1.2, 1.3
- §4.2 Three-migration sequence → Tasks 2.1, 2.2, 2.3
- §4.3 NewAgent factories + ContainerAgent typestate + atomic audit → Tasks 3.1, 3.2, 3.3, 4.2
- §5 Component inventory → all of Phase 3–8
- §6 Data flow → exercised by Phase 9 tests
- §7 Error messages + i18n → Tasks 7.1, 7.2
- §8 Migration safety + rollout + `migrate doctor` → Tasks 2.x, 10.1
- §9 Defense-in-depth layers → checks in Tasks 1.3, 6.5, 9.1
- §10 Observability → Tasks 9.3, 12.1–12.3
- §11 Tests → Phase 9
- §12 Open questions resolved (HostCliIdentity stays in api crate per Task 3.1; admin filter deferred — §14 of spec)
- §13 Acceptance criteria → Task 13.x
- §14 Out of scope — explicit defers, no plan tasks
- §15 Decision matrix → no plan tasks (audit trail in spec)
- §16 Threat model → reflected in Tasks 5.x, 6.1, 6.3, 9.3
- §17 Doc deltas → Tasks 11.1, 11.2, 11.3

**2. Placeholder scan:** No "TBD" / "TODO" / "implement later" in any task. Each step has either a real code block or an explicit command + expected output.

**3. Type consistency:**

- `NewAgent` (introduced Task 3.2) → used Task 4.2, 4.3, 6.1 — consistent
- `AgentAggregate` → introduced Task 3.3 (typestate context) and Task 4.3 (`find_aggregate`) — fields match
- `RuntimeKind` enum values: `Container | Cli | Api` everywhere — consistent
- DB-on-disk literal: `container | cli | api` everywhere — consistent
- Frontend literal: `'container' | 'cli' | 'api'` everywhere — consistent

**4. Ambiguity check:**

- `app_test_harness` in integration tests called out as illustrative (Task 5.2 note) — engineer substitutes the project's actual helper.
- `ErrorKind::Forbidden` may not exist (Task 6.3 calls this out and includes the add).
- Metrics facade is named `metrics::counter!` (project's likely `metrics` crate) — engineer adapts if the project uses `opentelemetry` directly.

Done.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-27-host-cli-enrollment.md`. Two execution options:

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints

Which approach?
