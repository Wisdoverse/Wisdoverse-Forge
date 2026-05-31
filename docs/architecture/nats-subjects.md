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
orchestration.assigned.<runtime_kind>.<uuid> # phase 1c (stream pre-widened; producer/consumer/grants still 3-token)
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

## Phase 1c — `orchestration.assigned` (DEFERRED)

`orchestration.assigned` is the harder, platform → sidecar direction: the
per-agent JetStream **durable pull-consumer name** (`orch-assignment-<uuid>`),
its `filter_subject`, and the four `$JS.API.CONSUMER.*` grant subjects all embed
the agent UUID and the assignment subject, so namespacing it by kind forces a
per-agent durable recreation coupled to the callout grant strings. Its stream
subject was **pre-widened to `.>`** in phase 1b (behaviourally inert today —
the only assigned publisher still emits the 3-token subject, which `.>` still
captures), so the phase-1c change is a pure additive grant/parser/publish change
with no stream-config edit. Tracked in
`docs/superpowers/specs/host-cli-enrollment-deferred-tracking.md`.

## See also

- `docs/runbooks/nats-auth.md` — the auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16.5 + Platform C7.
