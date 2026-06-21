# Loop-Engineering Improvement Proposal

Status: Proposal (not yet scheduled)
Date: 2026-06-21
Scope: Backend (`rust/`) + frontend (`src/app`) loop subsystems
Method: 7 subsystems mapped against the loop-engineering framework, 33 candidate
improvements generated, 22 survived adversarial verification against the actual
code (11 dropped as fabricated-harm, already-solved, or infeasible).

## What "loop engineering" is, and why it applies here

"Loop engineering" is the mid-2026 term (Addy Osmani, quoting Anthropic's Boris
Cherny and Peter Steinberger) for the discipline that sits one layer above the
single-agent _harness_: instead of prompting an agent turn by turn, you design
the autonomous control program that _drives_ agents over time — it discovers
work, dispatches it to agents, verifies the result, persists state, and decides
the next action on a schedule or until a goal is met.

A well-engineered loop has five parts:

1. Trigger — what starts an iteration (schedule, event, human, prior completion).
2. Goal + stop condition — a _verifiable_ end state and an explicit, _bounded_
   stopping rule (max attempts / deadline / budget) so it can never cycle forever.
3. Actions — the tools the loop body may take, including sub-agent spawn.
4. Verifier — "something in the loop that can say **no**": a test, type check,
   grader, supervisor audit, or CI gate. This is the hard part.
5. Memory — persistence across iterations plus deliberate context reset.

Cross-cutting: human-in-the-loop (HITL) escalation, observability/traces, and
whether the loop is _closed_ (consumes its own output as feedback) or _open_.

AgentForge is not a consumer of loop engineering — it **is** a loop-engineering
platform. The orchestrator drives agents on Temporal workflows; the PG job queue
and NATS consumers are the trigger substrate; the review gate is a verifier; the
Phase-4 self-fix loop is a self-improving (L4) loop. So the right question is not
"should we adopt loops" but "where are our existing loops weak as loops?"

## Current-state assessment

The platform is strong where loop engineering is _easy_ and weak where it is
_hard_. The trigger substrate is mature — transactional outbox, `FOR UPDATE SKIP
LOCKED`, `pg_notify` wake with polling fallback, per-agent NATS auth, delivery-id
dedup, lease reaping. Persistence is solid.

The weaknesses cluster on the three dimensions loop engineering says are the
hard, high-leverage parts:

- The loops are **open, not closed** in observability. Several loops act but
  never report whether the action achieved its goal: the orchestrator's realtime
  `Broadcaster` has _zero_ production callers; the generic `job_queue` has no
  depth/age metric; the self-fix and dependency-reconcile loops emit no metrics.
- The **verifier is thin**. Task completion is accepted on the agent's say-so
  with no result-shape check; review verdicts have no legal-transition guard, no
  mandatory reject feedback, no self-approval block, and the verdict-to-task sync
  is fire-and-forget.
- The **HITL gate is unaccountable and unbounded**. The human-review signal
  records no _who_ and writes no audit row; the review queue has no SLA; merge
  retries are uncapped; the human-review deadline is a fixed 24h with no
  escalation.

And the flagship loop is not yet a loop: the **self-fix L4 loop has no automatic
trigger** — `open_pr` is dead code and `complete_task` never fires it, so a human
must pump every iteration by hand.

The remainder of this document is a prioritized roadmap. Every item carries a
file anchor, effort (S/M/L), risk, a validation path matching the project's
validation expectations, and — where the adversarial pass found one — an honest
correction to the original framing.

---

## Tier 1 — Turn half-built loops into real closed loops

### 1.1 Auto-fire the self-fix PR Bridge on task completion (self-fix L4 trigger)

Severity: high · Effort: L · Risk: medium · Dimension: trigger

The self-fix loop has no automatic trigger. `SelfFixService::open_pr` is
`#[allow(dead_code)]` (`rust/crates/api/src/services/self_fix/mod.rs:84`), the
orchestrator crate has zero self-fix references, and `complete_task`
(`rust/crates/api/src/services/orchestration.rs:585-646`) commits a self-fix task
to `completed` without ever invoking `open_pr`. Every shipped Phase-4 milestone
(Bridge, Merge Executor, Review UI) is therefore unreachable in production — the
only thing that advances a finished self-fix task is a human, which makes the
"self-iterating platform" a manual workflow.

