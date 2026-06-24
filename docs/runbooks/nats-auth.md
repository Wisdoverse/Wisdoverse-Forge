# NATS authentication — per-agent callout (issue #38 phase 2)

## What changed

**Phase 1 (shipped earlier):** single shared `NATS_TOKEN` split into three role-scoped users (`backend`, `agent`, `sys`) across two accounts (`AGENTFORGE`, `SYS`). Every sidecar still used the same shared `agent` password — a compromised container could publish/subscribe against any other agent's per-UUID subjects.

**Phase 2 (this change):** the shared `agent` user is removed. Every spawned sidecar now authenticates as its own identity via a NATS 2.12+ auth callout service embedded in the Rust API. The callout validates a per-container `(agent_uuid, nats_connect_password)` pair against the `agents.nats_connect_password` column (populated by `start_agent`) and mints a short-lived (15 min) User JWT whose publish/subscribe allowlists mention only the caller's own `<agent_uuid>` in every subject segment.

Cross-agent spoofing — agent A publishing to `orchestration.result.<B>` or subscribing to `orchestration.assigned.<B>` — is now rejected at the NATS layer rather than relying on issue #39's HMAC envelope verification alone.

## Architecture

```
Sidecar                NATS 2.12+                  API (AuthCalloutWorker)
  │                        │                               │
  │── CONNECT nats://      │                               │
  │     <uuid>:<pw>@       │                               │
  │     nats:4222 ─────────►                               │
  │                        │── xkey-encrypt + publish      │
  │                        │   $SYS.REQ.USER.AUTH ─────────►
  │                        │                               │── SELECT nats_connect_password
  │                        │                               │   FROM agents WHERE id = $1
  │                        │                               │── constant-time compare
  │                        │                               │── sign User JWT (scoped to UUID)
  │                        │                               │── sign AuthorizationResponse
  │                        │                               │── xkey-seal to server ephemeral
  │                        ◄───────────────────────────────┤
  │                        │   inner User JWT binds:       │
  │                        │     pub.allow = [             │
  │                        │       events.ingest.<uuid>,   │
  │                        │       sidecar.<uuid>.heartbeat,│
  │                        │       orchestration.result.<uuid>,│
  │                        │       _INBOX.>                │
  │                        │     ]                         │
  │                        │     sub.allow = [             │
  │                        │       sidecar.<uuid>.cmd,     │
  │                        │       orchestration.assigned.<uuid>,│
  │                        │       _INBOX.>                │
  │                        │     ]                         │
  ◄── CONNECT ok ──────────┤                               │
```

## Secrets

Three accounts, each with its own user. All secrets live in `docker/.env` (git-ignored).

| Secret                                  | Consumer                     | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `NATS_BACKEND_PASSWORD`                 | Rust API + jobs              | Workload account user; unrestricted within AGENTFORGE. Bypasses callout (listed in `auth_users`).                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `NATS_AUTH_SERVICE_PASSWORD`            | Rust API (AuthCalloutWorker) | AUTH-account service user; receives `$SYS.REQ.USER.AUTH` requests.                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `NATS_SYS_PASSWORD`                     | Monitoring + KICK            | SYS-account user; used by `AuthCalloutWorker::revoke` to publish `$SYS.REQ.SERVER.<name>.KICK`.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `NATS_CALLOUT_ISSUER_SEED`              | API only                     | Ed25519 nkey seed. Signs outer AuthorizationResponse JWTs. Public half → NATS `authorization.auth_callout.issuer`.                                                                                                                                                                                                                                                                                                                                                                                                               |
| `NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED` | API only                     | Ed25519 nkey seed. Signs the inner User JWT. In server-config / non-operator mode the outer AuthorizationResponse signature (verified against `authorization.auth_callout.issuer`) is the sole trust anchor — the inner JWT's `iss` is informational and this key's public half does **not** appear in `nats.conf`. Issue #55 tracked the original bug where an earlier version of this runbook recommended placing the public half under `accounts.AGENTFORGE.signing_keys`, which `nats-server` rejects in server-config mode. |
| `NATS_CALLOUT_XKEY_SEED`                | API only                     | Curve25519 XKey seed. Decrypts callout request payloads; encrypts responses. Public half → NATS `authorization.auth_callout.xkey`.                                                                                                                                                                                                                                                                                                                                                                                               |
| `NATS_CALLOUT_ISSUER_PUBLIC`            | NATS only                    | Matches `NATS_CALLOUT_ISSUER_SEED`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `NATS_CALLOUT_XKEY_PUBLIC`              | NATS only                    | Matches `NATS_CALLOUT_XKEY_SEED`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `NATS_SERVER_NAME`                      | Both                         | Must match nats.conf `server_name`. Used in `$SYS.REQ.SERVER.<name>.KICK` subject for targeted revocation.                                                                                                                                                                                                                                                                                                                                                                                                                       |

