// Pure planners: every decision of deploy, recovery, retention and the frontend switch.
import { Buffer } from 'node:buffer'
import { preflightFindings } from './checks.js'
import { COMMITTED, TERMINAL, newAttempt } from './journal.js'

const IDENT = /^[A-Za-z0-9_]{1,63}$/
const ATTEMPT = /^[0-9a-z]{6,24}$/
const MAX_IDENT_BYTES = 63

function marked(db, kind, attemptId) {
  if (!IDENT.test(db) || !ATTEMPT.test(attemptId)) {
    throw new Error(`invalid database name or attempt id: ${db} / ${attemptId}`)
  }
  const name = `${db}__${kind}_${attemptId}`
  if (Buffer.byteLength(name) > MAX_IDENT_BYTES) {
    throw new Error(`database name exceeds the PostgreSQL limit of 63 bytes: ${name}`)
  }
  return name
}

export const snapshotName = (db, attemptId) => marked(db, 'snap', attemptId)
export const failedName = (db, attemptId) => marked(db, 'failed', attemptId)

// The engine has already handled "target is current" and "no current release"; it passes the
// start time, taken once and written with the intent.
export function planDeploy(config, state, observed, { at }) {
  const reasons = preflightFindings(config, state, observed)
  if (reasons.length > 0) {
    return { status: 'refused', reasons }
  }
  const { attemptId, target } = config
  const databases = config.databases.map((d) => ({
    key: d.key,
    source: d.name,
    sourceOid: observed.databases[d.name].oid,
    snapshot: snapshotName(d.name, attemptId),
    snapshotOid: null,
    failed: failedName(d.name, attemptId),
  }))
  const volumes = observed.volumes.map((volume) => ({
    volume,
    tar: `${config.snapshotDir}/${volume}.tar`,
    sha256: null,
    restored: false,
  }))
  const webroot = { tar: `${config.snapshotDir}/webroot.tar`, sha256: null, restored: false }
  const attempt = newAttempt({
    attemptId,
    target,
    previous: state.current,
    snapshot: { databases, volumes, webroot, expect: observed.expect },
    helperPrefix: `${config.project}-deploy-${attemptId}-`,
    inputs: null,
    at,
  })
  const snap = (op, d) => ({
    op,
    phase: 'snapshotting',
    db: d.key,
    name: d.source,
    snapshot: d.snapshot,
  })
  const steps = [
    { op: 'stop_project', phase: 'quiescing' },
    { op: 'check_no_agents', phase: 'quiescing' },
    ...databases.flatMap((d) =>
      [
        'disable_connections',
        'terminate_sessions',
        'create_snapshot',
        'enable_connections',
        'verify_snapshot',
      ].map((op) => snap(op, d))
    ),
    ...volumes.map((v) => ({
      op: 'archive_volume',
      phase: 'snapshotting',
      volume: v.volume,
      tar: v.tar,
    })),
    { op: 'archive_webroot', phase: 'snapshotting', tar: webroot.tar },
    { op: 'seal', phase: 'snapshotted' },
    { op: 'migrate', phase: 'migrating', rev: target },
    { op: 'start', phase: 'starting', rev: target },
    {
      op: 'verify_backend',
      phase: 'verifying_backend',
      rev: target,
      simulateFailure: config.simulateVerifyFailure,
    },
    { op: 'switch_frontend', phase: 'switching_frontend', rev: target },
    { op: 'verify_frontend', phase: 'verifying_frontend', rev: target },
    // Announcing this step IS the commit: one durable write moves phase and release pointers.
    { op: 'commit', phase: 'verified' },
    { op: 'prune', phase: 'verified', bestEffort: true },
    { op: 'close', phase: 'done' },
  ]
  return { status: 'plan', attempt, record: observed.record, steps, notes: observed.notes ?? [] }
}

export class RecoveryError extends Error {}

// Renames are derived from the observed name -> OID map, never replayed (A2).
function swapSteps(d, dbs) {
  const oid = (name) => dbs[name]?.oid
  const rb = (op, extra) => ({ op, phase: 'rolling_back', db: d.key, ...extra })
  const enable = rb('enable_connections', { name: d.source })
  if (oid(d.source) === d.snapshotOid) {
    return dbs[d.source].allowConnections ? [] : [enable]
  }
  const toSource = rb('rename', { from: d.snapshot, to: d.source })
  if (oid(d.source) === d.sourceOid && oid(d.snapshot) === d.snapshotOid && !dbs[d.failed]) {
    return [
      rb('disable_connections', { name: d.source }),
      rb('terminate_sessions', { name: d.source }),
      rb('rename', { from: d.source, to: d.failed }),
      toSource,
      enable,
    ]
  }
  if (!dbs[d.source] && oid(d.failed) === d.sourceOid && oid(d.snapshot) === d.snapshotOid) {
    return [toSource, enable]
  }
  throw new RecoveryError(
    `database ${d.source}: names and OIDs match no known swap state (snapshot ${d.snapshot} missing or moved)`
  )
}

