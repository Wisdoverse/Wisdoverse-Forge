# Temporal Preflight + Honest Orchestration Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the orchestrator from hard-aborting at boot when Temporal is unreachable; instead connect with a bounded timeout, degrade to an API-serving-only mode, and report the workflow-runtime state honestly on `/health`.

**Architecture:** Today `live_with_runtime` calls `build_live_workflow_components(...).await?`, so a Temporal connect failure propagates `?` all the way to `main` and the process exits — even though the orchestrator container `depends_on` only the API server, not Temporal (`docker/compose.yml:826-828`). This plan: (1) wrap the connect in a config-bounded `tokio::time::timeout`; (2) classify the build result into `WorkflowRuntimeStatus { Up, Disabled, Unreachable }` instead of erroring; (3) carry that status on `AppState` and surface it in the public `/health` body.

**Tech Stack:** Rust, Axum, Tokio, `temporalio_client`, `anyhow`.

## Global Constraints

- Backend ownership is Rust; all work is under `rust/crates/orchestrator` and `rust/bins/orchestrator`.
- `/health` is intentionally public infrastructure (CLAUDE.md auth carve-out) — keep it unauthenticated; do not leak internals (host/namespace go in logs, a coarse status goes in the body).
- Preserve current behavior when Temporal is healthy and when it is intentionally disabled (`temporal_enabled=false` already returns `Ok(None)` → "disabled", not an error).
- Adding a field to the `AppState` struct breaks every `AppState { .. }` literal site (entity-literal-fanout trap). Grep all sites (`state.rs` constructor + any test builders) and add the field; verify `cargo test -p agentforge-orchestrator --lib --no-run`.
- All worktrees share one cargo target; never run cargo in two worktrees concurrently.
- Validation per CLAUDE.md: narrow orchestrator test first, then `cd rust && make ci` (touches orchestration + startup).

## File Structure

- `rust/crates/orchestrator/src/config.rs` — add `temporal_connect_timeout_secs` (default 10).
- `rust/crates/orchestrator/src/workflow/mod.rs` (or `worker.rs`) — add `WorkflowRuntimeStatus` enum + a `build_workflow_runtime` wrapper that classifies instead of erroring, applying the connect timeout.
- `rust/crates/orchestrator/src/state.rs` — add `workflow_runtime: WorkflowRuntimeStatus` to `AppState`; degrade in `live_with_runtime`.
- `rust/crates/orchestrator/src/router.rs` — extend the root `health` handler to report `workflowRuntime`.
- `docker/compose.yml` — (optional, non-code) note that Temporal stays out of `depends_on`; the non-fatal boot is the contract.

---

### Task 1: Config `temporal_connect_timeout_secs`

**Files:**

- Modify: `rust/crates/orchestrator/src/config.rs` (struct ~64-119; `load` ~147-176; defaults ~24-30)

**Interfaces:**

- Produces: `config.temporal_connect_timeout_secs: u64` (default `10`, env `ORCHESTRATOR_TEMPORAL_CONNECT_TIMEOUT_SECS`).

- [ ] **Step 1: Write the failing test** (in the `config.rs` tests module)

```rust
    #[test]
    fn temporal_connect_timeout_defaults_to_ten() {
        assert_eq!(default_temporal_connect_timeout_secs(), 10);
    }
```

- [ ] **Step 2: Run it**

Run: `cd rust && cargo test -p agentforge-orchestrator temporal_connect_timeout_defaults_to_ten`
Expected: FAIL — `cannot find function default_temporal_connect_timeout_secs`.

- [ ] **Step 3: Add the default fn** (next to `default_temporal_host`, ~line 24)

```rust
fn default_temporal_connect_timeout_secs() -> u64 {
    10
}
```

- [ ] **Step 4: Add the struct field** (next to `temporal_namespace`)

```rust
    #[serde(default = "default_temporal_connect_timeout_secs")]
    pub temporal_connect_timeout_secs: u64,
```

- [ ] **Step 5: Parse it in `load()`** (next to `temporal_namespace`)

