# Agent-runtime console warnings — fix plan

Status: **Design / for review** (2026-06-14). Fixes the three warnings an operator
sees in an agent's in-app console. The terminal itself is healthy; these are the
agent **container** startup messages surfaced by the working terminal.

> **Review history.** Codex review (2026-06-14) returned two P2 findings, both
> incorporated: (1) `publisher.publish` is **not** WAL-durable, so Fix B must wire
> WAL or relayed events still drop on NATS outage; (2) `sys_password`-present ≠ KICK
> works, so Fix C must **not** raise the TTL on config-presence. A **second** Codex
> pass added (3) WAL is drained only once at startup — no reconnect/periodic drain —
> so append-on-failure alone doesn't survive a *reconnect*; Fix B must also add a
> reconnect/periodic WAL drain. Net effect: B = relay listener **+ append-on-failure
> + drain-on-reconnect** (the real durability fix); C reduces to **no change**
> (15-min TTL kept, churn made harmless by a fully-durable B; TTL-raise deferred
> behind a KICK self-probe). The fix is a single agent-image deploy (A + B).

## Problem & evidence

Opening an agent's console shows (verbatim, from a live staging agent):

```
WARNING: AGENTFORGE_DEVENV_POLICY is not set — Docker proxy will not start
WARNING: Docker commands (docker run, docker compose) will fail inside this container
WARNING: This usually means the server did not inject the policy — check session.service.ts
... Sidecar relay socket not ready after 5s — events will be lost
```

Decoded against the code:

| # | Symptom | Root cause | Severity |
| - | ------- | ---------- | -------- |
| **A** | `DEVENV_POLICY not set` + dead `session.service.ts` ref | docker-in-agent is **opt-in**; unset is the normal state. The message is alarming and cites a TypeScript file that no longer exists (legacy port artifact). | Cosmetic |
| **B** | `relay socket not ready — events will be lost` | **Real defect.** `hooks/agentforge-relay-hook.cjs` writes CLI hook events to the unix socket `/tmp/agentforge-relay.sock`, but **no component in the Rust tree binds it** (the only `UnixListener` in `rust/` is the unrelated buildx-plugin). Hook events are silently dropped. Confirmed live: the socket file does not exist in a running agent. | Real (event loss) |
| **C** | (sidecar log, not the box) `nats: User Authentication Expired` every ~15 min → reconnect | Per-agent NATS JWT TTL is 15 min (`DEFAULT_JWT_TTL`), deliberately short as a **revocation fallback**. A long-lived agent therefore churns its NATS connection every 15 min. | Low (churn) |

All three are agent-container/sidecar/callout concerns, **not** `agentforge-server`.
Fixing A+B requires rebuilding the **agent base image** (`make build-agent-base`)
and respawning agents; fixing C is a **server** change (agents just receive
longer-lived tokens).

## Fix A — reword the DEVENV message (cosmetic, no behavior change)

**File:** `docker/scripts/agent-entrypoint.sh` (the `else` branch, ~lines 733-735).

Replace the three `WARNING` lines with one neutral line:

```sh
# OLD (lines 733-735)
  echo "agent-entrypoint: WARNING: AGENTFORGE_DEVENV_POLICY is not set — Docker proxy will not start"
  echo "agent-entrypoint: WARNING: Docker commands (docker run, docker compose) will fail inside this container"
  echo "agent-entrypoint: WARNING: This usually means the server did not inject the policy — check session.service.ts"

# NEW
  echo "agent-entrypoint: Docker-in-agent (docker proxy) is not enabled for this agent — this is normal unless the agent needs to run docker commands"
```

No conditional/behavior change; the docker proxy still does not start when the
policy is unset. Only the echoed text + tone change. Removes the dead
`session.service.ts` reference.

## Fix B — bind the relay socket in the sidecar (the real bug)

**Wire protocol (read from `hooks/agentforge-relay-hook.cjs`):** the hook connects
to `AGENTFORGE_RELAY_SOCKET` (default `/tmp/agentforge-relay.sock`), writes a
**4-byte big-endian length header** then the UTF-8 JSON event, then closes. It
expects **no ack** (resolves on write completion), 2000 ms timeout, no retry
(one-shot, short-lived process). The event is a normalized object
(`{ schemaVersion, id, timestamp, type, sessionId, cwd, runtimeId, cliTool,
sourceType, ... }`).

