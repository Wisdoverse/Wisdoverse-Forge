// Write-ahead deployment journal (design A1). Pure: no I/O here; the effect layer persists the
// state object with write-temp + fsync + rename + fsync(dir) before each announced action.

export const STATE_SCHEMA = 1

export const TERMINAL = new Set(['done', 'rolled_back'])
// After `verified` the new release is committed; recovery only finishes the attempt.
export const COMMITTED = new Set(['verified'])

const TRANSITIONS = {
  quiescing: ['snapshotting', 'rolling_back'],
  snapshotting: ['snapshotted', 'rolling_back'],
  snapshotted: ['migrating', 'rolling_back'],
  migrating: ['starting', 'rolling_back'],
  starting: ['verifying_backend', 'rolling_back'],
  verifying_backend: ['switching_frontend', 'rolling_back'],
  switching_frontend: ['verifying_frontend', 'rolling_back'],
  verifying_frontend: ['verified', 'rolling_back'],
  verified: ['done'],
  rolling_back: ['rolled_back', 'recovery_required'],
  recovery_required: ['rolling_back'],
  done: [],
  rolled_back: [],
}

export const PHASES = Object.keys(TRANSITIONS)

// A state file that exists but cannot be parsed must stop everything (finding 2).
export class StateUnreadable extends Error {}

export function emptyState() {
  return {
    schema: STATE_SCHEMA,
    current: null,
    previous: null,
    protected: { baseline: null, lastGood: null },
    records: {},
    attempt: null,
  }
}

export function transition(from, to) {
  if (from === to) {
    return to
  }
  if (!TRANSITIONS[from]?.includes(to)) {
    throw new Error(`illegal journal transition ${from} -> ${to}`)
  }
  return to
}

export function newAttempt({ attemptId, target, previous, snapshot, helperPrefix, at }) {
  return {
    attemptId,
    target,
    previous,
    phase: 'quiescing',
    step: null,
    snapshot: { complete: false, ...snapshot },
    swaps: [],
    helperPrefix,
    startedAt: at,
    updatedAt: at,
    error: null,
  }
}

const clone = (value) => JSON.parse(JSON.stringify(value))

// Called BEFORE the step runs: the persisted journal names the action about to happen.
export function announce(state, step, at) {
  const next = clone(state)
  const a = next.attempt
  const committing = step.phase === 'verified' && a.phase !== 'verified'
  a.phase = transition(a.phase, step.phase)
  a.step = step
  a.updatedAt = at
  if (committing) {
    commitRelease(next)
  }
  return next
}

// Called AFTER the step succeeded: records its results and applies journal-only steps.
export function complete(state, step, result = {}, at) {
  const next = clone(state)
  const a = next.attempt
  const db = a.snapshot.databases.find((d) => d.key === step.db)
  const volume = a.snapshot.volumes.find((v) => v.volume === step.volume)
  switch (step.op) {
    case 'create_snapshot':
      db.snapshotOid = result.oid
      break
    case 'archive_volume':
      volume.sha256 = result.sha256
      break
    case 'archive_webroot':
      a.snapshot.webroot.sha256 = result.sha256
      break
    case 'restore_volume':
      volume.restored = true
      break
    case 'restore_webroot':
      a.snapshot.webroot.restored = true
      break
    case 'rename':
      a.swaps.push({ db: step.db, from: step.from, to: step.to })
      break
    case 'seal':
      a.snapshot.complete = true
      break
    default:
      break
  }
  a.step = null
  a.updatedAt = at
  return next
}

function commitRelease(state) {
  const a = state.attempt
  if (a.previous === 'baseline' && !state.protected.baseline) {
    state.protected.baseline = a.attemptId
  }
  state.protected.lastGood = a.attemptId
  state.previous = a.previous
  state.current = a.target
  const keep = new Set([state.current, state.previous, 'baseline'])
  for (const rev of Object.keys(state.records)) {
    if (!keep.has(rev)) {
      delete state.records[rev]
    }
  }
}

export function markRecoveryRequired(state, error, at) {
  const next = clone(state)
  const a = next.attempt
  a.phase = transition(transition(a.phase, 'rolling_back'), 'recovery_required')
  a.error = error
  a.updatedAt = at
  return next
}

export function summarize(state) {
  const a = state.attempt
  const lines = [
    `current release:  ${state.current ?? '(none - run baseline first)'}`,
    `previous release: ${state.previous ?? '(none)'}`,
    `recorded releases: ${Object.keys(state.records).join(', ') || '(none)'}`,
    `protected snapshots: baseline=${state.protected.baseline ?? '-'} last-good=${state.protected.lastGood ?? '-'}`,
  ]
  if (!a) {
    return [...lines, 'last attempt: (none)']
  }
  const { databases, volumes, webroot } = a.snapshot
  return [
    ...lines,
    `attempt: ${a.attemptId} ${a.previous} -> ${a.target}`,
    `phase: ${a.phase}${a.step ? ` (interrupted at ${a.step.op})` : ''}`,
    `helper containers: names start with ${a.helperPrefix}`,
    `snapshot: ${a.snapshot.complete ? 'complete' : 'incomplete'}`,
    ...databases.map(
      (d) =>
        `  ${d.key}: ${d.source} (oid ${d.sourceOid}) snapshot ${d.snapshot} (oid ${d.snapshotOid ?? 'pending'}) failed-name ${d.failed}`
    ),
    ...volumes.map((v) => `  volume ${v.volume}: ${v.tar}${v.restored ? ' (restored)' : ''}`),
    `  web root: ${webroot.tar}${webroot.restored ? ' (restored)' : ''}`,
    `started ${a.startedAt}, updated ${a.updatedAt}`,
    ...(a.error ? [`error: ${a.error}`] : []),
  ]
}
