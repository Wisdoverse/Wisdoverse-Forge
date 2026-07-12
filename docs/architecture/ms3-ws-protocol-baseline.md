Confirmed. Key facts locked in:
- `participant_update` has exactly ONE producer (jobs `participant_liveness.rs:928`), no api-side producer — baseline's "likely exist" is false; it's the sole producer.
- `task_update` has TWO producers: api `services/orchestration.rs:1033` (full TaskSummary) + jobs `orchestration_realtime.rs:8` (thin hand-rolled) — internal drift confirmed.
- `lint` runs `metrics:contract` — the drift gate belongs in the same `lint` chain, right after `metrics:contract`.

I have everything needed. Writing the contract document as my return value.

---

# WS Protocol Baseline — MS-3 Refactor Contract of Record

Unify all Rust WebSocket message construction into single serde `ServerMessage` + `ClientMessage` enums, then build a Rust↔TS drift gate. This document is the byte-exact baseline. TS union: `shared/types/protocol.ts:28-160`.

## Ground truth: how frames actually reach the browser

There is exactly one browser WS route: `router.rs:121` `/ws` → `gateway/ws.rs:106` `ws_handler`. It emits frames three ways:

1. **NATS pass-through (verbatim)** — `ws.rs:243-247` does `String::from_utf8_lossy` of the NATS `broadcast.{org_id}` (and scope/admin subject) payload and forwards it byte-for-byte, no re-wrapping. Every `event`, `turn_invalidate`, `orchestration:*`, `cli_image.updated`, `project_clone:status_update` frame is produced upstream (jobs/api services) as serialized bytes and only *relayed* here.
2. **Direct-to-socket** — `terminal_output` / `terminal_error` built by `domain/gateway.rs:114/125` and pushed straight to the socket in `run_terminal_attach`, bypassing NATS entirely.
3. **Handshake warning frames** — `realtime_unavailable_frame` / `realtime_disconnected_frame` (`domain/gateway.rs:72-78`), shape `{"ok":false,"error":string}` — **not** in the TS union at all.

Client→server dispatch is `handle_client_message` (`ws.rs:262-281`): it matches **only** 5 terminal_* tags; every other tag falls through `_ => {}` (`ws.rs:279`) and is silently dropped. Subscription is JWT-scope-driven at connect (`spawn_nats_forwarders` `ws.rs:222-260`, subjects from `subscription_subjects`/`admin_subscription_subjects` `domain/gateway.rs:80-100`), never client-driven.

The `BroadcastEnvelope` enum (`jobs/event_consumer.rs:157-162`, `#[serde(untagged)]`) has only `Event` + `TurnInvalidate` — the untagged wrapper means the frame *is* the inner struct's fields, no envelope key.

## 1. Variant table

Legend — Enumerable: **yes** = a serde enum reproduces the exact bytes today; **needs-care** = producible but has flattening/aliases/dynamic-keys/two-producers to reconcile; **no** = no Rust producer (nothing to enumerate byte-identically).