**Verified publisher signature** (`rust/bins/sidecar/src/publisher.rs`):
```rust
pub async fn publish(&self, event_type: &str, payload: serde_json::Value)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
```
It applies the per-agent HMAC envelope + NATS subject, so a relayed hook event
takes the **same** trusted path as native sidecar events for the signing/subject.

> **Codex correction (durability):** `publish()` does **not** append to the WAL —
> `Wal::append` is not wired into the normal publish path. So a bare
> `warn-and-continue` on publish failure would **still drop** the hook event during
> a NATS outage (exactly the 15-min reconnect windows from C). The relay must be
> durable: on publish failure (or unconditionally, append-then-publish), the
> listener appends a WAL-replayable record using the same WAL instance `main.rs`
> already replays at startup, so the event survives the outage. The cleaner option
> is to make `EventPublisher::publish` itself WAL-durable (append → publish →
> mark-sent) so native sidecar events get the same guarantee — pick during impl
> after reading the actual `wal.rs` append/replay record format. Either way, **WAL
> durability is a requirement of this fix, not an assumption.**

**New module:** `rust/bins/sidecar/src/unix_socket_listener.rs`
- `run(socket_path, publisher: Arc<EventPublisher>, shutdown_rx) -> Result<()>`:
  remove stale socket → `UnixListener::bind` → **`chmod 0o600`** on the socket
  (owner-only; hook + sidecar both run as the agent user) → accept loop, one task
  per connection, `tokio::select!` on `shutdown_rx`.
- `handle_connection`: `read_exact(4)` length header → reject if `> MAX_FRAME_SIZE`
  (10 MiB, DoS guard) → `read_exact(len)` → `serde_json::from_slice` → take the
  `type` field as `event_type` (default `"unknown"`) → **durably publish**: append
  a WAL-replayable record, then `publisher.publish(&type, value).await`; on publish
  failure the WAL record remains for replay. The listener needs the WAL handle
  (`Arc`), not just the publisher. A malformed frame is logged and the connection
  closed; the listener survives.

> **Codex correction #2 (reconnect replay):** appending to the WAL is not enough —
> `main.rs` drains `wal.replay()` **once at process startup** and there is no
> reconnect/periodic drain, so a record buffered during the 15-min NATS reconnect
> sits in the WAL until the next *restart*. **Fix B must add a reconnect-triggered
> (or short periodic) WAL drain** so buffered events flush when NATS returns — via
> the NATS reconnect callback/event and/or a `tokio::time::interval` task that
> replays the WAL whenever NATS is connected and the WAL is non-empty. Without it,
> C's "churn made harmless" claim is false and C must re-open. This makes Fix B
> the real durability fix (append-on-failure **plus** drain-on-reconnect).

**`main.rs` wiring:**
1. `mod unix_socket_listener;`
2. **Arc-wrap the publisher** (`let publisher = Arc::new(EventPublisher::new(...))`)
   and update every existing consumer to `.clone()` the `Arc` (heartbeat,
   orchestration subscriber, credentials, WAL replay). A missed site is a compile
   error, so the build is the safety net.
3. Spawn `listener_task` with `publisher.clone()`, the **WAL handle** (`Arc`, the
   same instance `main.rs` replays at startup), and `shutdown_rx.clone()`.
4. Add `listener_task` to the shutdown `tokio::join!`.

**Why a listener and not "rewire the hook to publish to NATS directly":** the hook
is a thin node script with no NATS creds/connection; the socket→sidecar→NATS path
is the existing design and keeps creds server-side. Restoring the missing listener
is the smaller, safer change.

## Fix C — cut NATS churn by raising the JWT TTL, **gated on KICK**

**Investigation:** `DEFAULT_JWT_TTL = Duration::from_secs(15 * 60)`
(`auth_callout/handler.rs:80`). Revocation has two paths
(`auth_callout/worker.rs`): **KICK** via the `$SYS` account (sub-second, used when
`NATS_CALLOUT__SYS_PASSWORD` is configured) and a **TTL fallback** (DB-clear +
natural JWT expiry) when it is not. On this staging deployment
`NATS_CALLOUT__SYS_PASSWORD` **is set** (value not reproduced here), so KICK is the
active revocation mechanism and the 15-min TTL is only a fallback. The short TTL is
what forces the 15-min reconnect churn for long-lived agents.