> **Account placement** — the account the minted user lands in is controlled by the inner User JWT's `aud` claim, which the Rust API sets to the string `"AGENTFORGE"` (hardcoded in `bins/server/src/main.rs`, matched against the `accounts { AGENTFORGE { … } }` label in `docker/nats.conf`). `nats-server` in server-config mode resolves this via `s.LookupAccount(aud)` — passing an account public nkey here would fail with `no valid account "A…" for auth callout response`.

Private seeds never reach the NATS container; only the public halves are templated into `nats.conf`.

### Generating the key material

NATS ships a `nk` CLI that generates the key material. Install it once (ships with `nats-server`) and run per-environment:

```bash
# Issuer key (signs outer AuthorizationResponse JWTs)
nk -gen account > /tmp/nats-issuer.seed
NATS_CALLOUT_ISSUER_SEED=$(cat /tmp/nats-issuer.seed)
NATS_CALLOUT_ISSUER_PUBLIC=$(nk -inkey /tmp/nats-issuer.seed -pubout)

# Account signing key (signs inner User JWTs — only the seed is needed;
# the public half is not wired into `nats.conf` in server-config mode).
nk -gen account > /tmp/nats-sk.seed
NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED=$(cat /tmp/nats-sk.seed)

# XKey (encrypts callout request/response transport). Current nk builds expose
# this key type as "curve" or "x25519"; older examples may call it "xkey".
nk -gen curve > /tmp/nats-xkey.seed
NATS_CALLOUT_XKEY_SEED=$(cat /tmp/nats-xkey.seed)
NATS_CALLOUT_XKEY_PUBLIC=$(nk -inkey /tmp/nats-xkey.seed -pubout)

# Passwords
NATS_BACKEND_PASSWORD=n$(openssl rand -hex 32)
NATS_AUTH_SERVICE_PASSWORD=n$(openssl rand -hex 32)
NATS_SYS_PASSWORD=n$(openssl rand -hex 32)

# Server name (must match nats.conf)
NATS_SERVER_NAME=agentforge-primary
```

The `n` prefix is intentional: `docker/nats.conf` expands these values as
unquoted NATS config variables, and values beginning with digits can be parsed
as numbers instead of strings.

Delete the `/tmp/*.seed` files after copying into the secret store; the seed is the private half and must never be committed.

## Rolling it out (first-time deploy)

Drain in-flight tasks before flipping the NATS config, avoiding the window where
a sidecar mid-task loses its JWT to the reload.

```bash
# 1. Put all secrets in docker/.env (see "Generating the key material" above).

# 2. Verify there are no in-flight orchestration tasks before restarting.
psql "$DATABASE_URL" \
  -c "SELECT count(*) AS working_tasks FROM orchestration_tasks WHERE status = 'working';"

# 3. Validate and restart the stack.
docker compose --env-file docker/.env -f docker/compose.yml config -q
docker compose --env-file docker/.env -f docker/compose.yml up -d --force-recreate nats agentforge-server
```

Abort if `working_tasks` is non-zero unless you have a separate drain window.
Always run `docker compose config -q` before touching live containers.

- Restarts NATS first, waits for healthcheck (30s budget).
- Restarts the API, waits for healthcheck (60s budget).
- Greps API logs for `Auth callout worker listening` as the smoke check.

## Verification

After cutover, confirm per-agent subject isolation at the NATS layer.

**Spin an agent, then from inside its container try to publish to a DIFFERENT agent's result subject:**