| type | dir | Rust producer (file:line) | Enumerable | Drift |
|------|-----|---------------------------|------------|-------|
| `event` | S | `jobs/event_consumer.rs:130-141` (struct `BroadcastMessage`), built `:433`, published `:382`/`:710` → relayed `ws.rs:243` | needs-care | **SEVERE**: flat, no `payload` |
| `turn_invalidate` | S | `jobs/event_consumer.rs:143-155` (`TurnInvalidateMessage`), built `:440`, published `:388`/`:710` | **yes** | none (structural); TS has no consumer |
| `orchestration:task_update` | S | **TWO**: (A) `api/domain/orchestration.rs:512` full `TaskSummary` (`:296-344`), called `services/orchestration.rs:1033`, `domain/orchestration.rs:1394`; (B) `jobs/orchestration_realtime.rs:8`+`:43` thin hand-rolled | needs-care | **MODERATE internal** (A vs B) |
| `orchestration:participant_update` | S | **ONE**: `jobs/participant_liveness.rs:928` hand-rolled | needs-care | subset vs `ParticipantSummary` |
| `cli_image.updated` | S | `jobs/cli_image_updater.rs:1060` `build_cli_image_frame`; const `core/broadcast_protocol.rs:25` | **yes** | none |
| `project_clone:status_update` | S | `api/domain/project_clone.rs:1011` `CloneEvent::ws_frame`; const `:949` | **yes** | none (envelope); `details` snake_case passthrough by design |
| `terminal_output` | S | `api/domain/gateway.rs:114` direct-to-socket | **yes** | semantic: `data` is base64, untyped in TS |
| `terminal_error` | S | `api/domain/gateway.rs:125` direct-to-socket | **yes (add to TS)** | **MISSING from TS union** |
| `{"ok":false,"error":..}` realtime | S | `api/domain/gateway.rs:72`/`76` | n/a (not in union) | **MISSING from TS union** |
| `terminal_attach` | C | parsed `ws.rs:274` → `domain/gateway.rs:9` `GatewayClientMessage`+dynamic getters `:106-112` | needs-care | none (lenient parse) |
| `terminal_data` | C | parsed `ws.rs:275` | needs-care | none |
| `terminal_input` | C | parsed `ws.rs:276` | needs-care | none |
| `terminal_resize` | C | parsed `ws.rs:277` | needs-care | none |
| `terminal_detach` | C | parsed `ws.rs:278` | needs-care | none |
| `connected` | S | **none** | no | dead / client-side-only (`WebSocketProvider.tsx` sets from `ws.onopen`) |
| `history` | S | **none** (REST only, `orchestrator/workflow/handler.rs:313` is an HTTP body) | no | dead |
| `error` | S | **none** in `{type,payload}` shape | no | dead / real error frames differ |
| `agents` | S | **none** (`api/domain/agent.rs:16` is a REST body) | no | dead; TS still consumes `useWsDispatch.ts:73` |
| `agent_update` | S | **none** | no | dead; TS still consumes `useWsDispatch.ts:92` |
| `permission_prompt` | S | **none** (inner hook event-type only) | no | dead top-level type |
| `permission_resolved` | S | **none** | no | dead |
| `text_tiles` | S | **none** (entity `TextTile` also dead) | no | dead |
| `groups` | S | **none** (`api/domain/resource.rs:48` REST body, thinner shape) | no | dead + shape mismatch |
| `group_update` | S | **none** (1 grep hit total) | no | dead |
| `worker_report` | S | **none** | no | dead |
| `auth_success` | S | **none** as WS (`routes/auth.rs:207` is REST) | no | dead |
| `auth_required` | S | **none** (0 hits) | no | dead |
| `auth_failed` | S | **none** (0 hits) | no | dead |
| `collaborator_added` | S | **none** (DB `AgentCollaborator` `entities.rs:554` never broadcast) | no | dead + shape mismatch |
| `collaborator_removed` | S | **none** | no | dead |
| `collaborator_updated` | S | **none** | no | dead |
| `ownership_transferred` | S | **none** | no | dead |
| `output` | S | **none** | no | dead |
| `channel_agents` | S | **none** | no | dead (paired w/ `subscribe_channel`) |
| `agent_health_changed` | S | **none** (0 hits outside protocol.ts) | no | dead — no producer AND no consumer |
| `pong` | S | **none** (WS control-frame Pong `ws.rs:192`) | no | dead at JSON layer |
| `server_draining` | S | **none** (0 hits) | no | dead |
| `voice_ready` | S | **none** (voice is REST-only) | no | dead |
| `voice_transcript` | S | **none** | no | dead |
| `voice_utterance_end` | S | **none** | no | dead |
| `voice_error` | S | **none** | no | dead |
| `subscribe` | C | parsed but `_ => {}` no-op `ws.rs:279` | no | no-op by intent (JWT-scope-driven) |
| `get_history` | C | no-op `ws.rs:279` | no | dead (pairs w/ `history`) |
| `ping` | C | no-op `ws.rs:279` | no | dead (WS control ping only) |
| `voice_start` | C | no-op `ws.rs:279` | no | dead |
| `voice_stop` | C | no-op `ws.rs:279` | no | dead |
| `permission_response` | C | no-op `ws.rs:279` | no | dead |
| `subscribe_channel` | C | no-op `ws.rs:279` | no | dead (pairs w/ `channel_agents`) |

