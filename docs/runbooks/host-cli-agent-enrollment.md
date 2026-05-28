# Host CLI Agent Enrollment

Use this when an operator wants a CLI agent running on their own machine to join
the remote Wisdoverse Forge control plane.

This runbook is written for operators who may not be professional engineers.
Follow the steps in order. Advanced runtime details are intentionally kept after
the basic path.

## Requirements

- The remote API must be reachable by the operator.
- `NATS_AGENT_URL` must point to a NATS address reachable from the operator
  machine. The API strips user-info and injects per-agent credentials.
- `agentforge-sidecar` and the selected CLI (`claude`, `codex`, `gemini`, or
  `opencode`) must be installed on the operator machine.
- The Platform CLI and sidecar should come from a release artifact for the
  operator's operating system whenever possible. See
  [CLI Platform Support](../guides/cli-platform-support.md).

Supported operator platforms are Linux x86_64, Linux ARM64, macOS Apple Silicon,
macOS Intel, and Windows x86_64. Windows ARM64 is a secondary target until the
release pipeline has validated artifacts for it.

## Enroll

Run this from the local directory where the agent should work. Replace
`<project-id>` with the project selected in the web UI.

For macOS or Linux Terminal:

```bash
agentforge agents enroll-local \
  --tool codex \
  --name "Host Codex" \
  --project <project-id> \
  --cwd "$PWD" \
  --shell-format bash
```

For Windows PowerShell:

```powershell
agentforge agents enroll-local `
  --tool codex `
  --name "Host Codex" `
  --project <project-id> `
  --cwd "$($PWD.Path)" `
  --shell-format powershell
```

The command returns the environment variables plus `agentforge-sidecar` in the
selected shell syntax. Run that returned block in the local directory that
should receive tasks. The platform will then see heartbeats, assign tasks
through NATS, and receive signed result evidence from that sidecar.

The `agentforge agents enroll-local` command generates an `Idempotency-Key`
header automatically. If you re-run the same command within 24 hours, the
platform returns the same agent rather than creating a duplicate. To force a
fresh enrollment, pass `--idempotency-key <new-uuid>` or wait for the 24-hour
window to expire.

## Verify

After the sidecar starts:

1. Open the Wisdoverse Forge web UI.
2. Go to Agents.
3. Confirm the new agent appears as `Host CLI`.
4. Confirm the agent status changes after the first heartbeat.
5. Assign a small task and check that task updates and evidence return to the
   platform.
6. (Optional) Confirm the platform recorded the enrollment as Host CLI:
   - Web UI: the agent's detail page shows the "Host CLI" badge.
   - DB: `SELECT runtime_kind FROM agents WHERE id = '<agent-id>';` returns 'cli'.
   - Audit: `SELECT * FROM events WHERE agent_id = '<agent-id>' AND event_type = 'agent.enrolled';`
     should return one row with your user_id and source IP.

If the agent stays offline, check that the local machine can reach the API and
`NATS_AGENT_URL`, then rerun the sidecar command from the enrollment output.

## Network

Host CLI enrollment requires `NATS_AGENT_URL` to use TLS (`tls://`) by default.
If your deployment runs NATS without TLS (lab/sandbox only), an organization
admin must set the policy flag `allow_plaintext_host_nats = true` before
enrollment will succeed. Production deployments should never set this flag.

## Revoke

Delete the managed agent row from the platform, or clear its enrollment by
removing the agent. The per-agent NATS password and HMAC key are stored only on
the agent row and are no longer valid after that row is removed.
