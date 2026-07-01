#!/usr/bin/env node
// WebSocket protocol contract gate (MS-3).
//
// The browser talks to Rust over one WS route (`/ws`). The wire shapes are
// declared for the frontend in `shared/types/protocol.ts` (ServerMessage /
// ClientMessage unions), but Rust produces them via SCATTERED paths (a serde
// enum, hand-rolled `json!` frames, direct-to-socket terminal frames). Nothing
// keeps the two in sync today, so the union drifted badly: most declared TS
// variants have NO Rust producer (legacy dead types), and several live Rust
// frames are absent from the TS union.
//
// This gate locks the byte-exact wire baseline as golden fixtures under
// `tests/fixtures/ws-protocol/` and reports the full drift set so the MS-3
// refactor (see docs/architecture/ms3-ws-protocol-baseline.md) can retire it
// PR by PR. It is wired into `npm run lint` next to `metrics:contract`.
//
// PR-0 (this) fails ONLY on a fixture problem (missing/extra/invalid fixture) —
// it ships GREEN against today's shapes. The TS<->Rust drift is REPORTED, not
// enforced, until PR-A truths-up the TS union and later PRs unify the Rust
// serializers behind a single serde enum + a Rust round-trip test.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const fixtureDir = path.join(repoRoot, 'tests/fixtures/ws-protocol')
const protocolTs = path.join(repoRoot, 'shared/types/protocol.ts')

// The authoritative set of LIVE messages — every one has a real Rust producer
// today (see the baseline doc for file:line). `fixture` is the golden file that
// pins its current wire shape (`null` = deliberately deferred: task_update has
// two divergent producers, reconciled in PR-E before it is pinned). Whether
// protocol.ts already declares each type is DERIVED from the parsed TS union
// below (never hard-coded), so the drift snapshot cannot silently go stale.
const LIVE_SERVER = [
  { type: 'event', fixture: 'event.json' },
  { type: 'turn_invalidate', fixture: 'turn_invalidate.json' },
  { type: 'terminal_output', fixture: 'terminal_output.json' },
  { type: 'terminal_error', fixture: 'terminal_error.json' },
  { type: 'cli_image.updated', fixture: 'cli_image.updated.json' },
  { type: 'project_clone:status_update', fixture: 'project_clone_status_update.json' },
  { type: 'orchestration:participant_update', fixture: 'orchestration_participant_update.json' },
  { type: 'orchestration:task_update', fixture: null }, // deferred to PR-E
]

// Out-of-band control frames the Rust WS handler emits that are NOT part of the
// ServerMessage union at all (shape `{ok:false,error}`). Pinned so a change is
// caught, tracked separately because they have no `type` field.
const CONTROL_FRAMES = [
  { name: 'realtime_unavailable', fixture: 'realtime_unavailable.json' },
  { name: 'realtime_disconnected', fixture: 'realtime_disconnected.json' },
]

const errors = []
const notes = []

// --- 1. Every fixture is valid JSON, and matches a known live/control entry ---
const expectedFixtures = new Set(
  [...LIVE_SERVER.map((m) => m.fixture), ...CONTROL_FRAMES.map((c) => c.fixture)].filter(Boolean)
)
const presentFixtures = fs.existsSync(fixtureDir)
  ? fs.readdirSync(fixtureDir).filter((f) => f.endsWith('.json'))
  : []

for (const file of presentFixtures) {
  try {
    JSON.parse(fs.readFileSync(path.join(fixtureDir, file), 'utf8'))
  } catch (err) {
    errors.push(`fixture ${file} is not valid JSON: ${err.message}`)
    continue
  }
  if (!expectedFixtures.has(file)) {
    errors.push(
      `orphan fixture ${file}: not referenced by any LIVE_SERVER/CONTROL_FRAMES entry (add it or delete the file)`
    )
  }
}

// --- 2. Every non-deferred live/control entry has its fixture on disk ---
for (const entry of LIVE_SERVER) {
  if (entry.fixture && !presentFixtures.includes(entry.fixture)) {
    errors.push(
      `live server message '${entry.type}' is missing its golden fixture ${entry.fixture}`
    )
  }
}
for (const control of CONTROL_FRAMES) {
  if (!presentFixtures.includes(control.fixture)) {
    errors.push(`control frame '${control.name}' is missing its golden fixture ${control.fixture}`)
  }
}

// --- 3. A fixture that carries a `type` must match its declared type ---
for (const entry of LIVE_SERVER) {
  if (!entry.fixture) continue
  const fx = JSON.parse(fs.readFileSync(path.join(fixtureDir, entry.fixture), 'utf8'))
  if (fx.type !== entry.type) {
    errors.push(
      `fixture ${entry.fixture} declares type '${fx.type}' but is registered as '${entry.type}'`
    )
  }
}

// --- 4. Report the TS <-> Rust drift (informational until PR-A/PR-E) ---
// Extract the `type: '...'` variants from the ServerMessage union in protocol.ts.
// The character class MUST include `.` and `:` — live tags like
// `cli_image.updated`, `project_clone:status_update`, and `orchestration:*` use
// them, and omitting them would falsely report those existing types as missing.
const tsSource = fs.readFileSync(protocolTs, 'utf8')
const serverStart = tsSource.indexOf('export type ServerMessage')
const clientStart = tsSource.indexOf('export type ClientMessage')
const serverBlock = tsSource.slice(serverStart, clientStart >= 0 ? clientStart : undefined)
const tsServerTypes = new Set([...serverBlock.matchAll(/type:\s*'([a-z_.:]+)'/g)].map((m) => m[1]))

const liveTypes = new Set(LIVE_SERVER.map((m) => m.type))
// Dead: declared in TS but not a live Rust producer (legacy TS-server frames).
const dead = [...tsServerTypes].filter((t) => !liveTypes.has(t)).sort()
// Rust-only: a live producer the TS union does not declare — DERIVED from the
// parsed union, so a type that already exists in TS is never mis-flagged.
const rustOnly = [...liveTypes].filter((t) => !tsServerTypes.has(t)).sort()

notes.push(
  `ServerMessage drift snapshot (retired PR by PR — see docs/architecture/ms3-ws-protocol-baseline.md):`
)
notes.push(
  `  live producers pinned: ${LIVE_SERVER.length} (fixtures: ${LIVE_SERVER.filter((m) => m.fixture).length}, deferred: ${LIVE_SERVER.filter((m) => !m.fixture).length})`
)
notes.push(
  `  TS-declared-but-dead (${dead.length}, PR-A deletes): ${dead.join(', ') || '(none — done)'}`
)
notes.push(
  `  Rust-only-missing-from-TS (${rustOnly.length}, PR-A adds): ${rustOnly.join(', ') || '(none — done)'}`
)

// --- Output ---
console.log('WS protocol contract gate (MS-3)')
for (const n of notes) console.log(n)
if (errors.length > 0) {
  console.error(`\n✗ ${errors.length} fixture problem(s):`)
  for (const e of errors) console.error(`  - ${e}`)
  process.exit(1)
}
console.log(`\n✓ ${presentFixtures.length} golden fixtures locked; wire baseline intact.`)
