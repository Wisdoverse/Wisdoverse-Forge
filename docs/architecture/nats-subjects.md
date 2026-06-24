# NATS Subject Namespacing

The platform's per-agent NATS auth-callout binds each connection to subjects
scoped by `agent_id`. Since the runtime_kind redesign (#447), `agents.runtime_kind`
is a first-class discriminator (`container | cli | api`), so we additionally
namespace workload subjects by runtime kind for defense-in-depth and per-kind
observability (#457).

> Note: an earlier draft of this document listed the subjects as
> `agent.events.<uuid>` / `agent.results.<uuid>`. Those names were aspirational
> and never matched the code. The real subjects are below; this document has
> been reconciled against the source (`rust/bins/sidecar/src/publisher.rs`,
> `rust/crates/api/src/services/auth_callout/perms.rs`,
> `rust/crates/core/src/{event_protocol,orchestration_protocol}.rs`,
> `rust/bins/server/src/streams.rs`).

## Real subjects

| Subject (legacy)                      | Direction          | Producer / Consumer |
| ------------------------------------- | ------------------ | ------------------- |
| `events.ingest.<uuid>`                | sidecar → platform | `EventPublisher` / `EventStreamWorker` |
| `orchestration.result.<uuid>`         | sidecar → platform | sidecar / result consumer |
| `orchestration.assigned.<uuid>`       | platform → sidecar | orchestrator / sidecar |
| `sidecar.<uuid>.heartbeat`            | sidecar → platform | sidecar / liveness |
| `sidecar.<uuid>.cmd`                  | platform → sidecar | platform / sidecar |
| `creds.<uuid>`                        | sidecar → platform | sidecar / credential consumer |

`<uuid>` is the agent UUID, templated into the JWT `pub`/`sub` allowlists by the
auth-callout. The callout did not consider runtime_kind before #457.

## Namespaced shape

```
events.ingest.<runtime_kind>.<uuid>          # SHIPPED phase 1
orchestration.result.<runtime_kind>.<uuid>   # SHIPPED phase 1b
orchestration.assigned.<runtime_kind>.<uuid> # SHIPPED phase 1c (single-filter swap + dual-publish)
```

with `<runtime_kind> ∈ {container, cli, api}`. The subject taxonomy and its
parser live in `agentforge_core::event_protocol` (events) and
`agentforge_core::orchestration_protocol` (orchestration) so producers, the
callout, and consumers share one source of truth.

## Phase 1 — `events.ingest` (SHIPPED, #457)

`events.ingest` was migrated first because it required **no JetStream
reconfiguration**: the `EVENTS` stream already captures `events.ingest.>` and
the consumer filter is already `events.>`, so the 4-token namespaced subject
already lands in the stream and reaches the consumer.

What shipped:

1. **Callout grant** (`perms.rs`) — each agent's JWT is granted BOTH
   `events.ingest.<kind>.<uuid>` (its own kind) AND the legacy
   `events.ingest.<uuid>`. A `cli` agent is never granted a `container` subject
   (cross-kind isolation; covered by `agent_is_not_granted_other_kinds_event_subjects`).
   The callout reads `runtime_kind` in the same `agents` row as the connect
   password — no extra round-trip.
2. **Sidecar publish** (`publisher.rs`) — new agent containers publish on the
   namespaced subject **only**. We do NOT dual-publish: the `events` table has
   no delivery-id dedup, so emitting both shapes would double-insert every
   event. The HMAC is over `agent_id:ts:payload` and is independent of the
   subject, so the platform signature check is unaffected.
3. **Platform consume** (`event_consumer.rs`) — the parser accepts both shapes
   (shared `parse_events_ingest_subject`) and counts every legacy receipt as
   `agentforge_nats_legacy_subject_received_total{subject="events.ingest"}`.

### Deploy ordering (REQUIRED)

The platform must be deployed **before** agent images that contain the
namespaced-publishing sidecar. Standard order already satisfies this: roll the
control plane, then `make update-agents`. An old platform (no namespaced parser)
receiving a namespaced event would drop it as an unsupported subject —
telemetry loss, not a crash or security issue, but avoid it by keeping the
order.

### Legacy-drop criteria (later deploy, NOT in this PR)

Drop the legacy publish + legacy callout grant + legacy parser arm only after
`agentforge_nats_legacy_subject_received_total{subject="events.ingest"}` has held
at **zero in production** across a full agent-container turnover (no pre-#457
containers still running). That flip is a separate, post-observation change.

The consumer emits this series at `0` on startup, so the gate must check that the
series is **present AND equal to zero** — a `== 0` rule on an *absent* series
(crashed/never-run consumer) reads as no-data and would pass falsely. Also
confirm the consumer is genuinely receiving traffic (non-zero event throughput
into the `events` table / event-processing metrics) before flipping, so a silent
consumer outage isn't mistaken for a drained legacy tail.

## Phase 1b — `orchestration.result` (SHIPPED, #457)

Unlike `events.ingest`, the `ORCHESTRATION_RESULTS` JetStream stream and its
consumer filter both used the single-token wildcard `orchestration.result.*`,
which does not match a 4-token namespaced subject. Phase 1b widens both to `.>`.

What shipped (sidecar → platform result envelope):

1. **Stream + filter widen** — `result_subject_wildcard()` (and
   `assign_subject_wildcard()`, pre-widened for phase 1c) become `.>`. The same
   helper feeds both the stream subjects (`streams.rs`) and the result
   consumer's `filter_subject`, so they cannot drift. `create_or_update_stream`
   applies this as an **in-place** config update (verified against nats-server
   2.12; precedent: `CREDENTIALS` already runs WorkQueue + `creds.>`). `.>` is a
   strict superset of `.*`, so already-stored 3-token messages stay valid.
2. **Durable filter migration (one-time)** — widening the consumer filter in
   code is NOT enough: `get_or_create_consumer` returns an EXISTING durable
   unchanged (empirically verified — the live `orchestration-result-handler`
   keeps its `.*` filter). So the consumer bootstrap detects a stale filter and
   **deletes + recreates** the durable. Safe: it is the single shared platform
   consumer (not per-agent), recreated once at deploy; unacked WorkQueue
   messages remain for redelivery.
3. **Callout grant** — each agent is granted BOTH
   `orchestration.result.<kind>.<uuid>` (its own kind) and the legacy
   `orchestration.result.<uuid>`; cross-kind isolation as in phase 1.
4. **Sidecar publish** — namespaced-only (the result has `delivery_id` dedup, so
   no double-insert risk, but the durable result outbox retries any publish that
   fails before the stream is widened — a result is delayed, never lost, on
   deploy-order skew, so dual-publish is unnecessary churn).
5. **Consumer** — `parse_result_subject` accepts both shapes; the trailing UUID
   remains the authoritative identity (envelope + payload cross-checks + HMAC).
   Legacy receipts count as
   `agentforge_nats_legacy_subject_received_total{subject="orchestration.result"}`,
   materialised at 0.

Deploy ordering (REQUIRED): control plane before agent images, same as phase 1.
Legacy-drop for `orchestration.result` is gated on the drain metric holding at
**present-AND-zero** across a full container turnover — a later post-observation
deploy, not this PR.

## Phase 1c — `orchestration.assigned` (SHIPPED, #457)

`orchestration.assigned` is the platform → sidecar direction, delivered through a
**per-agent JetStream durable pull consumer** (`orch-assignment-<uuid>`) whose
`filter_subject` and `$JS.API.CONSUMER.CREATE` grant embed the assignment
subject. Its stream subject was pre-widened to `.>` in phase 1b, so no
stream-config change is needed.

What shipped (additive, zero-outage; legacy-drop is a later deploy):

1. **Single filter, swapped in place.** The sidecar binds its per-agent durable
   to a SINGLE `filter_subject` = `orchestration.assigned.<kind>.<uuid>` (NOT
   `filter_subjects` plural). On nats-server 2.10+, `create_consumer_on_stream`
   (CreateOrUpdate) swaps an existing durable's legacy filter to the namespaced
   one **in place** — no delete/recreate, in-flight un-acked assignments survive
   (empirically verified on 2.12). A per-agent durable delete+recreate is unsafe
   (WorkQueue's no-overlap rule `10100` blocks a parallel durable; an orphaned
   one keeps draining with no live consumer).
2. **Single filter is a security boundary — never `filter_subjects` plural.**
   The `$JS.API.CONSUMER.CREATE.<stream>.<durable>.<filter>` grant embeds the
   filter token, so NATS only lets a sidecar create a consumer filtering its OWN
   subject. The multi-filter form needs a **filter-LESS** CREATE grant
   (`...CREATE.<stream>.<durable>`), which was shown live to let a rooted sidecar
   create a consumer under its own durable name but filtering ANOTHER agent's
   subject — draining that agent's WorkQueue assignments. The callout grants both
   the legacy and namespaced **single-filter** CREATE subjects (kind-scoped);
   INFO/NEXT/ACK embed only the durable name and are unchanged.
3. **Platform dual-publishes** each assignment (same `delivery_id`) to BOTH
   `orchestration.assigned.<uuid>` and `orchestration.assigned.<kind>.<uuid>`.
   Required because a single-valued consumer filter matches only one shape and
   the platform can't know which image (legacy/new) an agent runs. The sidecar's
   `AssignmentInbox` dedups by `delivery_id` **before** the CLI runs, so a
   consumer that ever matches both executes the task only once. The invariant:
   ONE stable `delivery_id` per logical assignment across both shapes.
4. **Kind threading.** `TaskAssignment` carries an optional `runtime_kind`,
   populated at enqueue time on the hot auto-dispatch path (one indexed lookup in
   the claim tx); the cold API-dispatch path leaves it `None` and the outbox
   publisher resolves it once (fallback `Container`).

Deploy ordering: backend (grants both + dual-publishes) before agent images
(swap to the namespaced filter). Legacy-drop for `orchestration.assigned` is a
later deploy gated on jsz showing all `orch-assignment-*` durables on namespaced
filters AND `agentforge_orchestration_assignment_kind_fallback_total` holding at
zero (a non-zero fallback means a kind couldn't be resolved and an assignment
would be mis-routed once the legacy copy is dropped).

Known follow-up (pre-existing, not introduced here): if a sidecar's one-shot
durable bind fails, the subscriber disables assignment intake for the container
lifetime (no retry/metric). The in-place filter swap is one more thing that can
transiently fail during rollout; remediation today is to restart the container.
Tracked for a bounded-retry + bind-error metric follow-up.

## See also

- `docs/runbooks/nats-auth.md` — the auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16.5 + Platform C7.