Change: reuse the existing `project_clone_worker` job-queue + reconciler pattern
verbatim. In `complete_task`, when `task.self_fix` is true, enqueue a
`self-fix-open-pr` job inside the _same transaction_ as `set_result_in_tx` via
`agentforge_jobs::queue::enqueue` (transactional outbox, so a crash never loses
the trigger and it never fires for an uncommitted completion). Add a
`SelfFixPrWorker` modeled on `project_clone_worker.rs:258-326` that dequeues and
calls `open_pr`, with the standard interval reconciler backstop.

Validation: `#[sqlx::test]` (af_sqlx_bookkeep recipe) asserting `complete_task`
on a self-fix task inserts exactly one queue row in the same tx (zero for
non-self-fix), and the worker drives it to PR-opened; then `cd rust && make ci`.

### 1.2 Temporal preflight + honest orchestration readiness (trigger plane)

Severity: high · Effort: M · Risk: low · Dimension: trigger

The outer loop's trigger plane can come up half-dead with no signal.
`validate_runtime` (`rust/crates/orchestrator/src/config.rs:186-192`) only checks
that `mcp_token` is set; it never probes Temporal. With `temporal_enabled=true`,
`build_live_workflow_components` propagates a connect failure via `?` at
`state.rs:226`, so a Temporal outage _hard-aborts the orchestrator at boot_ —
there is no degraded-but-serving mode and no readiness endpoint distinguishing
"API up, workflow worker down." The deployment makes this worse: the orchestrator
container runs with `ORCHESTRATOR_TEMPORAL_ENABLED=true` and `depends_on` only
the API server, not Temporal (`docker/compose.yml:823-828`).

Change: add a bounded Temporal preflight before building the worker — attempt
`connect_temporal_client` with a short timeout, log an actionable message (host,
namespace), keep it non-fatal, and record `workflow_runtime_ready: bool` on
`AppState` next to the existing `ready` flag (`state.rs:46`). Extend `/health`
to report `workflowRuntime: 'up' | 'disabled' | 'unreachable'` so readiness is
honest.

Validation: Rust tests for the three readiness states using the existing
`build_live_workflow_components_with_factory` seam; `cd rust && make ci`.

### 1.3 Wire orchestrator state transitions into the Broadcaster (close the open loop)

Severity: medium · Effort: M · Risk: low · Dimension: observability

The L3 control loop produces state but never feeds it back. `Broadcaster::
broadcast()` (`rust/crates/orchestrator/src/realtime.rs:57`) has **zero**
production call sites — activities persist node status straight to PG
(`activities.rs:36-39, 109-114`) and the runtime persists workflow status
(`temporal.rs:133, 344, 448-460`), but nothing is published, so a client on the
orchestrator `/ws/events` channel sees no transitions at all.

Change: define a `WorkflowEventSink` trait in the workflow domain layer,
implement it over `Arc<Broadcaster>`, and call it at the single store chokepoint
every transition already flows through (after `update_node_status` /
`update_status`). `org_id` is already threaded into `ExecuteAgentTaskInput` and
`FinalizeWorkflowStatusInput`, so the scope `broadcast` filters on
(`realtime.rs:60`) is available without new lookups. Keep the sink
fire-and-forget (`try_send` is already non-blocking) so a slow client can't stall
the workflow.

Honest correction (from verification): the orchestrator `/ws/events` channel has
**no consumer wired in the product today** — the frontend connects to the _API_
crate's `/ws` and consumes a separate NATS-backed `orchestration:task_update`
(`src/app/hooks/useWsDispatch.ts:28`). So this completes a documented-but-
unconsumed contract and unblocks live Temporal-workflow observability; it is not
fixing a user-visible regression. Prioritize the workflow/node emit; the
`task/handler.rs` emit is the lowest-value part (product task moves are already
broadcast by the API crate).

Validation: unit test with a fake sink asserting emit fires on each transition
(extend the existing `realtime_contract.rs` pattern); `cd rust && make ci`;
frontend `npm run fsd:check` + Vitest for the new realtime reducer.

---

## Tier 2 — Build the verifier ("something that can say no")

### 2.1 Completion verifier: gate "done" on a result-shape check

Severity: medium · Effort: L · Risk: medium · Dimension: verifier

