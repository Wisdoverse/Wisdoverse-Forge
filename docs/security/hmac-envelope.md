# HMAC Envelope (Sidecar → Platform)

The sidecar signs the payloads it publishes to the platform (task results,
credential syncs, and hook events) with HMAC-SHA-256 before publishing to NATS.
This document specifies the envelope schema **as implemented**, the platform's
verification rules, and the replay-protection window.

> Reconciled with the code in issue #458. The original #450 draft described a
> `nonce` (uuidv4) field and a Redis `hmac:nonce:` store. Neither shipped: the
> wire envelope carries a `signature` over the canonical message, and replay is
> bounded by a ±5-min timestamp window plus, on the one path that records an
> authorization decision, a persisted `delivery_id` dedup. The sections below
> describe what actually runs.

## Envelope schema

Every signed message is the same struct in
`rust/crates/core/src/orchestration_protocol.rs` (`SignedEnvelope`), and the
event path uses a structurally identical `SignedEventEnvelope` in
`rust/crates/jobs/src/event_consumer.rs`:

```json
{
  "payload": <payload object>,
  "timestamp": <unix-epoch-seconds>,
  "agent_id": "<agent uuid>",
  "signature": "<hex HMAC-SHA-256 over agent_id ++ \":\" ++ timestamp ++ \":\" ++ payload>"
}
```

- `payload` — the domain body (task result, credential file map, or
  `{ "event_type", "data" }` for events).
- `timestamp` — the sidecar's wall clock at message creation, in **seconds**
  since the Unix epoch (not milliseconds).
- `agent_id` — the publishing agent's UUID, as a string. Must match the
  `agent_id` embedded in the NATS subject.
- `signature` — `hex(hmac_sha256(per_agent_secret, canonical))` where the
  canonical form is `format!("{agent_id}:{timestamp}:{payload}")` and `payload`
  is `serde_json::Value`'s default (BTreeMap-ordered, no `preserve_order`)
  string form. Producer (`rust/bins/sidecar/src/publisher.rs`,
  `EventPublisher::sign`) and verifier (`SignedEnvelope::verify`) compute the
  identical bytes.

There is **no separate `nonce` field**. The per-message uniqueness that the
result path needs comes from the domain payload's `delivery_id` (see below),
not from an envelope-level nonce.

### The per-agent secret

The secret is generated at container spawn (`agent_container_control.rs`),
stored on `agents.hmac_secret` (migration 025), and handed to the sidecar as
the `HMAC_SECRET` environment variable. The platform verifiers look it up by
`agent_id` and never serialize it (`#[serde(skip_serializing)]` on the domain
type).

## Verification rules

Each consumer that ingests a signed envelope MUST, in order:

1. Parse `agent_id` from the NATS subject and reject the message if the
   envelope's `agent_id` does not match (`envelope_agent_mismatch`). This stops
   an agent that controls one secret from speaking for another agent.
2. Reject if `timestamp` drifts more than **±5 minutes** (300 s) from the
   consumer's wall clock (`timestamp_outside_window`). The bound is symmetric:
   it is a skew tolerance, not an expiry, so the same window applies to both
   stale and future timestamps.
3. Look up the per-agent HMAC secret by `agent_id`. A missing row, NULL secret,
   or pre-migration agent all map to a uniform `agent_unknown` rejection.
