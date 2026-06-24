# Org-scoped Control-Plane Snapshot in the Admin Panel (#805) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the orchestration "is a loop wedged" signals (already computed by `OrchestrationMetricsWorker` as global Prometheus gauges) as an org-scoped, admin-gated `GET /api/v1/admin/control-plane` endpoint rendered in a new admin health panel, so operators without Prometheus can read their own org's control-plane health.

**Architecture:** Mirror the existing `GET /admin/stats` vertical exactly — `route (routes/admin.rs) -> AdminService (services/admin.rs) -> AdminRepository (repositories/admin.rs) -> domain projection (domain/admin.rs)`. The repository runs 6 org-scoped SQL counts (the worker's checks, each constrained by `WHERE organization_id = $1`). Frontend mirrors the `SystemHealth` panel: a Zustand store action polling the endpoint every 30s with a visibility guard and defensive parsing.

**Tech Stack:** Rust (axum, sqlx, serde), React + TypeScript + Zustand, Vitest.

## Global Constraints

- This is a public repo: no private hostnames/emails/URLs in any artifact; use `dev@example.com` / `example.com` placeholders only.
- Tenant isolation is load-bearing: every new SQL query MUST include `WHERE organization_id = $1` bound to `scope.org_id().as_uuid()`. A query missing it bypasses auth.
- `job_queue` has **no** `organization_id` column. The worker's `job_queue_pending/running/dead/oldest_pending_age_seconds` gauges are platform-global and MUST NOT appear in this org-scoped snapshot (a per-org queue depth would be a lie). Document the omission; do not fake it.
- Backend DDD: domain owns the `Serialize` projection; the service owns repo I/O; the route consumes the domain type via the service. Production route AND service code must NOT construct `agentforge_core::ErrorKind` (the `route_ddd_boundary_test` greps for it) and routes must NOT read `state.config`.
- Admin endpoints return the `{ "ok": true, "data": <...> }` envelope via `admin_data_response(...)`. All admin routes call `AdminService::require_admin(&auth.role)?` first.
- Frontend FSD: the admin store + its types live in `src/app/shared/model`; the panel is a `src/app/features/admin` component. Imports point downward only (`features -> shared`); never import `entities` from `shared`. Run `npm run fsd:check` for any frontend change.
- Rust<->TS contract is hand-synced (no proto): Rust struct uses `#[serde(rename_all = "camelCase")]`; the TS interface must use the exact camelCase keys.
- Staleness threshold: reuse `agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER` (the `api` crate already depends on `agentforge-jobs`) so the panel's participant-staleness threshold matches the worker. Do not hardcode a divergent number.

## Source-of-truth references (read, do not modify)

- `rust/crates/jobs/src/orchestration_metrics.rs` — `collect_control_plane_snapshot` + the 10 global checks (the 6 org-scopable ones are reproduced below with the org filter added).
- `rust/crates/api/src/routes/admin.rs` — `get_stats` handler (lines ~173-177) + `admin_routes()` registration (lines ~350-364) + `make_service` (line ~60).
- `rust/crates/api/src/services/admin.rs` — `stats()` (line ~253), `require_admin` (line ~347), `require_admin` tests (lines ~391-411).
- `rust/crates/api/src/repositories/admin.rs` — `stats()` (line ~480) is the exact method shape to mirror; `new(pool)` (line ~122); seed helpers + `#[sqlx::test(migrations = "../db/migrations")]` (lines ~610-674).
- `rust/crates/api/src/domain/admin.rs` — projection structs with `#[serde(rename_all = "camelCase")]` (e.g. `AdminOrgProjection` ~line 262); `admin_data_response` (~line 272).
- `src/app/shared/model/admin.store.ts` — `AdminSection` (line 8), `AdminState` (lines ~172-279), `loadHealth` (~649), `adminFetch` (~296).
- `src/app/features/admin/SystemHealth.tsx` — the panel to mirror (30s poll, `document.visibilityState` guard, defensive parse).
- `src/app/features/admin/AdminLayout.tsx` — section nav + `SectionContent` routing.

## File Structure

- Backend (all in `rust/crates/api/src/`):
  - `domain/admin.rs` — add `OrgControlPlaneSnapshot` struct + a serde test (Modify).
  - `repositories/admin.rs` — add 6 SQL consts + `org_control_plane_snapshot(...)` + a `#[sqlx::test]` tenant-isolation test (Modify).
  - `services/admin.rs` — add `org_control_plane_snapshot(...)` delegating to the repo (Modify).
  - `routes/admin.rs` — add `get_control_plane` handler + register `/admin/control-plane` (Modify).
- Frontend (all under `src/app/`):
  - `shared/model/admin.store.ts` — add `OrgControlPlaneSnapshot` TS interface, extend `AdminSection`, add `controlPlane*` state + `loadControlPlane` action (Modify).
  - `features/admin/ControlPlanePanel.tsx` — new panel mirroring `SystemHealth.tsx` (Create).
  - `features/admin/controlPlaneErrorMessage.ts` — HTTP-status → operator copy (Create).
  - `features/admin/AdminLayout.tsx` — add nav entry + `SectionContent` route (Modify).
  - `tests/unit/app/ControlPlanePanel.test.tsx` — Vitest render/defensive-parse test (Create).

---

### Task 1: Backend — org-scoped control-plane endpoint (domain + repo + service + route + tests)

**Files:**

- Modify: `rust/crates/api/src/domain/admin.rs`
- Modify: `rust/crates/api/src/repositories/admin.rs`
- Modify: `rust/crates/api/src/services/admin.rs`
- Modify: `rust/crates/api/src/routes/admin.rs`

**Interfaces:**

- Produces (later tasks / frontend rely on the JSON shape): `GET /api/v1/admin/control-plane` → `{ ok: true, data: { assignmentOutboxBacklog, assignmentOutboxOldestAgeSeconds, staleParticipants, expiredWorkingLeases, busyParticipantsWithoutWork, workingTasksWithoutBusyParticipant, staleAfterSeconds } }`.
- Consumes: `TenantScope::org_id() -> OrgId` (`.as_uuid()` to bind), `AdminService::require_admin`, `admin_data_response`, `agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER`.

- [ ] **Step 1: Add the domain projection struct in `domain/admin.rs`**

Place near the other projection structs (after `AdminOrgProjection`). The struct is `pub(crate)` (same crate consumes it; the route serializes it generically).

```rust
/// Org-scoped orchestration control-plane health snapshot for the admin panel.
///
/// Reproduces the "is a loop wedged" signals the `OrchestrationMetricsWorker`
/// emits as GLOBAL Prometheus gauges (`jobs/src/orchestration_metrics.rs`), but
/// scoped to one organization via `WHERE organization_id = $1`, so operators
/// without a Prometheus stack can read their own org's health.
///
/// NOTE: the worker also emits `job_queue_*` depth gauges. `job_queue` has no
/// `organization_id` column, so those are platform-global and are intentionally
/// NOT represented here — a per-org queue depth would be a lie.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrgControlPlaneSnapshot {
    /// Unpublished `assignment` outbox events for this org (relay backlog).
    pub(crate) assignment_outbox_backlog: i64,
    /// Age (seconds) of the oldest unpublished `assignment` outbox event; 0.0 when none.
    pub(crate) assignment_outbox_oldest_age_seconds: f64,
    /// Non-offline participants in this org with no recent heartbeat.
    pub(crate) stale_participants: i64,
    /// `working` tasks in this org whose lease has expired.
    pub(crate) expired_working_leases: i64,
    /// `busy` participants in this org with no matching `working` task.
    pub(crate) busy_participants_without_work: i64,
    /// `working` tasks in this org whose assigned agent is not `busy`.
    pub(crate) working_tasks_without_busy_participant: i64,
    /// The participant-staleness threshold (seconds) used for `stale_participants`.
    pub(crate) stale_after_seconds: i64,
}
```

- [ ] **Step 2: Add a serde-shape unit test in `domain/admin.rs`**

In the existing `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn org_control_plane_snapshot_serializes_camel_case() {
    let snapshot = OrgControlPlaneSnapshot {
        assignment_outbox_backlog: 3,
        assignment_outbox_oldest_age_seconds: 12.5,
        stale_participants: 1,
        expired_working_leases: 2,
        busy_participants_without_work: 0,
        working_tasks_without_busy_participant: 4,
        stale_after_seconds: 90,
    };
    let value = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(value["assignmentOutboxBacklog"], 3);
    assert_eq!(value["assignmentOutboxOldestAgeSeconds"], 12.5);
    assert_eq!(value["staleParticipants"], 1);
    assert_eq!(value["expiredWorkingLeases"], 2);
    assert_eq!(value["busyParticipantsWithoutWork"], 0);
    assert_eq!(value["workingTasksWithoutBusyParticipant"], 4);
    assert_eq!(value["staleAfterSeconds"], 90);
    // No snake_case leakage and no global job_queue fields.
    assert!(value.get("assignment_outbox_backlog").is_none());
    assert!(value.get("jobQueuePending").is_none());
}
```

- [ ] **Step 3: Run the domain test (expected fail → then pass after Step 1 compiles)**

Run: `cd rust && cargo test -p agentforge-api --lib org_control_plane_snapshot_serializes_camel_case`
Expected: PASS (struct + test compile together).

- [ ] **Step 4: Add the 6 org-scoped SQL consts + repo method in `repositories/admin.rs`**

Import the domain type at the top of the file alongside the other `domain::admin` imports (e.g. `use crate::domain::admin::{... , OrgControlPlaneSnapshot};` — match the existing import path/style in this file). Add the SQL consts near the top-level consts (e.g. after `ADMIN_AGENT_SELECT`). Each is the worker's check with `WHERE organization_id = $1` added. `$2` on the participants query is `stale_after_secs` as `i32`.

```rust
const ORG_OUTBOX_BACKLOG_SQL: &str = "SELECT COUNT(*) FROM orchestration_outbox \
    WHERE organization_id = $1 AND published_at IS NULL AND event_type = 'assignment'";

const ORG_OUTBOX_OLDEST_AGE_SQL: &str = "SELECT COALESCE(CAST(EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) AS DOUBLE PRECISION), 0.0) \
    FROM orchestration_outbox WHERE organization_id = $1 AND published_at IS NULL AND event_type = 'assignment'";

const ORG_STALE_PARTICIPANTS_SQL: &str = "SELECT COUNT(*) FROM participants \
    WHERE organization_id = $1 AND status <> 'offline' \
    AND (last_heartbeat_at IS NULL OR last_heartbeat_at < NOW() - ($2::int * INTERVAL '1 second'))";

const ORG_EXPIRED_LEASES_SQL: &str = "SELECT COUNT(*) FROM orchestration_tasks \
    WHERE organization_id = $1 AND status = 'working' \
    AND lease_expires_at IS NOT NULL AND lease_expires_at < NOW()";

const ORG_BUSY_WITHOUT_WORK_SQL: &str = "SELECT COUNT(*) FROM participants p \
    WHERE p.organization_id = $1 AND p.status = 'busy' \
    AND NOT EXISTS (SELECT 1 FROM orchestration_tasks t \
        WHERE t.organization_id = p.organization_id AND t.assigned_agent_id = p.agent_id AND t.status = 'working')";

const ORG_WORK_WITHOUT_BUSY_SQL: &str = "SELECT COUNT(*) FROM orchestration_tasks t \
    WHERE t.organization_id = $1 AND t.status = 'working' \
    AND NOT EXISTS (SELECT 1 FROM participants p \
        WHERE p.organization_id = t.organization_id AND p.agent_id = t.assigned_agent_id AND p.status = 'busy')";
```

Add the method inside `impl AdminRepository` (after `stats()`):

```rust
/// Org-scoped orchestration control-plane snapshot. Runs the same wedged-state
/// checks the `OrchestrationMetricsWorker` emits globally, constrained to one
/// organization via `WHERE organization_id = $1`. `job_queue` depth is omitted
/// (no org column). `stale_after_secs` is the participant-staleness threshold.
pub async fn org_control_plane_snapshot(
    &self,
    scope: &TenantScope,
    stale_after_secs: i64,
) -> AppResult<OrgControlPlaneSnapshot> {
    let org = scope.org_id().as_uuid();
    let assignment_outbox_backlog =
        sqlx::query_scalar::<_, i64>(ORG_OUTBOX_BACKLOG_SQL).bind(org).fetch_one(&self.pool).await?;
    let assignment_outbox_oldest_age_seconds =
        sqlx::query_scalar::<_, f64>(ORG_OUTBOX_OLDEST_AGE_SQL).bind(org).fetch_one(&self.pool).await?;
    let stale_participants = sqlx::query_scalar::<_, i64>(ORG_STALE_PARTICIPANTS_SQL)
        .bind(org)
        .bind(stale_after_secs as i32)
        .fetch_one(&self.pool)
        .await?;
    let expired_working_leases =
        sqlx::query_scalar::<_, i64>(ORG_EXPIRED_LEASES_SQL).bind(org).fetch_one(&self.pool).await?;
    let busy_participants_without_work =
        sqlx::query_scalar::<_, i64>(ORG_BUSY_WITHOUT_WORK_SQL).bind(org).fetch_one(&self.pool).await?;
    let working_tasks_without_busy_participant =
        sqlx::query_scalar::<_, i64>(ORG_WORK_WITHOUT_BUSY_SQL).bind(org).fetch_one(&self.pool).await?;

    Ok(OrgControlPlaneSnapshot {
        assignment_outbox_backlog,
        assignment_outbox_oldest_age_seconds,
        stale_participants,
        expired_working_leases,
        busy_participants_without_work,
        working_tasks_without_busy_participant,
        stale_after_seconds: stale_after_secs,
    })
}
```

- [ ] **Step 5: Add the tenant-isolation `#[sqlx::test]` in `repositories/admin.rs`**

In the test module. Build two orgs; seed org A with one expired-lease working task, one stale participant, and one unpublished assignment outbox row; seed org B with a _fresh_ participant and a _published_ outbox row that must NOT be counted; assert org A's snapshot and that org B's snapshot is isolated. Use `OrgId::new()` / `UserId::new()` / `AgentId::new()` and `.as_uuid()` for seeding (import them from `agentforge_core` at the top of the test mod or file as the existing tests do). Reuse the existing `seed_org`/`seed_user` helpers if their signatures fit; otherwise insert directly as shown.

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn org_control_plane_snapshot_is_tenant_isolated(pool: PgPool) {
    use agentforge_core::{AgentId, OrgId, TenantScope, UserId};
    use uuid::Uuid;

    async fn seed_org_row(pool: &PgPool, org: Uuid) {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org)
            .bind(format!("Org {org}"))
            .bind(format!("org-{org}"))
            .execute(pool)
            .await
            .expect("seed org");
    }
    async fn seed_user_row(pool: &PgPool, user: Uuid) {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user)
            .bind(format!("u-{user}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
    }

    let org_a = OrgId::new();
    let org_b = OrgId::new();
    let user = UserId::new();
    seed_org_row(&pool, org_a.as_uuid()).await;
    seed_org_row(&pool, org_b.as_uuid()).await;
    seed_user_row(&pool, user.as_uuid()).await;

    let agent_a = AgentId::new();

    // Org A: an expired-lease working task (assigned, lease in the past).
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, assigned_agent_id, lease_expires_at, created_by, updated_at) \
         VALUES ($1, $2, 'A task', 'working', $3, NOW() - INTERVAL '1 hour', $4, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(org_a.as_uuid())
    .bind(agent_a.as_uuid())
    .bind(user.as_uuid())
    .execute(&pool)
    .await
    .expect("seed org A task");

    // Org A: a stale participant (non-offline, old heartbeat) that is also
    // 'busy' but has no matching working task for THAT agent -> also counts as
    // busy_participants_without_work.
    sqlx::query(
        "INSERT INTO participants (organization_id, agent_id, status, last_heartbeat_at) \
         VALUES ($1, $2, 'busy', NOW() - INTERVAL '2 hours')",
    )
    .bind(org_a.as_uuid())
    .bind(AgentId::new().as_uuid())
    .execute(&pool)
    .await
    .expect("seed org A participant");

    // Org A: an unpublished assignment outbox row.
    sqlx::query(
        "INSERT INTO orchestration_outbox (id, organization_id, aggregate_type, aggregate_id, event_type, payload, created_at) \
         VALUES ($1, $2, 'task', $3, 'assignment', '{}'::jsonb, NOW() - INTERVAL '30 seconds')",
    )
    .bind(Uuid::new_v4())
    .bind(org_a.as_uuid())
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("seed org A outbox");

    // Org B: a FRESH participant + a PUBLISHED outbox row -> must NOT be counted
    // for either org.
    sqlx::query(
        "INSERT INTO participants (organization_id, agent_id, status, last_heartbeat_at) \
         VALUES ($1, $2, 'available', NOW())",
    )
    .bind(org_b.as_uuid())
    .bind(AgentId::new().as_uuid())
    .execute(&pool)
    .await
    .expect("seed org B participant");
    sqlx::query(
        "INSERT INTO orchestration_outbox (id, organization_id, aggregate_type, aggregate_id, event_type, payload, published_at, created_at) \
         VALUES ($1, $2, 'task', $3, 'assignment', '{}'::jsonb, NOW(), NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(org_b.as_uuid())
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("seed org B outbox");

    let repo = AdminRepository::new(pool.clone());

    let snap_a = repo
        .org_control_plane_snapshot(&TenantScope::new(org_a, user), 60)
        .await
        .expect("snapshot A");
    assert_eq!(snap_a.expired_working_leases, 1, "org A has one expired lease");
    assert_eq!(snap_a.stale_participants, 1, "org A has one stale participant");
    assert_eq!(snap_a.assignment_outbox_backlog, 1, "org A has one unpublished assignment");
    assert!(snap_a.assignment_outbox_oldest_age_seconds > 0.0, "oldest age is positive");
    assert_eq!(snap_a.busy_participants_without_work, 1, "org A busy participant has no working task");
    assert_eq!(snap_a.stale_after_seconds, 60);

    let snap_b = repo
        .org_control_plane_snapshot(&TenantScope::new(org_b, user), 60)
        .await
        .expect("snapshot B");
    assert_eq!(snap_b.expired_working_leases, 0, "org B sees none of org A's tasks");
    assert_eq!(snap_b.stale_participants, 0, "org B participant is fresh");
    assert_eq!(snap_b.assignment_outbox_backlog, 0, "org B outbox row is published");
    assert_eq!(snap_b.assignment_outbox_oldest_age_seconds, 0.0);
}
```

NOTE for the implementer: verify the exact constructors (`OrgId::new()`, `AgentId::new()`, `UserId::new()`, `.as_uuid()`) against `rust/crates/core/src/` and the `participants` / `orchestration_tasks` / `orchestration_outbox` column names against `crates/db/migrations/004_orchestration.sql` and `033_orchestration_durable_delivery.sql`. Adjust column lists in the INSERTs to satisfy NOT NULL constraints (add any required columns with sensible test values). If `participants` requires additional NOT NULL columns, include them. Keep the assertions exactly as written.

- [ ] **Step 6: Run the repo test (needs a local Postgres for `#[sqlx::test]`)**

Run: `cd rust && cargo test -p agentforge-api --lib org_control_plane_snapshot_is_tenant_isolated`
Expected: PASS. If no local DB is configured, this test is exercised by CI's `Rust Tests` job; still run `cargo build -p agentforge-api --tests` locally to confirm it compiles.

- [ ] **Step 7: Add the service method in `services/admin.rs`**

Import `OrgControlPlaneSnapshot` alongside the existing `domain::admin` imports. Add after `stats()`:

```rust
/// Org-scoped orchestration control-plane snapshot for the admin health panel.
/// `stale_after_secs` is the participant-staleness threshold (caller supplies
/// it so the route stays free of config reads).
pub async fn org_control_plane_snapshot(
    &self,
    scope: &TenantScope,
    stale_after_secs: i64,
) -> AppResult<OrgControlPlaneSnapshot> {
    self.repo.org_control_plane_snapshot(scope, stale_after_secs).await
}
```

Ensure `TenantScope` is in scope in `services/admin.rs` (it is used by impersonation methods already).

- [ ] **Step 8: Add the route handler + registration in `routes/admin.rs`**

Add the handler near `get_stats`:

```rust
/// Org-scoped orchestration control-plane snapshot — the "is a loop wedged"
/// signals the metrics worker emits as Prometheus gauges, readable without a
/// Prometheus stack. Admin-gated; scoped to the caller's org. `job_queue` depth
/// is platform-global (no org column) and intentionally not included.
async fn get_control_plane(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let stale_after_secs = agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER.as_secs() as i64;
    let snapshot = service.org_control_plane_snapshot(&auth.scope, stale_after_secs).await?;
    Ok(Json(admin_data_response(snapshot)))
}
```

Register inside `admin_routes()` (after `/admin/stats`):

```rust
        .route("/admin/control-plane", get(get_control_plane))
```

NOTE: do not read `state.config` in the handler (boundary test). `agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER` is a `Duration` const — referencing it is allowed.

- [ ] **Step 9: Format + clippy + boundary test + targeted tests**

Run:

```bash
cd rust && cargo fmt --all
cargo clippy -p agentforge-api --all-targets -- -D warnings
cargo test -p agentforge-api --test route_ddd_boundary_test
cargo test -p agentforge-api --lib org_control_plane
```

Expected: fmt clean, clippy 0 warnings, boundary test PASS, both unit tests PASS (repo test requires DB — otherwise confirm `cargo build -p agentforge-api --tests`).

- [ ] **Step 10: Commit**

```bash
git add rust/crates/api/src/domain/admin.rs rust/crates/api/src/repositories/admin.rs rust/crates/api/src/services/admin.rs rust/crates/api/src/routes/admin.rs
git commit -m "feat(api): org-scoped control-plane snapshot admin endpoint (#805)"
```

---

### Task 2: Frontend — control-plane admin panel

**Files:**

- Modify: `src/app/shared/model/admin.store.ts`
- Create: `src/app/features/admin/ControlPlanePanel.tsx`
- Create: `src/app/features/admin/controlPlaneErrorMessage.ts`
- Modify: `src/app/features/admin/AdminLayout.tsx`
- Create: `tests/unit/app/ControlPlanePanel.test.tsx`

**Interfaces:**

- Consumes: `GET /api/v1/admin/control-plane` → `{ ok: true, data: OrgControlPlaneSnapshot }` (camelCase keys from Task 1).
- The TS `OrgControlPlaneSnapshot` interface MUST exactly match the Rust serde keys: `assignmentOutboxBacklog, assignmentOutboxOldestAgeSeconds, staleParticipants, expiredWorkingLeases, busyParticipantsWithoutWork, workingTasksWithoutBusyParticipant, staleAfterSeconds`.

- [ ] **Step 1: Extend the store in `admin.store.ts`**

(a) Extend the section union (line 8):

```ts
export type AdminSection =
  | 'users'
  | 'organizations'
  | 'agents'
  | 'health'
  | 'cli-images'
  | 'control-plane'
```

(b) Add the TS interface near the other admin data types (e.g. next to `SystemHealth`):

```ts
export interface OrgControlPlaneSnapshot {
  assignmentOutboxBacklog: number
  assignmentOutboxOldestAgeSeconds: number
  staleParticipants: number
  expiredWorkingLeases: number
  busyParticipantsWithoutWork: number
  workingTasksWithoutBusyParticipant: number
  staleAfterSeconds: number
}
```

(c) Add to the `AdminState` interface (mirror the `health`/`healthLoading`/`healthError` triplet and `loadHealth` signature):

```ts
controlPlane: OrgControlPlaneSnapshot | null
controlPlaneLoading: boolean
controlPlaneError: string | null
loadControlPlane: () => Promise<void>
```

(d) In the `create<...>` initializer, initialize the new fields:

```ts
  controlPlane: null,
  controlPlaneLoading: false,
  controlPlaneError: null,
```

(e) Add the `loadControlPlane` action, mirroring `loadHealth` (defensive parse, error mapping via `controlPlaneErrorMessage`, never throw):

```ts
  loadControlPlane: async () => {
    set({ controlPlaneLoading: true, controlPlaneError: null })
    try {
      const res = await adminFetch('/api/v1/admin/control-plane')
      const body = (await res.json().catch(() => ({}))) as
        | { ok?: boolean; data?: Partial<OrgControlPlaneSnapshot> }
        | null
      if (!res.ok || !body?.ok || !body.data) {
        set({ controlPlaneError: controlPlaneErrorMessage(res.status), controlPlaneLoading: false })
        return
      }
      const d = body.data
      // Coerce every field to a finite number so a malformed payload cannot
      // crash the panel.
      const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0)
      set({
        controlPlane: {
          assignmentOutboxBacklog: num(d.assignmentOutboxBacklog),
          assignmentOutboxOldestAgeSeconds: num(d.assignmentOutboxOldestAgeSeconds),
          staleParticipants: num(d.staleParticipants),
          expiredWorkingLeases: num(d.expiredWorkingLeases),
          busyParticipantsWithoutWork: num(d.busyParticipantsWithoutWork),
          workingTasksWithoutBusyParticipant: num(d.workingTasksWithoutBusyParticipant),
          staleAfterSeconds: num(d.staleAfterSeconds),
        },
        controlPlaneLoading: false,
      })
    } catch {
      set({ controlPlaneError: controlPlaneErrorMessage(0), controlPlaneLoading: false })
    }
  },
```

NOTE: match the exact `adminFetch`/`set`/error-handling idiom already used by `loadHealth` in this file — if `loadHealth` uses a slightly different envelope check or a shared error helper, follow that precedent rather than the sketch above.

- [ ] **Step 2: Create `features/admin/controlPlaneErrorMessage.ts`**

Mirror `features/admin/systemHealthErrorMessage.ts`: a function mapping an HTTP status (and `0` for network error) to operator-facing copy with a recovery step. Provide cases for 0 (network), 401/403 (sign in as an admin), 503 (control plane unavailable), and a default.

```ts
/// Maps a control-plane fetch failure to operator-facing copy + recovery step.
export function controlPlaneErrorMessage(status: number): string {
  switch (status) {
    case 0:
      return 'Could not reach the server. Check your connection and try again.'
    case 401:
    case 403:
      return 'You need an admin account to view the control-plane snapshot.'
    case 503:
      return 'The control plane is temporarily unavailable. Retry in a moment.'
    default:
      return 'Could not load the control-plane snapshot. Try refreshing.'
  }
}
```

(If `systemHealthErrorMessage.ts` exports a shared mapper or copy module, reuse it instead of duplicating the strings.)

- [ ] **Step 3: Create `features/admin/ControlPlanePanel.tsx`**

Mirror `SystemHealth.tsx` exactly for lifecycle: on mount call `loadControlPlane()`, then `setInterval` at the same refresh constant `SystemHealth` uses (e.g. `SYSTEM_HEALTH_REFRESH_MS`, imported or re-declared consistently), guarded by `document.visibilityState !== 'hidden'`; clear the interval on unmount. Subscribe to `controlPlane`, `controlPlaneLoading`, `controlPlaneError` from `useAdminStore`. Render: a loading state, an error state (the `controlPlaneError` string), and a grid of the 7 signals with human labels. Each numeric signal > 0 for the "wedge" signals should read as a warning (non-zero is the operator's cue). Show `staleAfterSeconds` as the heartbeat threshold context (e.g. "stale = no heartbeat in {staleAfterSeconds}s"). Include a one-line note that queue depth is platform-global and shown in `/metrics` (since it is intentionally omitted).

Use only Tailwind core utilities and the project's existing panel styling conventions (match `SystemHealth.tsx`). Labels:

- assignmentOutboxBacklog → "Unpublished assignment events"
- assignmentOutboxOldestAgeSeconds → "Oldest unpublished event (s)"
- staleParticipants → "Stale participants"
- expiredWorkingLeases → "Expired working leases"
- busyParticipantsWithoutWork → "Busy agents without work"
- workingTasksWithoutBusyParticipant → "Working tasks without a busy agent"

- [ ] **Step 4: Wire the panel into `AdminLayout.tsx`**

Add a nav entry for the new section (label e.g. "Control Plane", section key `'control-plane'`) alongside the existing nav items, and a branch in `SectionContent` (or the equivalent switch) that renders `<ControlPlanePanel />` when `activeSection === 'control-plane'`. Match the exact nav-item + routing structure already present for `'health'` and `'cli-images'`.

- [ ] **Step 5: Create `tests/unit/app/ControlPlanePanel.test.tsx`**

Mirror an existing admin panel test (e.g. a `SystemHealth` test if present, else `tests/unit/app/WorkspaceRows.test.tsx` for the render/mock idiom). Cover: (a) renders the six signal labels + values from a mocked store snapshot; (b) renders the error string when `controlPlaneError` is set; (c) a malformed/partial payload does not crash (the `num()` coercion → zeros). Mock `useAdminStore` (or the `adminFetch`) per the existing admin-test precedent.

- [ ] **Step 6: Run frontend checks**

Run:

```bash
npm run fsd:check
npm run lint
npm run typecheck
npm run test:unit -- ControlPlanePanel
```

Expected: fsd:check passes (no boundary violations), lint clean, typecheck clean, the new Vitest passes. Also run `npm run format:check` and fix with the project formatter if it flags the new files.

- [ ] **Step 7: Commit**

```bash
git add src/app/shared/model/admin.store.ts src/app/features/admin/ControlPlanePanel.tsx src/app/features/admin/controlPlaneErrorMessage.ts src/app/features/admin/AdminLayout.tsx tests/unit/app/ControlPlanePanel.test.tsx
git commit -m "feat(web): control-plane snapshot admin panel (#805)"
```

---

## Final validation (whole branch, before PR)

- `cd rust && make ci` (touches API contracts → full Rust gate incl. `route_ddd_boundary_test` + `Rust Tests`).
- `npm run fsd:check && npm run lint && npm run typecheck && npm run test:unit -- ControlPlanePanel`.
- PR body must include the `## Beginner UX / First-Time User Path` section with PLAIN `- Field:` bullets (no bold), 6 named fields, values ≥12 chars (UX gate).

## Self-Review (completed by plan author)

- Spec coverage: tenant-scoped repo method ✔ (Task 1 Step 4), domain projection ✔ (Step 1), admin GET behind auth ✔ (Step 8, `require_admin`), rendered in admin panel ✔ (Task 2), route auth covered by existing `require_admin` tests + tenant covered by the `#[sqlx::test]` ✔, expired-lease count asserted ✔ (Task 1 Step 5).
- Honest correction respected: `job_queue_*` omitted (no org column), documented in struct + panel + PR.
- Type consistency: Rust `OrgControlPlaneSnapshot` (snake_case + `rename_all="camelCase"`) ↔ TS `OrgControlPlaneSnapshot` (camelCase) ↔ panel labels — all seven fields aligned.
- Placeholder scan: every code step carries concrete code; the two implementer-verification NOTES (column NOT NULL lists, `loadHealth` idiom) are pointers to confirm exact local API, not deferred work.