function partialSnapshotSteps(d, dbs) {
  if (dbs[d.source]?.oid !== d.sourceOid) {
    throw new RecoveryError(`database ${d.source} is no longer the original (OID ${d.sourceOid})`)
  }
  const steps = []
  const copy = dbs[d.snapshot]
  if (copy && copy.oid !== d.sourceOid) {
    steps.push({ op: 'drop_database', phase: 'rolling_back', db: d.key, name: d.snapshot })
  }
  if (!dbs[d.source].allowConnections) {
    steps.push({ op: 'enable_connections', phase: 'rolling_back', db: d.key, name: d.source })
  }
  return steps
}

export function planRecovery(attempt, observed) {
  if (!attempt || TERMINAL.has(attempt.phase)) {
    return []
  }
  if (COMMITTED.has(attempt.phase)) {
    // Committed: never roll back. Make sure the committed release is running, then close.
    return [
      { op: 'start', phase: 'verified', rev: attempt.target },
      { op: 'verify_backend', phase: 'verified', rev: attempt.target },
      { op: 'close', phase: 'done' },
    ]
  }
  const rb = (step) => ({ ...step, phase: 'rolling_back' })
  const { snapshot, previous } = attempt
  const steps = [rb({ op: 'stop_project' })]
  if (snapshot.complete) {
    // Only the new release can have started agents: the API was stopped before the snapshot.
    steps.push(rb({ op: 'remove_new_agents', since: attempt.startedAt }))
    snapshot.databases.forEach((d) => steps.push(...swapSteps(d, observed.databases)))
    snapshot.volumes
      .filter((v) => !v.restored)
      .forEach((v) =>
        steps.push(rb({ op: 'restore_volume', volume: v.volume, tar: v.tar, sha256: v.sha256 }))
      )
  } else {
    snapshot.databases.forEach((d) => steps.push(...partialSnapshotSteps(d, observed.databases)))
  }
  steps.push(rb({ op: 'start', rev: previous }))
  if (snapshot.complete && !snapshot.webroot.restored) {
    steps.push(
      rb({ op: 'restore_webroot', tar: snapshot.webroot.tar, sha256: snapshot.webroot.sha256 })
    )
  }
  steps.push(rb({ op: 'verify_backend', rev: previous, against: 'snapshot' }))
  steps.push(rb({ op: 'verify_frontend', rev: previous }))
  steps.push({ op: 'close', phase: 'rolled_back' })
  return steps
}

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

// Only names this tool generates for the configured source databases are ever eligible:
// exactly `<source>__snap_<attempt>` / `<source>__failed_<attempt>`, or an attempt-id directory.
export function selectPrunable(items, state, activeAttempt, sources) {
  const attempts = new Set([state.protected.baseline, state.protected.lastGood, activeAttempt])
  const revs = new Set([state.current, state.previous, 'baseline'])
  // An input generation stays while any release record or the journal's attempt names it.
  const generations = new Set(
    [
      ...Object.values(state.records).map((r) => r.inputs?.generation),
      state.attempt?.inputs?.generation,
    ].filter(Boolean)
  )
  const generated = new RegExp(
    `^(?:${sources.map(escape).join('|')})__(?:snap|failed)_([0-9a-z]{6,24})$`
  )
  return items.filter((item) => {
    if (item.kind === 'database') {
      const m = sources.length > 0 ? generated.exec(item.name) : null
      return Boolean(m) && !attempts.has(m[1])
    }
    if (item.kind === 'snapshotDir') {
      return ATTEMPT.test(item.name) && !attempts.has(item.name)
    }
    if (item.kind === 'inputs') {
      return ATTEMPT.test(item.name) && !generations.has(item.name)
    }
    return item.kind === 'releaseDir' && Boolean(item.name) && !revs.has(item.name)
  })
}

// All changed assets first, `index.html` last, so a browser never loads an index whose assets
// are missing. Files only present in the web root are left in place for clients mid-load.
export function planFrontendSwitch(sourceFiles, targetFiles) {
  if (!sourceFiles.some((f) => f.path === 'index.html')) {
    throw new Error('frontend source has no index.html')
  }
  const changed = sourceFiles
    .filter((f) => targetFiles.get(f.path) !== f.sha256)
    .map((f) => f.path)
    .sort()
  const assets = changed.filter((p) => p !== 'index.html')
  return changed.includes('index.html') ? [...assets, 'index.html'] : assets
}