4. Recompute the HMAC over the canonical form and constant-time-compare against
   `signature` (`hmac` crate's `verify_slice`). Reject on mismatch
   (`signature_mismatch`). The domain payload is parsed only **after** the
   signature passes, so unauthenticated input never reaches deserialization of
   the inner type.
5. For the orchestration-result path only: dedup by `delivery_id` (rule in the
   next section).

## Replay window and dedup

The replay window is **5 minutes (300 s)**, enforced by rule 2 on all three
paths via the shared `TIMESTAMP_REPLAY_WINDOW_SECS = 300` constant. A captured
envelope can only be replayed inside that window before its timestamp ages out.

Within the window, the three paths differ in whether they additionally dedup:

- **Orchestration result** — records an authorization-relevant decision
  ("task X succeeded/failed with this evidence"). A within-window replay must
  **not** re-apply, so the writer inserts the payload's `delivery_id` into
  `orchestration_inbox` with `ON CONFLICT (delivery_id) DO NOTHING` inside the
  same transaction as the task update. A duplicate `delivery_id` short-circuits
  before the task row is touched, bumping
  `agentforge_orchestration_inbox_duplicate_total`.

  This DB-dedup is **equivalent-or-stronger** than the originally-specced Redis
  `hmac:nonce:` store: the dedup key is persisted in Postgres (it survives a
  consumer restart, unlike an in-memory LRU, and outlives the 5-min window so a
  redelivery long after the window still no-ops), it is written in the same
  transaction that applies the result (so dedup and apply can never diverge),
  and it adds no second datastore to operate or keep consistent.

- **Credential sync** — the upsert into `user_cli_credentials` is keyed on
  `(user_id, cli_tool)` and is idempotent (last-write-wins on identical
  content). A within-window replay re-writes the same encrypted blob. The
  ts-window is the intended replay bound for this path; there is no `delivery_id`
  and no dedup store, by design.

- **Event ingest** — events carry no `delivery_id`. Their effects are
  idempotent by content: the `agents` runtime patch (`status`, `current_tool`,
  `cwd`) is last-write-wins, and the `events` table is an append-only telemetry
  log, not an authorization decision. A within-window replay re-appends an
  already-true telemetry row and re-applies an identical patch; it cannot record
  old-evidence success for new-code work the way the result path could. The
  ts-window is the intended replay bound for this path. Adding a dedup key would
  require plumbing a unique message id through the sidecar publisher, the wire
  schema, and the `events` table — tracked separately, not bolted on here.

## Per-path coverage (where this is enforced today)

| Path                                                  | Module / fn                                                                                                              | HMAC verify      | ±5-min ts-window | Per-message dedup                 |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------- | ---------------- | --------------------------------- |
| Orchestration result (`orchestration.result.<agent>`) | `rust/crates/jobs/src/orchestration_result_consumer.rs` → `handle_message_with_subject_prefix` + `SqlxTaskWriter::apply` | yes              | yes              | yes — `delivery_id` `ON CONFLICT` |
| Credential sync (`creds.<agent>`)                     | `rust/crates/jobs/src/credential_consumer.rs` → `handle_message`                                                         | yes              | yes              | n/a — idempotent upsert           |
| Event ingest (`events.ingest.<agent>`)                | `rust/crates/jobs/src/event_consumer.rs` → `EventConsumer::handle`                                                       | yes (added #458) | yes (added #458) | n/a — idempotent telemetry        |

Signing happens in `rust/bins/sidecar/src/publisher.rs` (`EventPublisher`) for
events, `rust/bins/sidecar/src/orchestration.rs` for results, and
`rust/bins/sidecar/src/credentials.rs` for credential syncs — all via the same
canonical form.

### Issue #458: event-ingest parity

Before #458, `event_consumer.rs` deserialized the `signature` and `timestamp`
fields but **never used them** — no HMAC verify, no ts-window, no dedup. A party
able to publish to `events.ingest.<agent_id>`, or anyone who captured and
replayed a signed event frame, could forge agent telemetry and runtime state.
#458 brought the event path to HMAC + ts-window parity with the other two
consumers (reusing the same secret, the same canonical-form verify, and the same
`TIMESTAMP_REPLAY_WINDOW_SECS`). Rejections increment
`event_ingest_unauthorized_total{reason}`.

## Future work

- If a stronger replay guarantee is ever required for the event path
  (exactly-once telemetry rather than at-least-once-within-window), plumb a
  unique `delivery_id` through the sidecar `EventPublisher`, the
  `SignedEventEnvelope` schema, and an `events` dedup key, mirroring the
  orchestration-result `orchestration_inbox` pattern. Track as a scoped issue
  before building.
- Sign the full TLS-encrypted payload (not just the JSON body) when the NATS
  transport switches from `tls://` connect-time to mTLS per-message.

## See also

- `docs/runbooks/nats-auth.md` for the per-agent NATS auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16 for the
  Host CLI threat model.