```bash
# Inside a sidecar container (e.g. via `docker exec -it agentforge-agent-<A> sh`):
nats pub -s "$NATS_URL" "orchestration.result.<SOME-OTHER-UUID>" '{}'
# Expected: nats: error: nats: Permissions Violation for Publish to "orchestration.result.<SOME-OTHER-UUID>"
```

And to its own subject (should succeed):

```bash
nats pub -s "$NATS_URL" "orchestration.result.$AGENT_ID" '{"status": "test"}'
# Expected: Published 16 bytes
```

**Confirm the callout counter stays at zero under normal load:**

```bash
curl -s http://localhost:4003/metrics | grep nats_auth_callout_unauthorized_total
# Expected: no samples, or samples with value 0. Any non-zero rate is a security event.
```

## Rotation

Rotating any callout secret does NOT require restarting agent containers — agents reconnect automatically when their 15-min JWT expires, and the next CONNECT goes through the callout under the new key material.

```bash
# Rotate the three callout seeds together (they are independent, but
# rotating together is simpler):
./regenerate-callout-seeds.sh > new-seeds.env  # operator-provided helper

# Update docker/.env with the new SEED + PUBLIC values.

# Restart NATS + API in order:
docker compose up -d --force-recreate nats
docker compose up -d --force-recreate agentforge
```

Agents whose JWTs expire during the NATS restart window reconnect under the new key material automatically. No `docker restart <every-agent>` step is required — this is the primary rotation improvement over phase 1.

Rotating `NATS_BACKEND_PASSWORD`, `NATS_AUTH_SERVICE_PASSWORD`, or `NATS_SYS_PASSWORD` still requires a NATS restart (the passwords are expanded into `nats.conf` at load time) but does NOT require restarting agents.

## Targeted revocation

Stopping an agent triggers a two-step revocation in `containers.rs::stop_agent`:

1. **DB clear** — `AgentRepository::clear_container` NULLs `nats_connect_password` FIRST, then `hmac_secret`, then `container_id`. Any reconnect attempt after this point fails the callout (password not found → uniform deny).
2. **Active KICK** — `AuthCalloutWorker::revoke(agent_id)` publishes on `$SYS.REQ.SERVER.<name>.KICK` with the `(server_id, cid)` pair recorded when the JWT was issued. Targeted revocation window is ≤2s (the KICK is a NATS system RPC).

If the API crashes between steps 1 and 2, an orphaned JWT lives at most 15 min (TTL) before expiring naturally. The DB clear alone is sufficient for correctness; the KICK is a latency optimization.

## Debugging dropped events: dead-letter capture

When the backend consumer permanently rejects an inbound envelope — bad HMAC
signature, unknown agent, bad subject, stale timestamp, or a malformed body — it
`Term`-drops the message. Before this feature the raw message vanished with only
a log line and a `*_unauthorized_total` counter increment, so an operator asking
"why aren't agent X's events showing up?" had nothing durable to inspect.

The consumers now record each permanent drop to the `dead_events` table. One row
per drop carries:

- `source` — `events.ingest` or `orchestration.result`.
- `reason` — the structured drop reason, e.g. `signature_mismatch`,
  `agent_unknown`, `bad_subject`, `timestamp_outside_window`.
- `subject` — the NATS subject. **This carries the agent UUID**, which is the
  real key for "which agent is dropping?" — most drops are pre-authentication, so
  `org_id` is `NULL`.
- `detail` — short human context built at the reject site.
- `payload_excerpt` — a truncated (<= 8 KiB) excerpt of the raw dropped message.
- `recorded_at` — drop time (the list is newest-first).

What is **not** recorded: transient/retryable errors (they redeliver and usually
succeed) and the `orchestration_inbox` dedup hit (a deduped replay was handled
successfully — that is idempotency working, not a drop). Recording is
best-effort: if the `dead_events` INSERT fails, the consumer still `Term`s the
message and logs a warning. That failure is **not** silent — it increments
`dead_event_record_errors_total{source}` (primed to zero at startup) and logs at
`error!` level. A rising value means the `dead_events` table is broken or
missing and drops are **not** being captured even though the reader looks empty;
treat an empty table plus a non-zero error counter as "capture is down", not "no
drops". A lost dead-letter row never blocks or crashes the consumer.

