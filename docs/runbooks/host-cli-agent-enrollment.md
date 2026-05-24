# Host CLI Agent Enrollment

Use this when an operator wants a CLI agent running on their own machine to join
the remote Wisdoverse Forge control plane.

## Requirements

- The remote API must be reachable by the operator.
- `NATS_AGENT_URL` must point to a NATS address reachable from the operator
  machine. The API strips user-info and injects per-agent credentials.
- `agentforge-sidecar` and the selected CLI (`claude`, `codex`, `gemini`, or
  `opencode`) must be installed on the operator machine.

## Enroll

```bash
agentforge agents enroll-local \
  --tool codex \
  --name "Host Codex" \
  --project <project-id> \
  --cwd "$PWD"
```

The command returns shell exports plus `agentforge-sidecar`. Run that block in
the local directory that should receive tasks. The platform will then see
heartbeats, assign tasks through NATS, and receive signed result evidence from
that sidecar.

## Revoke

Delete the managed agent row from the platform, or clear its enrollment by
removing the agent. The per-agent NATS password and HMAC key are stored only on
the agent row and are no longer valid after that row is removed.