```rust
        temporal_connect_timeout_secs: read("ORCHESTRATOR_TEMPORAL_CONNECT_TIMEOUT_SECS")
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(default_temporal_connect_timeout_secs),
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cd rust && cargo test -p agentforge-orchestrator temporal_connect_timeout`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/orchestrator/src/config.rs
git commit -m "feat(orchestrator): add temporal_connect_timeout_secs config"
```

---

### Task 2: `WorkflowRuntimeStatus` + classifying builder

**Files:**

- Modify: `rust/crates/orchestrator/src/workflow/worker.rs` (alongside `build_live_workflow_components`)
- Modify: `rust/crates/orchestrator/src/workflow/mod.rs` (re-export the enum + fn)

**Interfaces:**

- Consumes: `build_live_workflow_components_with_factory` (existing test seam, worker.rs:54), `connect_temporal_client` (temporal.rs:181), `config.temporal_connect_timeout_secs` (Task 1).
- Produces:
  - `pub enum WorkflowRuntimeStatus { Up, Disabled, Unreachable }` with `pub fn as_str(&self) -> &'static str` → `"up" | "disabled" | "unreachable"`.
  - `pub async fn build_workflow_runtime(config: &Config, store: Option<Arc<dyn Store>>, outbound_mcp: Option<Arc<dyn OutboundMcp>>) -> (Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus)` — never returns `Err`; classifies a connect failure as `Unreachable` and a disabled/no-store case as `Disabled`.

- [ ] **Step 1: Write the failing tests** (in the worker.rs tests module — use the existing factory seam with fake connect/start closures, mirroring the current `_with_factory` tests)

```rust
    #[test]
    fn status_as_str_maps_each_variant() {
        assert_eq!(WorkflowRuntimeStatus::Up.as_str(), "up");
        assert_eq!(WorkflowRuntimeStatus::Disabled.as_str(), "disabled");
        assert_eq!(WorkflowRuntimeStatus::Unreachable.as_str(), "unreachable");
    }

    #[tokio::test]
    async fn build_runtime_disabled_when_temporal_off() {
        let mut config = test_config(); // temporal_enabled = false
        config.temporal_enabled = false;
        let store: Option<Arc<dyn Store>> = Some(Arc::new(MemoryStore::default()));
        let (components, status) = build_workflow_runtime(&config, store, None).await;
        assert!(components.is_none());
        assert_eq!(status, WorkflowRuntimeStatus::Disabled);
    }

    #[tokio::test]
    async fn build_runtime_unreachable_when_connect_fails() {
        let mut config = test_config();
        config.temporal_enabled = true;
        let store: Option<Arc<dyn Store>> = Some(Arc::new(MemoryStore::default()));
        // Use the factory seam with a connect that errors, then assert the
        // wrapper classifies it Unreachable instead of returning Err.
        let (components, status) =
            build_workflow_runtime_with_factory(
                &config,
                store,
                None,
                |_c| async { Err(anyhow::anyhow!("temporal down")) },
                |_c, _m, _s| Err(anyhow::anyhow!("unreachable")),
            )
            .await;
        assert!(components.is_none());
        assert_eq!(status, WorkflowRuntimeStatus::Unreachable);
    }
```

> `test_config()` and `MemoryStore` already exist in the orchestrator test support used by the current `_with_factory` tests — reuse them. If the existing tests construct the config/store differently, match that.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p agentforge-orchestrator build_runtime`
Expected: FAIL — `WorkflowRuntimeStatus` / `build_workflow_runtime` not found.

- [ ] **Step 3: Add the enum** (top of worker.rs)

```rust
/// Honest state of the Temporal-backed workflow runtime at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRuntimeStatus {
    /// Connected and the worker is running.
    Up,
    /// Intentionally off (`temporal_enabled=false`) or no store configured.
    Disabled,
    /// Enabled but Temporal could not be reached at boot.
    Unreachable,
}

impl WorkflowRuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowRuntimeStatus::Up => "up",
            WorkflowRuntimeStatus::Disabled => "disabled",
            WorkflowRuntimeStatus::Unreachable => "unreachable",
        }
    }
}
```

- [ ] **Step 4: Add the classifying wrappers**

```rust
/// Build the workflow runtime, classifying failures instead of propagating them.
/// A Temporal outage at boot becomes `Unreachable` (API keeps serving) rather
/// than aborting the process. Applies the configured connect timeout.
pub async fn build_workflow_runtime(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
) -> (Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus) {
    let timeout = std::time::Duration::from_secs(config.temporal_connect_timeout_secs.max(1));
    build_workflow_runtime_with_factory(
        config,
        store,
        outbound_mcp,
        move |config: Config| async move {
            match tokio::time::timeout(timeout, connect_temporal_client(&config)).await {
                Ok(result) => result,
                Err(_) => Err(anyhow!("temporal connect timed out after {}s", timeout.as_secs())),
            }
        },
        |client, outbound_mcp, store| {
            let activities = WorkflowActivities::new(outbound_mcp, store);
            start_worker(client, activities)
        },
    )
    .await
}