**Live server producers: 8** (`event`, `turn_invalidate`, `orchestration:task_update`, `orchestration:participant_update`, `cli_image.updated`, `project_clone:status_update`, `terminal_output`, `terminal_error`). **Live client handlers: 5** (`terminal_*`). Everything else is dead weight.

## 2. Drift list (must resolve or deliberately preserve)

**D1 — `event` shape is a fiction (SEVERE).** Wire is FLAT: `{"type":"event","eventType":<hook type>,"eventData":{…},"agentId":<sessionId-or-uuid>,"orgId":<uuid>}` — 5 top-level camelCase keys via `#[serde(rename)]` (`event_consumer.rs:130-141`). TS declares `{type:'event'; payload: ClaudeEvent}`. There is **no `payload` key**. `eventData` is the normalized object from `normalize_event_data` (`event_consumer.rs:460-486`): always injects `type`,`orgId`,`sessionId`,`timestamp`(ms i64),`id`(uuidv7) plus the sidecar's original keys, via `.entry().or_insert` (caller values win). `agentId` is a **sibling**, not inside `eventData`. *Resolve*: rewrite the TS `event` variant to the flat shape before/with the enum move.

**D2 — `terminal_error` produced by Rust, missing from TS union.** `domain/gateway.rs:125`, `{type:'terminal_error',payload:{agentId,message}}`, consumed by `AgentTerminalTab.tsx`. *Resolve*: add to `ServerMessage`.

**D3 — realtime warning frames missing from TS union.** `{"ok":false,"error":"real-time updates unavailable"|"…disconnected"}` (`domain/gateway.rs:72/76`). Not `{type,payload}` at all. *Preserve as an out-of-band control shape OR add a typed variant* — pinned by `gateway.rs:196-199`.

**D4 — `terminal_output.data` is base64, untyped in TS.** `BASE64.encode` at `gateway.rs:119`; `AgentTerminalTab.tsx:164` decodes. Structurally exact; only the encoding contract is undocumented. Asymmetry: inbound `terminal_data.data` is a **plain UTF-8 string** (`ws.rs:341`), outbound is base64. *Preserve*, document.

**D5 — `orchestration:task_update` internal drift between two producers (MODERATE).** Producer A (`api/domain/orchestration.rs:512`) serializes the full `TaskSummary`: includes `selfFix`, `prNumber`, `prUrl`, `prHeadSha`, `reviewStatus`, `contextCounts`, `attempt`, `leaseExpiresAt`, and **omits** `None` optionals (`skip_serializing_if`). Producer B (`jobs/orchestration_realtime.rs:43-73`) is a hand-rolled `json!` that **omits all of** selfFix/pr*/reviewStatus/contextCounts/attempt/leaseExpiresAt, hardcodes `method:"tasks/send"`, emits `groupId:""` (not null) and `assignedTo:null` (not omitted). Two non-byte-identical wire shapes for one type. *Resolve*: switch producer B to serialize the same `TaskSummary` struct.

**D6 — `orchestration:participant_update` is a subset of `ParticipantSummary`.** The sole producer (`participant_liveness.rs:942`) emits only `{id, agentId, name, status}`. The `ParticipantSummary` struct (`domain/orchestration.rs:397-411`) also has `capabilities: Vec<String>` and optional `runtimeKind`/`lastHeartbeatAt`. TS types `payload.participant` as `ParticipantSummary`. So the WS frame is a strict subset of the REST type. *Resolve*: define a dedicated participant projection matching the 4-field wire, or extend the producer to emit the full struct.

