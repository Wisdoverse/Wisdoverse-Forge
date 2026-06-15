# ADR 0008 — Orchestration presence/liveness write scaling

## Status

Accepted (Phase 1). Phase 2 (Redis presence offload) is **implemented but
flag-gated dark** (`PRESENCE_REDIS_ENABLED`, default off) and should be **enabled
only after the measurement gate below fires** — it ships so the architecture is
in place and can be dark-launched on staging, not because the gate has fired.

## Context

Each agent's container sidecar publishes a NATS heartbeat to
`sidecar.<agent_id>.heartbeat` every `heartbeat_interval_secs` (default 30s).
`ParticipantLivenessWorker` (`rust/crates/jobs/src/participant_liveness.rs`)
consumes every beat and, before this ADR, ran two PostgreSQL writes **per beat,
unconditionally**:

1. `UPSERT_PARTICIPANT_SQL` — a CTE plus a correlated `EXISTS` against
   `orchestration_tasks` to recompute `busy`/`available`, an
   `INSERT … ON CONFLICT DO UPDATE` that always restamps
   `last_heartbeat_at = NOW()`, and `RETURNING *`.
2. `UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL` — `UPDATE agents SET status = …`
   that executed on every beat (only `updated_at` was gated on change, so the
   row was still rewritten each beat, producing a dead tuple every time).

Operationally this surfaced as recurring `slow statement` WARNs (2–16s) on the
shared external PostgreSQL host. Investigation found:

- The PostgreSQL host was ~3× oversubscribed (load ~35 on 12 cores) from
  co-tenant stacks. The slow statements were **the query being starved**, not a
  query whose own plan was expensive on the agentforge tables (which are tiny:
  `participants` ≈ 112 kB, 2 live rows).
- Even so, the per-beat write was **structurally wasteful**: it recomputed
  `busy`/`available` from a correlated subquery and rewrote two rows on every
  beat even when nothing changed, scaling linearly with agent count and adding
  avoidable write/WAL/vacuum pressure to the shared durable primary.
- Critically, `busy`/`available` is **already maintained event-driven** and the
  per-beat recompute is redundant reconciliation:
  - claim → `busy` (`claim_next_task_for_participant`),
  - task result (completed/failed) → `available`
    (`orchestration_result_consumer.rs`, inside the result-apply transaction),
  - lease expiry → recomputed (`RELEASE_PARTICIPANT_AFTER_LEASE_EXPIRY_SQL`).
    The one moment the recompute is load-bearing is **resurrection**: an agent
    marked `offline` by the stale sweeper that heartbeats back must return to
    `busy` (not `available`) if it still holds a `working` task, otherwise the
    auto-dispatcher could double-assign it. Assignment leases are 900s
    (`DEFAULT_ASSIGNMENT_LEASE_SECS`), far longer than the 90s offline window, so
    a returning agent legitimately still owns its task.

Redis is wired but used only for two narrow, fully-degradable features (CLI
OAuth `state`, context-resolver cache) plus a health PING. It is **optional**:
`RedisClient::new` returns a `None` connection on a missing URL or connect
failure and never aborts boot, and readiness never depends on Redis. No
presence/heartbeat/status data touches Redis today. PostgreSQL is the sole
durable source of truth for presence, and `EXPIRE_WORKING_LEASES_SQL` reads
`participants.status = 'busy'` to fail `agent_lost` leases — so presence cannot
move to an optional store without a durable, reconcilable backstop.

## Decision

Treat presence as **two separate signals with different durability needs**, and
fix the write amplification at the source before considering a new datastore.

### Phase 1 — slim the per-beat write to its load-bearing minimum (this ADR)

1. **Hot path writes only `last_heartbeat_at`.** Heartbeats become an
   UPDATE-first / INSERT-on-miss pair instead of a single upsert. The common
   case (`TOUCH_PARTICIPANT_SQL`) is a plain single-row `UPDATE` that restamps
   `last_heartbeat_at` + refreshes `name`/`capabilities` and **only recomputes
   `busy`/`available` when the row is currently `offline`** (resurrection), using
   a correlated subquery that PostgreSQL short-circuits via `CASE`. A
   steady-state beat is therefore a single-row update with no subquery. On a miss
   (first-seen, or a row hard-deleted under a live task), `INSERT_PARTICIPANT_SQL`
   derives the initial `busy`/`available` from the agent's `working` task — so a
   first beat can never leave a task-owning agent wrongly `available` — and its
   `ON CONFLICT` collapses a concurrent-first-beat race into a heartbeat touch.
   That subquery is paid once per participant lifetime, not per beat.
2. **`agents` status write is conditional.** Add
   `AND status IS DISTINCT FROM $3::agent_status` to
   `UPDATE_AGENT_STATUS_FROM_HEARTBEAT_SQL` so an unchanged agent row is not
   written at all (no new row version, no WAL, no dead tuple).