The loop has no verifier before a task is accepted as complete. `complete_task`
marks `status='completed'` / `progress=100` purely on the agent's say-so; the
only quality gate is an operator eyeballing artifacts in `TaskDetailPanel`. A
loop can declare success with an empty or malformed result. The gap exists on
**both** completion paths — the HTTP `complete_task` and the NATS
`orchestration_result_consumer` the loop actually uses.

Change: a pure-domain `CompletionVerifier` policy in
`rust/crates/api/src/domain/orchestration.rs` that takes the task params (which
can carry an expected result contract — required keys, non-empty diff, self-fix
PR fields) and the submitted result JSON and returns `Ok` or a typed rejection.
Wire it into both completion paths before the `SET_RESULT` transition; on
rejection route the task to `blocked/waiting_input` with a machine-readable hint
and broadcast, instead of `completed`.

Honest correction: self-fix already has a _strong_ server-side gate downstream
(`approve_and_merge` enforces CI + sensitivity + head-stability before merge), so
the verifier's value is mainly for **non-self-fix** tasks and for catching empty
self-fix results before they reach review.

Validation: domain unit tests (passes on contract match, rejects on missing keys,
rejects self-fix without `pr_number`); `#[sqlx::test]` proving a rejected
completion lands `blocked` not `completed`; `cd rust && make ci`.

### 2.2 Review state-machine guards: legal transitions, mandatory reject feedback, no self-approval

Severity: medium · Effort: M · Risk: low · Dimension: verifier

The verifier accepts any verdict with no rules. `store.update_state`
(`rust/crates/orchestrator/src/review/repository.rs:176`) blindly writes any
`ReviewState`, so an Approved review can be silently re-flipped; the model
(`review/model.rs:10-16`) has no transition table. The reject feedback field is
collected but discarded (`review/handler.rs:35-38` marks it `#[allow(dead_code)]`)
— a rejection (a "no") carries no reason. There is no self-approval block. The
existing `review_contract.rs` test confirms no guard is enforced today.

Change: add a pure `can_transition(from, to) -> bool` and
`verdict_requires_feedback` to `review/model.rs`; in the handler, reject illegal
or terminal transitions with 409, require non-empty feedback on reject and
persist it, and block the creator approving their own review with 403.

Validation: unit-test `can_transition` for every pair; handler tests for the 400
/ 409 / 403 cases and the happy path; `cd rust && make ci`.

### 2.3 Make the review verdict and task-state sync transactional

Severity: medium · Effort: M · Risk: medium · Dimension: verifier

Approve/reject update the review and the linked task in two separate,
non-transactional calls, and the task result is silently discarded —
`review/handler.rs:154-156` and `:183-185` do `let _ = task_store.update_state(...)`.
If the review write succeeds and the task write fails, the system holds an
Approved review whose task is not Completed, with no error to the caller and no
reconciliation.

Change: add `apply_verdict(id, org_id, new_review_state, task_id, new_task_state)`
to the review `Store` trait, implemented in `PgReviewStore` as a single `sqlx`
transaction updating `code_reviews` and `tasks` together, returning `NotFound`
if either `rows_affected == 0`. The handler calls one method instead of two
`let _ =`.

Validation: a test injecting a task-store failure mid-verdict asserts the review
state rolls back and the handler returns an error; happy-path asserts both rows
flip together; `cd rust && make ci`.

### 2.4 Dependency-reconcile verifier + alertable metric

Severity: medium · Effort: S · Risk: low · Dimension: verifier

`DependencyReconcileWorker` is a closed self-heal loop with no verifier and no
metric. `dependency_reconcile.rs:75-78` only WARN-logs when it unblocks orphan
children, and the crate's `register_metrics` fan-out (`lib.rs:91-99`) has no
entry for it. The loop can never tell a dashboard that the happy-path
`complete_task` tx is failing (which is _why_ the backstop is firing), and it has
no check that an unblocked child is ever actually claimed.

Change: add `register_metrics()` (matching the `credential_consumer.rs:345-377`
pattern) for `agentforge_dependency_reconcile_unblocked_total` (rate > 0 = the tx
backstop is firing = alert) and `..._tick_errors_total`. Wire into `lib.rs:91`.

Honest correction: the original "stuck queued child" verifier-gauge idea is
weaker than the unblock/error counters — the counters are the clean S/low fit;
treat the gauge as optional.

Validation: `cargo test -p agentforge-jobs dependency_reconcile` (keep the
cross-tenant pin test green; add a register-prime test); `cd rust && make ci`.