**D7 — client `subscribe`/`ping`/`get_history` are accepted-but-inert.** Valid JSON, zero effect. Not a serialization drift; a semantics drift. *Preserve or delete* — deletion is cleaner (see §4 PR-C).

**D8 — `cloneStatus` vocab uses `queued` not `pending`** (`project_clone.rs`, per memory). TS matches. No action; a guard test should pin it.

## 3. Enumerability verdict

**Move byte-identically into a single serde enum NOW (already `type`+`payload` or already renamed):**
- `turn_invalidate` — `TurnInvalidateMessage` is already `{type, payload:{agentId,timestamp}}` with renames; the one clean round-trip.
- `cli_image.updated` — already a struct-backed frame, camelCase, null-present (not omitted), string `eventId`, `unix` number.
- `project_clone:status_update` — `CloneEvent::ws_frame` already builds `{type, payload:{action,eventId,projectId,cloneStatus,details}}`; model `details` as `serde_json::Value`.
- `terminal_output` + `terminal_error` — struct variants `{type, payload:{agentId, data|message}}`; keep the base64 encode in the serializer path.

**Need care (producible, with a named reconciliation):**
- `event` — enum can match ONLY if modeled as the flat internally-tagged shape (`#[serde(tag="type")]` + flattened `eventType`/`eventData`/`agentId`/`orgId`), **not** `payload:ClaudeEvent`. Requires D1 first. `eventData` stays `serde_json::Value` (nested passthrough — it is a superset of `ClaudeEvent` with injected keys).
- `orchestration:task_update` — enum must serialize the full `TaskSummary`; requires collapsing producer B onto it (D5).
- `orchestration:participant_update` — needs a fixed participant projection struct (4 fields) or D6 resolution; hand-rolled `json!` today.
- All 5 client `terminal_*` — parseable as a tagged `ClientMessage` enum, but the enum **must preserve lenient semantics**: `agentId` as *string* uuid, `cols`/`rows` optional-with-default 80/24 min-1, non-string `keys[]` dropped, and **`None`-on-parse-failure → silent no-op** (`parse_gateway_client_message` returns `Option`, `ws.rs:269`). A strict `#[derive(Deserialize)]` that errors on a missing field would change behavior — use `#[serde(default)]` + tolerant field types.

**Cannot enumerate (no producer — nothing to keep byte-identical):** all 33 dead types in the table. A serde `ServerMessage`/`ClientMessage` enum must **not** include them unless a producer is being built in the same PR.

## 4. Recommended phasing (each PR byte-identical-verified)

The precedent for the gate is `scripts/check-metrics-contract.mjs`, wired into `npm run lint` at `package.json:65` (`fsd:check && beginner:ux:copy && metrics:contract && eslint`). Follow it exactly.

**PR-0 — Golden fixture + drift gate (no behavior change).** Land `tests/fixtures/ws-protocol/*.json` (one file per **live** variant = the 8 producers, plus the 2 out-of-band control shapes) and `scripts/check-protocol-contract.mjs`. The gate: (a) parse the live producer sites and assert every emitted `"type":"…"` string literal has a matching TS `ServerMessage` variant and a golden fixture; (b) round-trip each fixture through the (future) Rust enum via a `#[test]` and assert `serde_json::to_value == fixture`. Wire into `lint`: `… && metrics:contract && protocol:contract && eslint`. Add `"protocol:contract": "node scripts/check-protocol-contract.mjs"` to `package.json` scripts. This PR ships the gate GREEN against today's shapes (fixtures are the current wire), so it locks the baseline before any refactor.

**PR-A — TS truth-up (docs/type-only, no Rust).** Fix D1 (`event` → flat 5-key shape), D2 (add `terminal_error`), D3 (add/annotate realtime warning shape), D4 (comment `data` base64). Delete the 33 dead `ServerMessage`/`ClientMessage` variants + dead entities (`TextTile`/`CreateTextTileRequest`/`UpdateTextTileRequest` `agent.ts:226-260`, `WorkerReportPayload`). Update `useWsDispatch.ts` to drop dead `case 'agents'/'agent_update'/'event'.payload` handlers. Fixtures from PR-0 now assert the corrected TS shapes.