**Codex correction (do not raise TTL on config-presence):** `NATS_CALLOUT__SYS_PASSWORD`
being *set* does NOT prove KICK works. The worker opens the SYS client **lazily**
inside `revoke`, and revocation also depends on an **in-memory** connection
tracker — so invalid SYS creds, a KICK publish failure, an API restart (tracker
empties), or a connection that was never tracked all silently fall back to natural
JWT expiry. Minting 4 h tokens on the mere presence of the env var would widen the
*real* revocation window to 4 h in exactly those failure modes. The gate
"`sys_password.is_some()`" is therefore **insufficient**.

**Revised decision: keep the 15-min TTL; do NOT raise it now.** Two reasons:
1. Once **Fix B wires WAL durability**, the 15-min reconnect windows stop losing
   events (the relayed + native events buffer through the reconnect and replay). So
   the churn becomes **cosmetic** (reconnect log noise), not data loss — which was
   the only real harm.
2. Raising TTL safely requires *proving* KICK is usable, not assuming it. That is a
   real piece of work (an active startup KICK self-probe against this NATS, re-probed
   periodically, gating the long TTL; fall back to 15 min on any probe failure) — a
   later optimization, not worth the revocation-window risk in this fix.

So **Fix C reduces to: no code change to the TTL.** The 15-min reconnect is
documented as by-design (short TTL = revocation fallback) and rendered harmless by
B. If churn-in-logs is still undesirable later, do it properly behind a KICK probe
(tracked as a follow-up), not behind config-presence. This also means **C ships no
server change** — only A + B (agent image) are needed.

## Testing

- **A:** spawn an agent without `AGENTFORGE_DEVENV_POLICY`; assert the single
  neutral line replaces the 3 warnings; assert docker-in-agent still off
  (behavior unchanged); assert the `if`-branch still works when the policy is set.
- **B (unit, in `unix_socket_listener.rs`):** frame parse happy path; oversize
  length header → error; malformed JSON → logged + connection closed, listener
  survives; mock publisher receives `(event_type, value)`. **(integration, gated):**
  a node one-liner writes a framed event to the socket → assert the mock publisher
  / WAL receives it; concurrent writers; stale-socket cleanup across a SIGKILL
  restart; graceful shutdown.
- **C:** no TTL code change. The value of B is what matters here: an integration
  test that publishes a relay event while NATS is **down** and asserts it lands in
  the WAL and **replays** when NATS returns — proving the 15-min reconnect windows
  no longer lose events. The auth-callout tests are untouched (TTL unchanged).

## Deploy

- A + B ride in the **agent base image**: `make build-agent-base` (sidecar +
  entrypoint), then respawn agents (existing containers keep the old image until
  recreated).
- C ships **no code** (TTL stays 15 min; churn made harmless by B). No server
  rebuild needed. The proper TTL-raise (behind a KICK self-probe) is a tracked
  follow-up, not part of this fix.

So this fix is **one deploy**: the agent base image (A + B), then respawn agents.

## Risks

- **B** is the highest-risk change (new listener + Arc-wrapping the publisher). The
  Arc refactor touches every publisher consumer; rely on the compiler + the gated
  integration test. Socket is `0o600` owner-only inside a single-user container.
- **C** ships nothing now (Codex showed config-presence ≠ working KICK; a long TTL
  could widen the real revocation window on KICK failure). The TTL-raise is deferred
  behind a real KICK self-probe — never behind config-presence, never a flat raise.
- A/B need an agent-image rebuild + respawn to take effect; already-running
  containers keep the old behaviour until recreated.

## Open questions

- B (durability ownership): wire WAL inside the relay listener only, or push it
  into `EventPublisher::publish` so native sidecar events get the same guarantee?
  Decide after reading `wal.rs`'s record format — the publisher-owned option is
  cleaner but touches more call sites.
- B: should the sidecar read `AGENTFORGE_RELAY_SOCKET` (matching the hook's
  override) instead of hardcoding the path? Low cost; do it for symmetry.

## Follow-up (separate, not this fix)

- C proper: an active **KICK self-probe** at sidecar/server startup (and periodic
  re-probe) that proves KICK terminates a connection on this NATS; only then mint
  longer-lived JWTs, falling back to 15 min on any probe failure. This is the only
  safe way to cut the reconnect frequency, per the Codex finding. Tracked separately.