---

## Tier 3 — HITL accountability and bounds

### 3.1 Audited, identity-bound human-review signal

Severity: medium · Effort: M · Risk: medium · Dimension: hitl

The human-review signal is the loop's only kill switch, and it records no _who_
and writes no audit row. `SignalRequest` (`workflow/model.rs:183-189`) carries
only `node_id/decision/comment`; the handler resolves `identity` but uses only
`identity.org_id`, discarding `user_id` (`workflow/handler.rs:219-241`); neither
the service nor the runtime persists the decider. Any authenticated org user can
approve/reject any node with zero record.

Change: add `AuditAction::WorkflowReviewApprove/Reject`; after a successful
signal, write an `AuditLog` with `actor_id = identity.user_id` via the existing
`audit_store`; persist a `reviewed_by` identity on the node (additive migration +
MANIFEST + `pool.rs include_str!` per the migration contract).

Validation: Rust test asserting one audit row with the caller's `user_id` and the
right action, and `reviewed_by` persisted; schema-contract test; `cd rust && make ci`.

### 3.2 Emit audit records on every review verdict

Severity: medium · Effort: M · Risk: low · Dimension: verifier

The review approve/reject/create/comment paths emit zero audit records even
though `AuditAction` already defines `ReviewCreate/Approve/Reject/Comment`
(`orchestrator/src/audit/model.rs:19-26`) and `AppState` holds `audit_store`
(`state.rs:32`). The most security-sensitive governance verdicts in the system
are forensically invisible.

Honest correction (broadens the problem): the orchestrator `audit_store` has **no
writers anywhere** — tasks, workflows, and teams emit nothing either, and
`audit_logs` is read-only in practice. So this should be framed as "wire the
orchestrator audit sink" with reviews as the first and highest-priority writer,
not a review-only fix. (3.1 is the workflow-signal slice of the same gap.)

Change: after each successful review state change, build and persist an `AuditLog`
(actor, resource=`review`, action, from/to state, comment, ip/user-agent),
emitted before returning success.

Validation: integration tests asserting one correct audit row per verdict route;
`cd rust && make ci`.

### 3.3 Configurable human-review deadline with escalation

Severity: medium · Effort: M · Risk: low · Dimension: boundedness

`human_review_activity_options` (`workflow/temporal.rs:479-484`) hardcodes a 24h
start-to-close timeout; the activity heartbeats forever waiting for a signal
(`activities.rs:135-153`). If no one signals, the node silently times out after
24h with no reminder and no way to tune the deadline per node.

Honest correction: this fails _closed_ (the workflow terminates rather than
hanging), so it is not a safety hole — the harm is operator surprise and a
one-size deadline.

Change: read a per-node `reviewTimeoutSecs` from `node.config` with an
`ORCHESTRATOR_REVIEW_TIMEOUT_SECS` config default; add warn-threshold heartbeats
(e.g. 50% / 90% of deadline) that emit a realtime event via the 1.3 sink.

Validation: unit tests for deadline derivation/fallback and the threshold math
(deterministic); `cd rust && make ci`.

### 3.4 Bound the self-fix loop: merge-retry cap + stuck-review reconciler

Severity: medium · Effort: M · Risk: low · Dimension: boundedness

The self-fix loop is unbounded on two axes: no max-retry cap on
`approve_and_merge` (an operator can re-click Finish forever if CI stays red or
the head keeps moving — `merge_executor.rs:122-145`), and no timeout on the human
decision (a task can sit `in_review` indefinitely; `mod.rs:226` only ever writes
MERGED).

Change: corrective additive migration adding `merge_attempts INT DEFAULT 0` and
`review_opened_at TIMESTAMPTZ` to `orchestration_tasks`; bump `merge_attempts`
before running the executor and refuse past a config cap
(`self_fix_max_merge_attempts`, default 5) with a typed error that moves the task
to `changes_requested`; add an interval reconciler that ages out / escalates
stuck `in_review` tasks.

Validation: additive idempotent migration registered in MANIFEST + `pool.rs`;
merge-executor tests for the (cap+1)th approve and the stuck-review sweep;
`cd rust && make ci`.

### 3.5 Review SLA (`due_at`) + stuck-review and verdict-rate metrics

Severity: low · Effort: L · Risk: medium · Dimension: observability

