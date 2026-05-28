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
- [NATS Subject Namespacing](../architecture/nats-subjects.md) — planned subject discriminator by runtime_kind.
