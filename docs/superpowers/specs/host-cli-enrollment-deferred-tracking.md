# Host CLI Enrollment — Deferred Item Tracking

The Host CLI Enrollment redesign
([2026-05-27-host-cli-enrollment-design.md](2026-05-27-host-cli-enrollment-design.md))
deferred eight items in its §14 "Out of Scope" section. Each now has a tracking
issue so nothing is lost to the spec archive. This table is the index.

| Spec ref     | Item                                                              | Issue                                                             | Status                     |
| ------------ | ----------------------------------------------------------------- | ----------------------------------------------------------------- | -------------------------- |
| DDD C1       | Model `AgentRuntime` as a sum type with per-variant value objects | [#455](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/455) | open                       |
| DDD C6       | `EnrolledHostCli` typestate for NATS-bound operations             | [#456](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/456) | open                       |
| Platform C7  | Namespace NATS subjects by runtime kind                           | [#457](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/457) | open (design shipped #450) |
| Platform C4  | HMAC envelope replay protection (nonce + ts window)               | [#458](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/458) | open (design shipped #450) |
| Platform C2  | Sidecar binary supply chain — complete operator-verify loop       | [#459](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/459) | open (partial #452)        |
| Architect C7 | Benchmark `RuntimeKind` serde on the agent-list hot path          | [#460](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/460) | open                       |
| PM C4        | Admin UI filter + projection field for `runtime_kind`             | [#461](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/461) | open                       |
| Ops          | Staging dry-run of migration 062-065 sequence                     | [#462](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/462) | open                       |

## Shipped follow-ups (no longer deferred)

These were on the original deferred list and have since landed:

| Item                                                          | PR                                                              |
| ------------------------------------------------------------- | --------------------------------------------------------------- |
| Grafana dashboard + Prometheus alerts for agents runtime SLOs | [#451](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/451) |
| cosign keyless signing + SBOM + `agentforge verify` CLI       | [#452](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/452) |
| FormEvent deprecation cleanup + codex mass-rebase postmortem  | [#453](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/453) |
| SHA-256 manifest for migrations                               | [#454](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/454) |
| HMAC envelope + NATS subject namespacing **design specs**     | [#450](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/450) |

## Why this file exists

A shipped spec's "Out of Scope" section is where good intentions go to die. The
FAANG-standard practice is: every deferred item gets a tracked issue at merge
time, indexed from the spec, so the backlog is real and auditable rather than
folklore. When an issue here closes, move its row to the "Shipped follow-ups"
table.