/// Test seam: same classification, injectable connect/start factories.
pub async fn build_workflow_runtime_with_factory<Connect, ConnectFut, Start>(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    connect: Connect,
    start: Start,
) -> (Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus)
where
    Connect: Fn(Config) -> ConnectFut,
    ConnectFut: Future<Output = anyhow::Result<temporalio_client::Client>>,
    Start: Fn(temporalio_client::Client, Arc<dyn OutboundMcp>, Arc<dyn Store>) -> anyhow::Result<WorkflowWorkerHandle>,
{
    // Distinguish "intentionally off" from "tried and failed": when temporal is
    // disabled or no store is configured, the inner builder returns Ok(None).
    if !config.temporal_enabled || store.is_none() {
        return (None, WorkflowRuntimeStatus::Disabled);
    }
    match build_live_workflow_components_with_factory(config, store, outbound_mcp, connect, start).await {
        Ok(Some(components)) => (Some(components), WorkflowRuntimeStatus::Up),
        Ok(None) => (None, WorkflowRuntimeStatus::Disabled),
        Err(err) => {
            tracing::error!(
                error = %err,
                temporal_host = %config.temporal_host,
                temporal_namespace = %config.temporal_namespace,
                "Temporal unreachable at boot; orchestrator serving in degraded (API-only) mode"
            );
            (None, WorkflowRuntimeStatus::Unreachable)
        }
    }
}
```

- [ ] **Step 5: Re-export** from `workflow/mod.rs`

```rust
pub use worker::{WorkflowRuntimeStatus, build_workflow_runtime};
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p agentforge-orchestrator build_runtime status_as_str`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/orchestrator/src/workflow/worker.rs rust/crates/orchestrator/src/workflow/mod.rs
git commit -m "feat(orchestrator): classify workflow-runtime boot state (up/disabled/unreachable)"
```

---

### Task 3: Degrade in `live_with_runtime` + carry status on `AppState`

**Files:**

- Modify: `rust/crates/orchestrator/src/state.rs` (struct ~27-47; `live_with_runtime` ~197-254)

**Interfaces:**

- Consumes: `build_workflow_runtime` + `WorkflowRuntimeStatus` (Task 2).
- Produces: `AppState.workflow_runtime: WorkflowRuntimeStatus`.

- [ ] **Step 1: Add the field** to the `AppState` struct (after `broadcaster`, before `ready`)

```rust
    pub workflow_runtime: crate::workflow::WorkflowRuntimeStatus,
    pub ready: bool,
```

- [ ] **Step 2: Switch `live_with_runtime` to the non-fatal builder** — replace the `build_live_workflow_components(...).await?` line (state.rs:225-226) and the component unpacking

```rust
    let (workflow_components, workflow_runtime) =
        crate::workflow::build_workflow_runtime(config.as_ref(), workflow_store.clone(), outbound_mcp.clone()).await;
    let workflow_service = workflow_components.as_ref().map(|components| components.service.clone());
    let workflow_worker = workflow_components.map(|components| components.worker);
```

- [ ] **Step 3: Set the field** in the `AppState { .. }` literal (add next to `broadcaster`)

```rust
            broadcaster: Arc::new(Broadcaster::new()),
            workflow_runtime,
```

- [ ] **Step 4: Fix all other `AppState { .. }` literal sites** (entity-literal-fanout trap)

Run: `cd rust && grep -rn "AppState {" crates/orchestrator`
For each (test builders, fixtures), add `workflow_runtime: crate::workflow::WorkflowRuntimeStatus::Disabled,` (a sensible default for tests that do not exercise Temporal).

- [ ] **Step 5: Verify lib + tests build**

