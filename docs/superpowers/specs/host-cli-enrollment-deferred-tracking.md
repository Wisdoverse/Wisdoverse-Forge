# Host CLI Enrollment — Deferred Item Tracking

The Host CLI Enrollment redesign
([2026-05-27-host-cli-enrollment-design.md](2026-05-27-host-cli-enrollment-design.md))
deferred eight items in its §14 "Out of Scope" section. Each now has a tracking
issue so nothing is lost to the spec archive. This table is the index.

| Spec ref     | Item                                                              | Issue                                                             | Status                                                                                                        |
| ------------ | ----------------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| DDD C1       | Model `AgentRuntime` as a sum type with per-variant value objects | [#455](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/455) | ✅ closed — PR #470                                                                                           |
| DDD C6       | `EnrolledHostCli` typestate for NATS-bound operations             | [#456](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/456) | ✅ closed — PR #471                                                                                           |
| Platform C7  | Namespace NATS subjects by runtime kind                           | [#457](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/457) | 🟢 all three channels shipped additive/zero-outage: phase 1 (`events.ingest`), 1b (`orchestration.result`, WorkQueue stream/durable widen), 1c (`orchestration.assigned`, in-place single-filter swap + dual-publish + kind-scoped single-filter CREATE grant). Only per-channel **legacy-drop** deploys remain (gated on each drain signal). |
| Platform C4  | HMAC envelope replay protection                                   | [#458](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/458) | ✅ closed — PR #472 (reconcile found+fixed a real event-ingest verify gap)                                    |
| Platform C2  | Sidecar binary supply chain — complete operator-verify loop       | [#459](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/459) | ✅ closed — PR #469 (container-image cosign + verify-image)                                                   |
| Architect C7 | Benchmark `RuntimeKind` serde on the agent-list hot path          | [#460](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/460) | ✅ closed — measured, no action (see below)                                                                   |
| PM C4        | Admin UI filter + projection field for `runtime_kind`             | [#461](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/461) | ✅ closed — PR #468                                                                                           |
| Ops          | Staging dry-run of migration 062-065 sequence                     | [#462](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/462) | ✅ closed — PR #474 (dry-run against live prod-ext; found+fixed 062/063 pre-flight + rollback)                |

**7 of 8 closed; #457 fully namespaced.** Remaining work: only the per-channel
**legacy-drop deploys** for `events.ingest` + `orchestration.result` (gated on each
`agentforge_nats_legacy_subject_received_total{subject=…}` series holding at
present-AND-zero across a container turnover) and for `orchestration.assigned`
(gated on jsz showing all `orch-assignment-*` durables on namespaced filters).
These are post-observation **deploys, not code**. #457 shipped all three channels
additively with zero outage — phase 1 (`events.ingest`) and phase 1b
additionally widened the `ORCHESTRATION_RESULTS` WorkQueue stream + its shared
durable from `.*`→`.>` (the durable via an empirically-required delete+recreate,
since `get_or_create_consumer` does not update an existing filter). See
`docs/architecture/nats-subjects.md`.

## Shipped follow-ups (no longer deferred)

These were on the original deferred list and have since landed:

| Item                                                           | PR                                                                            |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Grafana dashboard + Prometheus alerts for agents runtime SLOs  | [#451](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/451)               |
| cosign keyless signing + SBOM + `agentforge verify` CLI        | [#452](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/452)               |
| FormEvent deprecation cleanup + codex mass-rebase postmortem   | [#453](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/453)               |
| SHA-256 manifest for migrations                                | [#454](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/454)               |
| HMAC envelope + NATS subject namespacing **design specs**      | [#450](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/450)               |
| Benchmark `RuntimeKind` serde on agent-list hot path           | [#467](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/467) (closes #460) |
| Admin `runtime_kind` projection field + agents filter          | [#468](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/468) (closes #461) |
| Sign + attest container images; `agentforge verify-image`      | [#469](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/469) (closes #459) |
| `AgentRuntime` sum type — domain branches on the typed view    | [#470](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/470) (closes #455) |
| `EnrolledHostCli` typestate gates host-CLI credential issuance | [#471](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/471) (closes #456) |
| HMAC replay reconcile — fixed unverified event-ingest frames   | [#472](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/472) (closes #458) |

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