One more **known limitation**: a TRANSIENT error that _never_ succeeds and
exhausts the consumer's `max_deliver` redeliveries is dropped by the JetStream
broker without ever reaching the consumer's terminal (`Term`) path, so **no
dead-letter row is written** for it — the consumer simply never sees a terminal
event. Such losses are not invisible: they show up as a rising
`*_transient_errors_total` counter. If that counter climbs while `dead_events`
stays flat, look for a stuck-transient (e.g. a DB outage) exhausting redelivery,
not a permanent drop.

### How to read it

`GET /api/v1/admin/dead-events` returns the cross-org list, newest first,
paginated:

```bash
# Newest 25 drops (any reason). A platform-admin JWT is required
# (the caller's users.is_admin must be true — see "Access and safety").
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:4003/api/v1/admin/dead-events?page=1&limit=25"

# Filter to one reason, e.g. forged signatures.
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:4003/api/v1/admin/dead-events?reason=signature_mismatch"
```

Response shape: `{ "ok": true, "data": { "items": [...], "total", "page",
"totalPages" } }`. Each item is the row above in camelCase
(`payloadExcerpt`, `recordedAt`, …).

### Access and safety

- **Platform-admin-only.** The reader is gated on the server-side
  `users.is_admin` column (a **platform admin**, NOT the per-org `owner`/`admin`
  membership role). This distinction matters: the JWT `role` claim is the per-ORG
  membership role, and a self-registered user is `owner` of their own personal
  org — gating on the claim would let any registered user read the cross-org
  table. `users.is_admin` defaults to `false` and is only settable by an existing
  admin, so it is not self-assignable. The view is cross-org by design (auth
  drops have no trustworthy org), so any non-platform-admin caller gets `403`.
- **`payload_excerpt` is UNTRUSTED.** It is an attacker- or work-controlled
  excerpt of the dropped message (a forged/stale `SignedEnvelope`, or for a
  `bad_payload` drop, real task `stdout`/`stderr`). It is **not** a secret leak
  (the envelope carries only the HMAC digest, never the per-agent key), but it
  **may contain task output** and is stored-XSS-capable: any UI **must render it
  as escaped plain text**. The 8 KiB cap bounds table growth from a flooding or
  oversized payload.
- **No TTL prune yet.** A sustained drop flood signals an attack or
  misconfiguration; the `recorded_at` index keeps a future prune reaper cheap,
  but it is deferred until volume justifies it.

## What's NOT fixed by this phase

- **Sidecar compromise within its own UUID** — a compromised sidecar can still spoof its own `events.ingest.<self>`, `orchestration.result.<self>`, or `sidecar.<self>.heartbeat`. Issue #39's HMAC envelope signing catches forged results; heartbeat spoofing has low blast radius.
- **TLS client certificates** — connections are plaintext over TCP (`nats://`). TLS + client certs would layer on top of callout without conflict; tracked separately if/when the deployment model demands it.
- **Multi-region NATS gateway** — this phase assumes a single NATS cluster. Gateway federation requires operator-mode JWT hierarchy instead of callout; out of scope.

## Cross-reference

- `docker/nats.conf` — source of truth for permissions and the auth_callout block.
- `docker/compose.yml` — env wiring + required env vars.
- `rust/crates/core/src/config.rs` — `AppConfig.nats_callout: NatsCalloutConfig`.
- `rust/crates/api/src/services/auth_callout/` — worker, JWT, XKey, perms, kick.
- `rust/crates/api/src/routes/containers.rs` — per-agent NATS_URL interpolation.
- `rust/crates/api/src/repositories/agent.rs` — `set_container` / `clear_container` atomic lifecycle.

## Related design docs

- [HMAC Envelope](../security/hmac-envelope.md) — signed result envelope schema + replay window.
- [NATS Subject Namespacing](../architecture/nats-subjects.md) — runtime_kind subject discriminator. Phase 1 (`events.ingest.<kind>.<uuid>`) is live: the callout grants each agent BOTH the kind-namespaced and legacy ingest subjects, and the platform exports `agentforge_nats_legacy_subject_received_total`. Operators must keep deploying the control plane **before** agent images, and the legacy-drop deploy is gated on that metric holding at zero.
