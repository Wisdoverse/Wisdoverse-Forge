# Host CLI Enrollment — Deferred Item Tracking

The Host CLI Enrollment redesign
([2026-05-27-host-cli-enrollment-design.md](2026-05-27-host-cli-enrollment-design.md))
deferred eight items in its §14 "Out of Scope" section. Each now has a tracking
issue so nothing is lost to the spec archive. This table is the index.

| Spec ref     | Item                                                              | Issue                                                             | Status                                   |
| ------------ | ----------------------------------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------- |
| DDD C1       | Model `AgentRuntime` as a sum type with per-variant value objects | [#455](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/455) | open                                     |
| DDD C6       | `EnrolledHostCli` typestate for NATS-bound operations             | [#456](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/456) | open                                     |
| Platform C7  | Namespace NATS subjects by runtime kind                           | [#457](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/457) | open (design shipped #450)               |
| Platform C4  | HMAC envelope replay protection (nonce + ts window)               | [#458](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/458) | open (design shipped #450)               |
| Platform C2  | Sidecar binary supply chain — complete operator-verify loop       | [#459](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/459) | open (partial #452)                      |
| Architect C7 | Benchmark `RuntimeKind` serde on the agent-list hot path          | [#460](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/460) | closed — measured, no action (see below) |
| PM C4        | Admin UI filter + projection field for `runtime_kind`             | [#461](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/461) | open                                     |
| Ops          | Staging dry-run of migration 062-065 sequence                     | [#462](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/462) | open                                     |

## Shipped follow-ups (no longer deferred)

These were on the original deferred list and have since landed:

| Item                                                          | PR                                                                                                  |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Grafana dashboard + Prometheus alerts for agents runtime SLOs | [#451](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/451)                                     |
| cosign keyless signing + SBOM + `agentforge verify` CLI       | [#452](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/452)                                     |
| FormEvent deprecation cleanup + codex mass-rebase postmortem  | [#453](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/453)                                     |
| SHA-256 manifest for migrations                               | [#454](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/454)                                     |
| HMAC envelope + NATS subject namespacing **design specs**     | [#450](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/450)                                     |
| Benchmark `RuntimeKind` serde on agent-list hot path          | [#460](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/460) — see PR chore/runtime-kind-bench |

## #460 — RuntimeKind decode benchmark result

Criterion benchmark committed at `rust/crates/core/benches/runtime_kind_decode.rs`.
Runner: Linux x86-64 (CI-class runner, unoptimised background load).
All numbers from `cargo bench` optimised (`--release`) profile.

| Benchmark                                                       | Median time                |
| --------------------------------------------------------------- | -------------------------- |
| `RuntimeKind::parse_legacy` — canonical hit (container/cli/api) | ~36–38 ns/call             |
| `RuntimeKind::parse_legacy` — miss path                         | ~60 ns/call                |
| `CliToolKind::parse_legacy` — canonical hit                     | ~37–40 ns/call             |
| 1 000-row agent-list decode loop (RuntimeKind)                  | ~33 µs total (~33 ns/row)  |
| 10 000-row agent-list decode loop (RuntimeKind)                 | ~325 µs total (~33 ns/row) |
| `as_str` (encode path)                                          | ~1.3 ns                    |
| Baseline: integer compare                                       | ~0.7 ns                    |

**Decision: no optimization warranted.**

`parse_legacy` adds ~36 ns per row. The entire decode of a 1 000-row agent list
totals ~33 µs. A typical PostgreSQL round-trip is 0.5–10 ms, so the decode cost
is < 0.1 % of the observable request latency. The `to_ascii_lowercase()` call
does allocate a `String` per row, but at ~20–30 bytes per allocation on an arena
that is freed at row drop, the allocator overhead is absorbed by CPU-cache-hot
reuse and is undetectable against DB latency. No zero-alloc optimisation applied.

## Why this file exists

A shipped spec's "Out of Scope" section is where good intentions go to die. The
FAANG-standard practice is: every deferred item gets a tracked issue at merge
time, indexed from the spec, so the backlog is real and auditable rather than
folklore. When an issue here closes, move its row to the "Shipped follow-ups"
table.
