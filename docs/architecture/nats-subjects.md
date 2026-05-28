# NATS Subject Namespacing

The platform's per-agent NATS auth-callout binds each connection to subjects scoped by `agent_id`. As of the runtime_kind redesign (#447), `agents.runtime_kind` is a first-class discriminator and we have an opportunity to defense-in-depth subjects by namespacing them by runtime kind.

## Current subjects (today)

```
agent.events.<agent-uuid>           # sidecar → platform: heartbeat, log, tool-use
agent.results.<agent-uuid>          # sidecar → platform: signed result envelope
orchestration.assigned.<agent-uuid> # platform → sidecar: task assignment
```

The auth-callout permits these by templating `<agent-uuid>` into the JWT `subs` / `pubs` claims. The callout does NOT consider runtime_kind today.

## Proposed (future) namespaced subjects

```
agent.<runtime_kind>.events.<agent-uuid>           # e.g., agent.container.events.<uuid>
agent.<runtime_kind>.results.<agent-uuid>          # e.g., agent.cli.results.<uuid>
orchestration.<runtime_kind>.assigned.<agent-uuid> # e.g., orchestration.cli.assigned.<uuid>
```

With `<runtime_kind> ∈ {container, cli, api}`.

## Migration plan (deferred)

1. Sidecar publishes on BOTH subjects for one release (current + namespaced).
2. Platform subscribes to BOTH; emits a metric `nats_legacy_subject_received_total` on legacy hits.
3. After legacy traffic drops to zero in production, sidecar drops the legacy subject; callout policy drops legacy templates.
4. Production deploy verifies metric stays at zero before flag-flip.

## Why we are deferring

Implementing this requires changes to:

- Sidecar publish helpers (Rust)
- Auth-callout subject permission templates (Rust + NATS auth-callout config)
- Orchestrator subscriber templates
- Existing test fixtures that hardcode the subject names

Spec §14 explicitly lists this as deferred. This document locks in the design so the future implementation has a single source of truth.

## See also

- `docs/runbooks/nats-auth.md` for the auth-callout model.
- `docs/superpowers/specs/2026-05-27-host-cli-enrollment-design.md` §16.5 + Platform C7.
