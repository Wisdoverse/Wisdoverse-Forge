# Credential Sync Runbook

## What this is

When a user runs `claude /login` (or `gemini` / `opencode` / `codex`
equivalents) inside an agent container, the sidecar publishes the resulting
`~/.claude/*.json` file map to NATS subject `creds.<agent_id>`. The backend
consumer verifies the HMAC envelope, encrypts the blob, and writes it to
`user_cli_credentials`. Future container spawns bind-mount the credentials
back via `/run/secrets/oauth-credentials/`.

Implementation: issue #41, restoring the Go/TS flow dropped during the
Rust migration.

## Rollout flag

`CREDENTIAL_SYNC_ENABLED` (backend + agent container env). Default `false`.

- `true`: backend spawns `CredentialStreamWorker`; sidecar spawns
  `credential watcher`.
- `false`: neither is spawned. Users fall back to the manual
  `PUT /api/v1/cli-credentials/<cli>` upload endpoint.

## Key metrics

- `credential_sync_published_total{cli_tool}` — watcher publishes.
- `credential_sync_received_total{cli_tool}` — backend persists.
- `credential_sync_publish_errors_total{reason}` — producer-side failures
  (`build_failed`, `serialize_failed`, `sign_failed`, `envelope_encode_failed`,
  `publish_failed`, `ack_failed`). Sidecar-owned.
- `credential_sync_unauthorized_total{reason}` — backend rejections
  (`bad_subject`, `envelope_decode_failed`, `envelope_agent_mismatch`,
  `payload_agent_mismatch`, `payload_org_mismatch`, `agent_row_missing`,
  `timestamp_outside_window`, `agent_unknown`, `hmac_lookup_failed`,
  `signature_mismatch`, `file_map_invalid`, `payload_oversized`,
  `cli_tool_unknown`). Security-relevant drops.
- `credential_sync_persist_duration_seconds{cli_tool}` — encrypt + UPSERT.

Expected shape: `received_total` ≤ `published_total`. A permanent gap
(> 1% over 24 h) means the consumer is falling behind or rejecting.

## Common alerts

**`credential_sync_unauthorized_total{reason="signature_mismatch"}` > 0**

A sidecar published a bad signature OR an attacker is probing. Check:

1. Did an operator just rotate `agents.hmac_secret` without restarting the
   agent? (Agents re-read `HMAC_SECRET` only at container start — restart
   them.)
2. Is the counter growing from a single agent? Check
   `SELECT hmac_secret FROM agents WHERE id = '<agent>'` vs what the
   sidecar environment shows via `docker exec <container> env | grep HMAC`.

**`reason="timestamp_outside_window"` > 0**

Clock drift on the host running the sidecar. Check NTP. Accept a small
baseline (< 10/hour) as routine; action on sustained spikes.

**`reason="agent_row_missing"` > 0**

A signed, well-formed envelope arrived for an `agent_id` that has no row
in the `agents` table. Most likely cause: agent was deleted between the
publish and the consume. If rate > 0 sustained, investigate for forgery —
the HMAC lookup and this DB lookup should agree.

**`reason="payload_org_mismatch"` > 0**

Envelope signed correctly by agent A, but the payload's `organization_id`
differs from agent A's DB row. This is active tampering — cut a security
incident ticket.

**Stream pending growing**

```bash
nats stream info CREDENTIALS
```

Check `messages` + `num_pending`. If pending climbs, the worker is slow
or dead. Inspect backend logs for `credential sync dropped` lines.

## CLI Auth Proxy Invalid Client

Alert: `CliAuthProxyInvalidClientFailures`

Metric:

```promql
increase(credential_refresh_errors_total{reason="invalid_client"}[10m]) > 0
```

This is an operator-owned OAuth app failure. The IdP rejected the configured
`client_id` / `client_secret` / app registration, so asking users to reconnect
will not fix it. Do not revoke user rows for this failure class.

Immediate checks:

1. Confirm which CLI provider is affected from the alert's `cli_tool` label.
2. Inspect backend logs for `OAuth app rejected by IdP`.
3. Verify `CLI_AUTH_PROXY_OPENAI_CLIENT_ID`,
   `CLI_AUTH_PROXY_OPENAI_CLIENT_SECRET`,
   `CLI_AUTH_PROXY_OPENAI_AUTH_ENDPOINT`, and
   `CLI_AUTH_PROXY_OPENAI_TOKEN_ENDPOINT` against the IdP registration.
4. If the OAuth app secret was rotated, update the secret manager or deploy env
   and restart the backend.
5. After restart, check `/metrics` and confirm the counter stops increasing.

## Manual force-resync

If a user reports their CLI credentials didn't persist:

1. `docker exec <container> ls -la <CREDS_DIR>` — confirm files exist.
   (`CREDS_DIR` = `/home/agent/.claude` for claude, `/home/agent/.gemini`
   for gemini, `/home/agent/.local/share/opencode` for opencode,
   `/home/agent/.codex` for codex.)
2. `docker logs <container> | grep 'credential watcher'` — confirm
   watcher started and published.
3. Backend logs: `grep 'credential sync' server.log` — look for
   `received` line with matching `agent_id`.
4. If sync never fired, ask user to touch a file:
   `docker exec <container> sh -c "touch /home/agent/.claude/auth.json"`
   — this triggers the fsnotify Modify event.

## Disabling without code change

```bash
# Backend
docker compose --env-file docker/.env -f docker/compose.yml --profile external up -d --force-recreate agentforge-server
# Set CREDENTIAL_SYNC_ENABLED=false in the backend environment before restart.
# Agent containers started after this inherit the flag from API env.
```

Existing running containers keep publishing (harmless — backend drops).

## Rollback

None required. Flip the flag; manual upload path still works; stored
ciphertexts remain valid because the encryption key did not change.

## Reference

- Sidecar watcher: `rust/bins/sidecar/src/credentials.rs`
- Backend consumer: `rust/crates/jobs/src/credential_consumer.rs`
- Shared types: `rust/crates/core/src/credential_protocol.rs`
- Stream bootstrap: `rust/bins/server/src/streams.rs`
- NATS perms: `rust/crates/api/src/services/auth_callout/perms.rs`
- Legacy origin: commit `f0feb468` (2026-02-17, Go + TS era)