3. **A reconcile backstop replaces the per-beat self-heal.** The old per-beat
   recompute silently corrected a participant left `busy` after its task already
   left `working` — which happens when an event-driven release (task
   result/cancel/fail) fails on its best-effort, post-commit
   `participants.status = 'available'` write. With the per-beat recompute gone a
   continuously-heartbeating agent would never recover (it never goes `offline`,
   so never resurrects). `reconcile_orphaned_busy` runs in the existing 30s sweep
   tick (after lease expiry, before the drain) as one set-based
   `UPDATE … WHERE status = 'busy' AND NOT EXISTS (working task)`, restoring the
   self-heal at sweep cadence — O(participants) per sweep, not O(beats). It
   cannot race a live claim or release (both are single transactions that move
   task and participant together), so it only catches the genuinely-stranded
   row. A `participant_reconciled_total` counter makes the failure rate visible.
4. **Everything else is unchanged**: the 30s stale sweeper, lease expiry,
   event-driven claim/release, and all WS broadcasts remain the correctness
   backbone. Behaviour is preserved, including resurrection-to-`busy`.
5. **Attribution metrics** (gate input for Phase 2):
   - `agentforge_orchestration_participant_heartbeats_total` — every beat.
   - `agentforge_orchestration_participant_status_transitions_total` — beats
     that actually changed `agents.status` (the expensive minority).

The default `heartbeat_interval_secs` stays at 30s; widening it is an
orthogonal config lever (it trades offline-detection latency) and is not part
of this change.

### Phase 2 — Redis TTL presence offload (IMPLEMENTED, flag-gated dark)

The ephemeral liveness signal can move off the durable primary, gated by
`PRESENCE_REDIS_ENABLED` (default off). When the flag is on AND Redis is
connected (`presence_store::PresenceBackend`):

- A heartbeat is `SET af:presence:{agent} 1 EX <stale_after> GET`. The returned
  prior value distinguishes a steady-state beat (key existed → zero PostgreSQL,
  no broadcast, no auto-dispatch) from a transition (key absent → the Phase 1 PG
  write runs so `busy`/`available` + `last_heartbeat_at` are correct).
- PostgreSQL `participants` / `agents` remain the **durable, lease-relevant
  source of truth**, written only on real transitions (claim, result, lease
  expiry, and these resurrections) — never on a steady-state beat.
- The offline sweep uses **Redis key existence** (a pipelined `EXISTS` over
  non-offline participants) instead of `last_heartbeat_at`. The Phase 1 reconcile
  backstop for orphaned `busy` rows still runs.

Degradation (Redis optional): any missing connection or Redis error makes
`record`/`dead_agents` report unavailable and the worker uses the Phase 1
PostgreSQL path. Because `last_heartbeat_at` is not written on steady-state Redis
beats, a fallback would see stale timestamps; the backend therefore **grace-skips
the PG offline sweep for `stale_after`** after any fallback so PG-path beats
repopulate `last_heartbeat_at` before offline detection resumes. The
`agent_lost` lease sweeper (which reads `participants.status = 'busy'`) is
unaffected — `busy`/`available` never leaves PostgreSQL.

Trade-off: a steady-state beat skips not only the PG write but also the
per-beat auto-dispatch claim, so dispatch for an already-online idle agent is
bounded by the 30s `drain_available_participants` sweep instead of the agent's
next beat (both are ~30s, so worst-case dispatch latency is unchanged in order;
a task can never be left undispatched — drain still runs every tick).

Metrics: `presence_redis_steady_total` (the win), `presence_redis_transition_total`,
`presence_redis_fallback_total`, `presence_redis_errors_total{op}`.

**Enablement gate:** keep `PRESENCE_REDIS_ENABLED=false` until the Phase 1
attribution metrics (`participant_heartbeats_total` write rate, or its share of
PostgreSQL time in `pg_stat_statements`) show the per-beat write is a measured
top contributor at a realistic agent count — and the host-contention issue is
addressed. Enabling it while the proximate cause is host oversubscription
relocates the symptom. Enable on staging first; watch `presence_redis_fallback_total`
(should stay ~0) and confirm agents still go offline/online correctly before any
production change.

## Consequences

- **Steady-state cost drops from two unconditional row writes + a correlated
  subquery per beat to one single-row `last_heartbeat_at` update per beat**,
  with zero `agents` writes when status is unchanged (the common case). Write,
  WAL, and autovacuum pressure on the shared primary fall accordingly and scale
  better with agent count.
- **No behaviour change.** Resurrection still restores `busy`; event-driven
  transitions, the lease backstop, and WS broadcasts are untouched. Pinned-SQL
  tests are updated to assert the new shape, and behavioural tests cover the
  steady-state-skips-recompute and resurrection-recomputes cases.
- **The architecture is now staged.** Presence is explicitly modelled as an
  ephemeral signal over a durable, lease-relevant state, so the Phase 2 Redis
  offload (if the gate fires) is an additive, degrade-safe change rather than a
  rewrite.
- **Risk:** the conditional `agents` write means an unchanged row is no longer
  touched per beat; no consumer relies on that rewrite (`updated_at` was already
  change-gated). The resurrection branch is the one correctness-critical path
  and is covered by a dedicated test.

## References

- `rust/crates/jobs/src/participant_liveness.rs` — worker, SQL, tests.
- `rust/crates/jobs/src/orchestration_result_consumer.rs` — event-driven
  `busy → available` release on task result.
- `rust/crates/core/src/orchestration_protocol.rs` — `DEFAULT_ASSIGNMENT_LEASE_SECS`.
- ADR 0006 — SQLx migration policy (no schema change here; SQL-only).
