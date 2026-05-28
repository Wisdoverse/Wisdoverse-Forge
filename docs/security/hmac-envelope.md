# HMAC Envelope (Sidecar → Platform)

The sidecar signs result payloads (e.g., `orchestration.result.<agent-uuid>` NATS subject) with HMAC-SHA-256 before publishing. This document specifies the envelope schema, the platform's verification rules, and the replay-protection window.

## Envelope schema

Every signed message carries:

```json
{
  "nonce": "<uuidv4>",
  "ts": <unix-epoch-ms>,
  "body": <payload object>,
  "hmac": "<hex-encoded HMAC-SHA-256 over canonical (nonce ++ ts ++ body) using the per-agent secret>"
}
```

- `nonce` — fresh UUIDv4 per message; never reused.
- `ts` — sidecar's monotonic wall-clock at message creation, in milliseconds since the Unix epoch.
- `body` — domain payload (task result, evidence pointer, log batch, etc).
- `hmac` — `hmac_sha256(per_agent_secret, canonical(nonce, ts, body))`. Canonical form is `utf-8(nonce) ++ ":" ++ utf-8(format!("{}", ts)) ++ ":" ++ utf-8(serde_json::to_string(body))`.

## Verification rules

The platform's callout verifier MUST:

1. Look up the per-agent HMAC secret by agent_id (from the JWT identity).
2. Recompute the HMAC over `canonical(nonce, ts, body)`; constant-time-compare against the envelope's `hmac`.
3. Reject if `ts` is more than **5 minutes** (300 s) older than the platform's wall-clock, OR more than 60 s in the future. Skew tolerance MUST be configurable via `HMAC_ENVELOPE_SKEW_TOLERANCE_S` (default `60`).
4. Look up `nonce` in a per-agent replay-nonce store (Redis: `hmac:nonce:<agent_id>:<nonce>`) with a TTL matching the replay window. If present, reject as replay.
5. On accept, store `nonce` in the replay store with TTL `= replay_window_s`.

## Replay window

The replay window is **5 minutes (300 s)**. Operationally:

- Messages older than 5 min are rejected outright (Rule 3).
- Messages with a duplicate `nonce` within the 5-min sliding window are rejected as replay (Rule 4).
- Combined, this bounds the attacker's replay budget to messages they capture and replay within 5 min before the platform's GC sweeps the nonce store.

## Where this is enforced today

- Signing: `rust/bins/sidecar/...` (todo: link the exact module after sidecar refactor).
- Verification: `rust/crates/infra/src/nats.rs` callout policy (todo: link the exact function).
- Replay nonce store: Redis, key prefix `hmac:nonce:`, TTL = 300 s.

## Future work

- Replace the `hmac:nonce:` Redis keys with a streaming bloom-filter once message volume exceeds 10k/s per agent; document the false-positive bound (1 in 10^9).
- Sign the full TLS-encrypted payload (not just the JSON body) when the NATS transport switches from `tls://` connect-time to mTLS per-message.

## See also

- `docs/runbooks/nats-auth.md` for the per-agent NATS auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16 for the Host CLI threat model.