Run: `cd rust && cargo test -p agentforge-orchestrator --lib --no-run`
Expected: compiles (no missing-field errors). This catches every literal site the grep might miss.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/orchestrator/src/state.rs
git commit -m "feat(orchestrator): degrade to API-only on Temporal outage; carry runtime status"
```

---

### Task 4: Report `workflowRuntime` on `/health`

**Files:**

- Modify: `rust/crates/orchestrator/src/router.rs` (root `health` handler, ~54-56)

**Interfaces:**

- Consumes: `AppState.workflow_runtime` (Task 3).
- Produces: a pure `pub fn health_body(status: WorkflowRuntimeStatus) -> serde_json::Value` and a handler that reads state.

- [ ] **Step 1: Write the failing test** (in router.rs tests, or a new `tests/health_contract.rs`)

```rust
    #[test]
    fn health_body_reports_runtime_status() {
        use crate::workflow::WorkflowRuntimeStatus::*;
        assert_eq!(health_body(Up)["workflowRuntime"], "up");
        assert_eq!(health_body(Disabled)["workflowRuntime"], "disabled");
        assert_eq!(health_body(Unreachable)["workflowRuntime"], "unreachable");
        // Keep the existing top-level field stable for current consumers.
        assert_eq!(health_body(Up)["status"], "healthy");
    }
```

- [ ] **Step 2: Run it**

Run: `cd rust && cargo test -p agentforge-orchestrator health_body_reports_runtime_status`
Expected: FAIL — `cannot find function health_body`.

- [ ] **Step 3: Add the pure body fn + wire the handler** (replace the current `health` at router.rs:54-56)

```rust
pub fn health_body(status: crate::workflow::WorkflowRuntimeStatus) -> serde_json::Value {
    serde_json::json!({
        "status": "healthy",
        "workflowRuntime": status.as_str(),
    })
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> axum::Json<serde_json::Value> {
    axum::Json(health_body(state.workflow_runtime))
}
```

> The root `health` route at router.rs:39 already uses `.with_state(state)`, so the `State` extractor resolves with no route-wiring change. `/health` stays unauthenticated.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd rust && cargo test -p agentforge-orchestrator health_body_reports_runtime_status`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/orchestrator/src/router.rs
git commit -m "feat(orchestrator): report workflowRuntime state on /health"
```

---

### Task 5: Full validation + ops note

- [ ] **Step 1: Run the full Rust CI gate**

Run: `cd rust && make ci`
Expected: PASS (clippy, fmt, `cargo test --workspace`).

- [ ] **Step 2: Manual degraded-boot check** (proves the abort is gone)

```bash
ORCHESTRATOR_TEMPORAL_ENABLED=true \
ORCHESTRATOR_TEMPORAL_HOST=127.0.0.1:1 \
ORCHESTRATOR_TEMPORAL_CONNECT_TIMEOUT_SECS=2 \
ORCHESTRATOR_DATABASE_URL=postgres://...  \
cargo run -p agentforge-server --bin orchestrator
```

In another shell: `curl -s localhost:4010/health` → expect `{"status":"healthy","workflowRuntime":"unreachable"}` and the process still running (previously it exited at boot).

- [ ] **Step 3: Ops note** — in `docs/guides/configuration.md` document `ORCHESTRATOR_TEMPORAL_CONNECT_TIMEOUT_SECS` and the three `workflowRuntime` states; note in `docs/runbooks/` that `workflowRuntime:"unreachable"` means "API up, Temporal down — workflows paused, fix Temporal." No `docker/compose.yml` change is required: Temporal intentionally stays out of `depends_on` because it may be external (the non-fatal boot is the contract).

- [ ] **Step 4: Commit**

```bash
git add docs/guides/configuration.md docs/runbooks/
git commit -m "docs: document temporal connect timeout + workflowRuntime health states"
```

---

## Self-Review

1. **Spec coverage:** Proposal 1.2 asks for (a) a bounded Temporal preflight — Task 2's `tokio::time::timeout` around `connect_temporal_client` using Task 1's config; (b) non-fatal degraded mode — Task 3 replaces the `?` with classification; (c) an honest readiness signal distinguishing "API up, worker down" — Task 4's `workflowRuntime: up|disabled|unreachable`.
2. **Placeholder scan:** `test_config()` and `MemoryStore` are referenced as existing test support (the current `_with_factory` tests use them) rather than re-defined — confirm their names before writing Task 2's tests. No "TBD"/"handle errors" placeholders remain.
3. **Type consistency:** `WorkflowRuntimeStatus { Up, Disabled, Unreachable }` and `as_str() -> "up"|"disabled"|"unreachable"` are used identically in Tasks 2, 3, and 4; `build_workflow_runtime` returns `(Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus)` and is consumed with that exact shape in Task 3.
