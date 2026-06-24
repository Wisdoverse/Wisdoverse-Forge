# Task #806 Implementation Report — Attempt Count + Lease Countdown on Task Detail Panel

## Summary

Surfaces `attempt` (1-based retry counter) and `leaseExpiresAt` (RFC3339 ISO string, only set
while `working`) through the full Rust → shared TS → React stack and renders them on the task
detail panel.

---

## API Fields Added

### `rust/crates/api/src/domain/orchestration.rs`

**`TaskSummary` struct** (after `context_counts`):
```rust
/// Current attempt number (1-based; incremented on each retry).
pub attempt: i32,
#[serde(rename = "leaseExpiresAt", skip_serializing_if = "Option::is_none")]
pub lease_expires_at: Option<String>,
```

**`task_summary()` function** — populates both fields from `OrchestrationTask`:
```rust
attempt: task.attempt,
lease_expires_at: task.lease_expires_at.map(|t| t.to_rfc3339()),
```

**`sample_task_summary()` test literal** — fixed to include:
```rust
attempt: 1,
lease_expires_at: None,
```

**New domain unit test** `task_summary_copies_attempt_and_lease_expires_at_from_row`:
- Builds an `OrchestrationTask` with `attempt: 3` and a known `lease_expires_at` timestamp
- Asserts `summary.attempt == 3` and `summary.lease_expires_at == Some("2026-06-22T09:00:00+00:00")`

---

## Shared TypeScript Contract

### `shared/types/agent.ts` — `TaskSummary` interface
```ts
/** 1-based attempt counter; incremented on each retry. */
attempt: number
/** RFC3339 timestamp when the current worker lease expires (only set while working). */
leaseExpiresAt?: string
```

### `src/app/shared/api/orchestration.ts` — `TaskSummary` interface
Same two fields added in the same location, keeping both contracts in sync.

---

## Panel Render — `src/app/features/detail/TaskMetadata.tsx`

Two additions to the badges row and below it:

1. **Attempt badge** — always visible, styled with the existing muted badge style:
   ```tsx
   <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-badge bg-apple-gray-5 text-apple-gray-2 tabular-nums">
     Attempt {task.attempt}
   </span>
   ```

2. **Lease countdown** — only when `task.state === 'working'` and `task.leaseExpiresAt != null`:
   ```tsx
   <p className="text-[10px] text-secondary-light dark:text-secondary-dark">
     Lease expires {formatRelativeTime(task.leaseExpiresAt)}
   </p>
   ```
   Reuses the existing `formatRelativeTime` import. Both `leaseExpiresAt` are read-only display.

---

## Test Output

### Rust stack

**fmt:**
```
FMT-OK
```

**cargo test -p agentforge-api task_summary:**
```
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1208 filtered out; finished in 0.00s
```
(Includes `task_summary_copies_attempt_and_lease_expires_at_from_row` + existing `task_summary_projects_kanban_response_and_inlines_blocked_hint`)

**cargo clippy -p agentforge-api -- -D warnings:**
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 01s
```
(Clean — no warnings or errors)

### Frontend stack

**fsd:check:**
```
FSD boundary check passed.
```

**lint:**
```
[beginner-ux-copy] UI copy guard passed.
```

**typecheck:**
```
(clean — no output = zero errors)
```

**test:unit:**
```
Test Files  1 failed | 168 passed (169)
Tests       1 failed | 2355 passed (2356)
```

The single failure (`WorkspaceRows.test.tsx > labels generated team and project link names
without implementation terms`) is **pre-existing** — it fails identically on the base commit
before any of these changes. Confirmed by `git stash` + re-run.

---

## Branch Confirmation

Branch: `worktree-feat-loop-eng-operator-panels`

---

## Files Changed

| File | Change |
|---|---|
| `rust/crates/api/src/domain/orchestration.rs` | `TaskSummary` struct, `task_summary()`, `sample_task_summary()` literal, new unit test |
| `shared/types/agent.ts` | `TaskSummary` interface — `attempt` + `leaseExpiresAt` |
| `src/app/shared/api/orchestration.ts` | `TaskSummary` interface — `attempt` + `leaseExpiresAt` |
| `src/app/features/detail/TaskMetadata.tsx` | Attempt badge + lease countdown render |

---

## Concerns

None. The `OrchestrationTask` row already carries `attempt: i32` and
`lease_expires_at: Option<DateTime<Utc>>` — no migration or schema change needed.
The `leaseExpiresAt` field is guarded by `skip_serializing_if = "Option::is_none"` on
the Rust side and `?` in TS so it only appears in WS payloads when a lease is active.
The `WorkspaceRows` test failure is pre-existing on this branch and out of scope.