Reviews created directly via `POST /reviews` are not covered by the Temporal 24h
activity timeout — they can sit Pending/InReview forever (`review/model.rs:53-65`
has no `expires_at`), and there are no metrics for verdict rate, queue depth, or
time-to-verdict.

Change: add nullable `due_at` to `code_reviews` (orchestrator migration +
MANIFEST + `migrations.rs` entry), defaulted from a configurable SLA at create;
add `reviews_pending_total`, `reviews_overdue_total`, `review_verdicts_total
{verdict}`, and a time-to-verdict histogram via the existing orchestrator metrics
store.

Validation: migration + `manifest_integrity_test`; repo test for overdue
filtering; metrics test; `cd rust && make ci`. (Lower priority — overlaps 3.4's
escalation; do after 3.4.)

---

## Tier 4 — Make every loop report whether work actually moved

### 4.1 Job-queue depth + dead-count gauges

Severity: medium · Effort: S · Risk: low · Dimension: observability

There is no backpressure signal over the PG queue. The orchestration-outbox path
has rich gauges (`orchestration_metrics.rs:148-150`), but the generic `job_queue`
(carrying `project_clone` and future jobs, including the 1.1 self-fix-pr job) has
none — no pending/running/dead counts, no oldest-pending age.

Change: extend `OrchestrationControlPlaneSnapshot` with `job_queue_pending/
running/dead` and `job_queue_oldest_pending_age_seconds` (COUNT / MIN(created_at)
grouped by status, on the existing tick), emitted as gauges materialized at 0 to
avoid absent series. Note: `release_stale_locks` is exported but has **no caller**
— wiring it in is a natural companion (it was proposed and dropped only because
its _harm framing_ was wrong, not the dead-code fact).

Validation: extend the `#[sqlx::test]` at `orchestration_metrics.rs:346`;
`cd rust && make ci`.

### 4.2 Self-fix loop metrics + queryable audit projection

Severity: low · Effort: M · Risk: low · Dimension: observability

The self-fix loop is the one long-running loop with no metrics — no counters for
Bridge success/failure, merge attempts, or sensitive rejections, and the only
audit record is the GitHub PR comment (not queryable via the API). First-class
metrics infra already exists (`auth_callout/metrics.rs`, `register_metrics` at
`main.rs:132`).

Change: add `self_fix/metrics.rs` mirroring `auth_callout/metrics.rs` —
`agentforge_self_fix_bridge_total{outcome}`, `..._merge_total{outcome}`,
`..._review_open` gauge — incremented at the decision points in
`merge_executor.rs` and `open_pr`. (`#[instrument]` spans are a cheap optional
add; the queryable-audit projection is the lower-value part — defer it.)

Validation: extend `metrics_endpoint_test.rs`; `cd rust && make ci`.

### 4.3 Claim-success + JetStream consumer-lag metrics

Severity: low · Effort: M · Risk: low · Dimension: observability

`ParticipantLiveness` claims work but the claimed count is only an INFO log
(`participant_liveness.rs:424`), so "heartbeats arriving but no work picked up" is
undetectable; `CredentialStreamWorker` has error metrics but no consumer-lag
gauge.

Change: add `agentforge_orchestration_participant_tasks_claimed_total` and a
cached `consumer.info()` lag gauge `credential_sync_consumer_pending`, both
through the existing metrics path and `register_metrics`.

Honest correction: the participant-claim counter is largely convenience (existing
outbox + busy-without-work gauges already approximate it); the consumer-lag gauge
is the more valuable half. Pure additive instrumentation, low risk.

Validation: `cargo test -p agentforge-jobs`; `cd rust && make ci`.

### 4.4 Expose the control-plane snapshot in the admin health panel

Severity: medium · Effort: M · Risk: low · Dimension: observability

The 15s `OrchestrationMetricsWorker` already computes the exact "is a loop
wedged" signals — stale participants, expired leases, busy-without-work,
work-without-busy, outbox backlog (`orchestration_metrics.rs:11-18, 84-143`) —
but only emits Prometheus gauges. Operators without Prometheus can't see them.

Change: add a tenant-scoped repository method (accepts `&TenantScope`) that runs
the same checks org-scoped, a domain projection in `domain/admin.rs`, and a
`GET` under the admin router behind auth; render in the admin health panel
(`src/app/shared/model/admin.store.ts`).