**PR-B — Introduce `ServerMessage` enum for the 4 clean + 2 terminal variants.** `turn_invalidate`, `cli_image.updated`, `project_clone:status_update`, `terminal_output`, `terminal_error`. Replace the individual `json!`/struct sites with enum variants; the fixture round-trip test proves byte-identity.

**PR-C — `ClientMessage` enum for the 5 `terminal_*`.** Replace `GatewayClientMessage` + dynamic getters with a tagged enum carrying tolerant defaults (preserve D7 no-op-on-unknown by keeping the `_`/`Unknown` arm). Delete the dead client tags.

**PR-D — Fold `event` into the enum.** Flat internally-tagged variant with `eventData: Value`. Depends on PR-A's TS fix. Highest-risk (most-trafficked frame) — ship last of the server side.

**PR-E — Reconcile the two orchestration producers.** D5: switch `jobs/orchestration_realtime.rs::summarize_task_for_ws` to build a real `TaskSummary` (share the `task_summary` constructor at `domain/orchestration.rs:527`). D6: give participant_update a fixed projection struct and move it into the enum. This is the only PR that changes wire bytes (thin→full task shape) — call it out; the frontend already types `TaskSummary` loosely so it tolerates the added fields.

**Gate + fixture location:** `scripts/check-protocol-contract.mjs` (next to `check-metrics-contract.mjs`), fixtures under `tests/fixtures/ws-protocol/`, Rust round-trip `#[test]` co-located with the enum (`rust/crates/jobs/src/event_consumer.rs` tests + wherever the unified `ServerMessage` lands — likely a new `rust/crates/core/src/ws_protocol.rs` so both `api` and `jobs` crates share it, mirroring how `broadcast_protocol.rs` already holds the shared consts).

## 5. Variants with NO Rust producer (candidate dead TS types)

**Server (33):** `connected`, `history`, `error`, `agents`, `agent_update`, `permission_prompt`, `permission_resolved`, `text_tiles`, `groups`, `group_update`, `worker_report`, `auth_success`, `auth_required`, `auth_failed`, `collaborator_added`, `collaborator_removed`, `collaborator_updated`, `ownership_transferred`, `output`, `channel_agents`, `agent_health_changed`, `pong`, `server_draining`, `voice_ready`, `voice_transcript`, `voice_utterance_end`, `voice_error`.

**Client (7):** `subscribe`, `get_history`, `ping`, `voice_start`, `voice_stop`, `permission_response`, `subscribe_channel`.

Strongest-dead (zero producer AND zero consumer, safe to delete outright): `agent_health_changed`, `server_draining`, `auth_required`, `auth_failed`, `permission_resolved`, `group_update`, `collaborator_*`, `ownership_transferred`, `text_tiles` (+ its entity), all `voice_*`. The `channel_agents`↔`subscribe_channel` and `history`↔`get_history` pairs should be deleted together.

**Not-dead-but-not-in-TS (add, don't delete):** `terminal_error` (`gateway.rs:125`) and the `{ok:false,error}` realtime warning frames (`gateway.rs:72/76`) — real Rust producers absent from the TS union.

**Files that are the contract of record:** producers `rust/crates/jobs/src/event_consumer.rs:130-162`, `rust/crates/jobs/src/orchestration_realtime.rs:8-74`, `rust/crates/jobs/src/participant_liveness.rs:928-960`, `rust/crates/jobs/src/cli_image_updater.rs:1060`, `rust/crates/api/src/domain/orchestration.rs:296-521`, `rust/crates/api/src/domain/project_clone.rs:1011`, `rust/crates/api/src/domain/gateway.rs:72-134`; relay `rust/crates/api/src/gateway/ws.rs:243-281`; TS union `shared/types/protocol.ts:28-160`; gate precedent `scripts/check-metrics-contract.mjs` + `package.json:64-65`.