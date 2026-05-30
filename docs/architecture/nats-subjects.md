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
events.ingest.<runtime_kind>.<uuid>          # e.g. events.ingest.cli.<uuid>
orchestration.result.<runtime_kind>.<uuid>   # phase 1b (not yet shipped)
orchestration.assigned.<runtime_kind>.<uuid> # phase 1b (not yet shipped)
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

## Phase 1b — `orchestration.result` / `orchestration.assigned` (DEFERRED)

These are intentionally NOT namespaced in #457 phase 1 because their JetStream
streams use single-token wildcards (`orchestration.result.*`,
`orchestration.assigned.*`). A 4-token namespaced subject would not match, so
phase 1b must first widen those stream subjects to `.>` (a stream-config change)
before the producer/consumer/callout can move. Tracked in
`docs/superpowers/specs/host-cli-enrollment-deferred-tracking.md`.

## See also

- `docs/runbooks/nats-auth.md` — the auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16.5 + Platform C7.