Honest correction: `collect_control_plane_snapshot` is **global** (no org WHERE),
so the org-scoped method is real new work, not a trivial reuse of the worker SQL.

Validation: route auth/tenant test; `#[sqlx::test]` seeding an expired-lease
task; `cd rust && make ci`; frontend `npm run fsd:check` + lint + typecheck.

### 4.5 Surface attempt count + lease countdown on the task panel

Severity: low · Effort: S · Risk: low · Dimension: observability

Loop-iteration state exists in the DB but never reaches the operator.
`OrchestrationTask` carries `attempt` and `lease_expires_at`
(`db/src/entities.rs:466-467`) and `TaskAssignmentSnapshot` reads them, but
`task_summary()` drops both and the shared `TaskSummary` contract has no such
fields, so `TaskDetailPanel` can't show them.

Honest correction: `max_attempts` does **not** exist on the orchestration task
row — surface `attempt` and `lease_expires_at` only (an "Attempt N" badge and a
lease countdown), not "N of M".

Change: add `attempt` and `lease_expires_at` to the `TaskSummary` domain struct
and the `shared/types/agent.ts` contract (keep Rust serializer and TS in sync per
the Frontend Contracts rule); render read-only in `TaskDetailPanel`.

Validation: domain unit test; `npm run fsd:check`, lint, format:check, typecheck,
Vitest.

---

## Tier 5 — Runtime boundedness, backpressure, and dead-letter

### 5.1 Bound the relay WAL with backpressure

Severity: medium · Effort: M · Risk: low · Dimension: boundedness

The sidecar relay listener spawns an unbounded task per accept
(`unix_socket_listener.rs:102-112`) and the WAL-first path appends to disk before
publishing with no size or count ceiling (`wal.rs:29-37`). During a NATS outage
every hook event is written and _kept_ (`WalAction::Keep`); a chatty CLI in a
tight loop can fill the container disk. In steady state the WAL drains, but the
routine ~15-min per-agent JWT reconnect window (`main.rs:54-59`) and longer
outages are real exposure.

Change: add `WAL_MAX_PENDING` / `WAL_MAX_BYTES` admission limits (drop oldest /
reject newest with a structured warn + dropped-events counter), cap relay
listener concurrency with a `Semaphore` before the per-connection spawn, and feed
WAL depth into the heartbeat (see 5.2).

Validation: `wal.rs` unit tests (append past ceiling drops/rejects determinist-
ically, no-op below); listener burst test; sidecar build + `cd rust && make ci`.

### 5.2 Escalate sidecar circuit-breaker-open as a HITL heartbeat signal

Severity: medium · Effort: M · Risk: medium · Dimension: hitl

When the watchdog hits `SIDECAR_MAX_RESTARTS=5` the circuit breaker opens
(`docker/scripts/agent-entrypoint.sh:1017-1021`) and the agent's relay is
permanently dead for the session, but the only output is a local stdout `ALERT`.
The agent keeps heartbeating as healthy (`sidecar/src/publisher.rs:101-117`), so
the dispatcher keeps routing work to an agent whose events vanish into the WAL,
with no operator-visible signal.

Change: extend the heartbeat payload with a `health` object `{state: ok|degraded,
reason, sidecar_restarts, wal_pending}` (the sidecar already knows
`wal.pending_count`); add a `degraded` participant state in `participant_liveness`
/ `presence_store` so the UI and dispatcher can see and de-prioritize it.

Validation: sidecar unit (payload serializes, defaults `ok`); participant_liveness
unit (degraded beat flips presence); `cd rust && make ci`.

### 5.3 Closed-loop task assignment: durable dispatch record instead of fire-and-forget

Severity: medium · Effort: L · Risk: medium · Dimension: verifier

`POST /tasks/{id}/assign` returns 200 immediately (`task/handler.rs:229`) then
does the real work — `session_create`, agent upsert, `set_session_id`, prompt,
transition to Working — in a detached `tokio::spawn` (`handler.rs:186-226`).
Every failure in that closure is only `tracing::error!`-logged and dropped; the
client is told "assigned" while the task may never start, and nothing retries or
surfaces it.

Change: add a tenant-scoped `task_dispatches` table (migration + MANIFEST +
`pool.rs`): `task_id, org_id, status (queued|starting|started|failed), attempt,
last_error, session_id, timestamps`. Insert `queued` synchronously before
spawning; have the closure drive it through `starting → started | failed` at each
step (replacing the bare error logs). Return 202 Accepted with the dispatch id;
add `GET /tasks/{id}/dispatch`; broadcast on terminal states.

Validation: Rust tests — assign inserts `queued` before spawn; a forced
`session_create` failure drives the row to `failed` with `last_error` and leaves
the task `Assigned`; `cd rust && make ci`.

### 5.4 Blocked-task TTL reaper

Severity: low · Effort: M · Risk: medium · Dimension: goal-stop

Working tasks have a stop condition (`EXPIRE_WORKING_LEASES_SQL`,
`participant_liveness.rs:234-246`) but blocked tasks do not. `try_auto_dispatch`
parks a task in `blocked/waiting_agent` when no participant is free
(`orchestration.rs:398-411`) and `BlockedTaskPolicy` applies no TTL, so it stays
eligible indefinitely.

Honest correction (must fix before shipping): the original claim that reusing
`publish_task_update` makes the feed "emit a failed notification" is **false** —
`notifyTaskOwner` does not fire on this path. Any operator notification needs
explicit wiring, not a free side effect.

Change: a `BlockedTaskReaperWorker` modeled on `DependencyReconcileWorker` —
select `blocked` + `blocked_reason='waiting_agent'` + aged-out rows, flip to
`canceled` with `failure_code='waiting_agent_timeout'`, org-scoped by
construction; add the query-shape unit test (mirrors `dependency_reconcile.rs:
109-128`).

Validation: query-shape unit test (asserts `blocked_reason`, age predicate, no
cross-org leak); `#[sqlx::test]` on an aged blocked task; `cd rust && make ci`.

### 5.5 Durable dead-letter table + admin export

Severity: low · Effort: L · Risk: medium · Dimension: hitl

Permanently-rejected work is hard to audit after the fact. `event_consumer.rs:
800-804` ACKs `ConsumeError::Permanent` envelopes (forged signature, unknown
agent, malformed payload) with a `tracing::warn`; `job_queue` rows that exhaust
`max_attempts` move to `status='dead'` (`queue.rs:108`) with no API/UI surface.

Honest correction (premise partly false): both consumers **already** increment
reason-labeled Prometheus counters on the auth-drop cases, so these are _not_
invisible — the real gap is the absence of a _queryable, per-row_ record and an
operator escape hatch. Scope this down accordingly; it is the lowest-priority
item.

Change: idempotent migration `070_dead_events.sql` (+ MANIFEST + `pool.rs`)
creating `dead_events(id, organization_id, source, subject, agent_id, reason,
payload_excerpt, dropped_at)`; insert on Permanent/Unauthorized rejection; surface
via tenant-scoped `GET /admin/dead-letters` with a reason histogram.

Validation: schema-contract test; `#[sqlx::test]` for tenant scoping; `cd rust && make ci`.

---

## Suggested sequencing

1. Tier 1 first — `1.1` and `1.2` are the only two **high**-severity items and
   together turn the self-fix loop into an actual loop with an honest trigger
   plane. `1.3` unblocks live orchestrator observability the later tiers reuse.
2. Tier 2 next — the verifier is the highest-leverage loop-engineering dimension
   and the platform's thinnest. `2.1`–`2.3` are independent and parallelizable.
3. Tier 3 — accountability and bounds; `3.1`/`3.2` share the audit-sink work and
   should land together.
4. Tiers 4–5 — observability and runtime hardening; mostly additive, low risk,
   good parallel/junior work. `4.4` reuses `1.3`'s realtime path.

## What was rejected (and why it matters)

The adversarial pass dropped 11 candidates. Recording them prevents re-proposing:
some "gaps" were already solved (CI re-poll + typed transient errors already exist
in `github_app`; the result-loss case is already covered by the lease reaper;
credential/lease drops already have reason-labeled counters), and some "fixes"
were infeasible or harmful (`tokio::select!` cannot preempt a chosen branch's
body, so a "drain barrier" claim was false; rejecting under-leased assignments
mid-run fights the dispatch design; a clock-skew check on a loop that never reads
client time is impossible; image-integrity re-inspect duplicates Docker's
content-addressed pull). The lesson is the loop-engineering lesson: a proposal is
only as good as the verifier that can say no to it.

```

```
