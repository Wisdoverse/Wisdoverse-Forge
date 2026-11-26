// Fake world + executor for `deploy-release.mjs self-test` and the unit tests. Models what
// matters for safety: databases by name -> OID, connection flags, schema versions, volumes,
// web root, services and agent containers. Loaded only by self-test and tests.
import {
  Crash,
  EXIT,
  baseline,
  deploy,
  recover,
} from '../../../scripts/lib/deploy-release/engine.js'
import { createdVolumes, leftoverAdoptions } from '../../../scripts/lib/deploy-release/checks.js'
import { emptyState, releaseId } from '../../../scripts/lib/deploy-release/journal.js'

const DBS = [
  { key: 'api', role: 'api', name: 'forge', migrations: 'x' },
  { key: 'orchestrator', role: 'orchestrator', name: 'forge_orch', migrations: 'x' },
  { key: 'temporal', role: 'temporal', name: 'temporal' },
  { key: 'temporal_visibility', role: 'temporal', name: 'temporal_visibility' },
]
const T0 = '2026-11-24T00:00:00.000Z'
const agent = (uuid, created, running = true) => ({
  name: `agentforge-agent-${uuid}`,
  labels: { 'agentforge.agent_id': uuid },
  created,
  running,
})
// Created by the new release: one still running, one already stopped. Both are discarded state.
const NEW_AGENTS = [
  agent('00000000-0000-4000-8000-000000000002', '2026-11-24T00:05:00.000Z'),
  agent('00000000-0000-4000-8000-000000000003', '2026-11-24T00:06:00.000Z', false),
]

const IDENTITY = 'c'.repeat(64)
const PREV_INPUTS = {
  generation: 'prevgen1',
  envSha256: 'a'.repeat(64),
  identity: IDENTITY,
  runner: 'abcdef0',
  runnerImage: 'sha256:' + '1'.repeat(64),
}
// What Jenkins staged for revision `next`; the release id is (revision, env hash).
const STAGED = {
  envSha256: 'b'.repeat(64),
  identity: IDENTITY,
  runner: 'abcdef1',
  runnerImage: 'sha256:' + '1'.repeat(64),
}
export const NEXT = releaseId('next', STAGED.envSha256)
export const fakeRecord = (rev, inputs) => ({
  rev,
  revision: rev,
  config: `/private/configs/${rev}.json`,
  releaseDir: `/release/releases/${rev}`,
  indexSha256: `sha-${rev}`,
  frontendDir: rev === 'next' ? '/release/frontend/next' : null,
  images: { nats: 'sha256:1' },
  refs: { nats: 'nats:2' },
  ...(inputs ? { inputs } : {}),
})

export function createWorld() {
  const databases = {}
  DBS.forEach((d, i) => {
    databases[d.name] = { oid: 100 + i, allowConnections: true, schema: 'prev' }
  })
  const state = {
    ...emptyState(),
    current: 'prev',
    records: { prev: fakeRecord('prev', PREV_INPUTS) },
  }
  return {
    databases,
    originals: Object.fromEntries(DBS.map((d) => [d.name, databases[d.name].oid])),
    snapshots: {},
    nextOid: 200,
    running: 'prev',
    agents: [],
    volumes: { 'forge_nats-data': 'prev', 'forge_agentforge-uploads': 'prev' },
    // Named volumes that exist in Docker. The target adds forge_agentforge-compliance (R4).
    dockerVolumes: new Set(['forge_nats-data', 'forge_agentforge-uploads']),
    tars: {},
    webroot: 'prev',
    sqlHelper: null,
    generations: ['prevgen1'],
    sqlStarts: 0,
    stateJson: JSON.stringify(state),
  }
}

const VOLUME_CLASSES = {
  'nats-data': { class: 'snapshot' },
  'agentforge-uploads': { class: 'snapshot' },
  'agentforge-compliance': { class: 'excluded', reason: 'derived exports' },
}
export const CREATED_VOLUME = 'forge_agentforge-compliance'
const vol = (source, target) => ({ type: 'volume', source, target })
const DECLARED = {
  'nats-data': { name: 'forge_nats-data' },
  'agentforge-uploads': { name: 'forge_agentforge-uploads' },
  'agentforge-compliance': { name: CREATED_VOLUME },
}
// The running release mounts two volumes; the target adds the compliance volume to the API.
const PREV_RENDERED = {
  services: {
    nats: { container_name: 'forge-nats', volumes: [vol('nats-data', '/data')] },
    api: { container_name: 'forge-api', volumes: [vol('agentforge-uploads', '/var/lib/up')] },
  },
  volumes: {
    'nats-data': DECLARED['nats-data'],
    'agentforge-uploads': DECLARED['agentforge-uploads'],
  },
}
const TARGET_RENDERED = {
  services: {
    nats: PREV_RENDERED.services.nats,
    api: {
      container_name: 'forge-api',
      volumes: [
        vol('agentforge-uploads', '/var/lib/up'),
        vol('agentforge-compliance', '/var/lib/c'),
      ],
    },
  },
  volumes: DECLARED,
}
const PREV_CONTAINERS = [
  {
    name: 'forge-nats',
    project: 'forge',
    mounts: [{ type: 'volume', name: 'forge_nats-data', target: '/data' }],
  },
  {
    name: 'forge-api',
    project: 'forge',
    mounts: [{ type: 'volume', name: 'forge_agentforge-uploads', target: '/var/lib/up' }],
  },
]
export const fakeConfig = (overrides = {}) => ({
  target: 'next',
  attemptId: 'a1b2c3d4',
  project: 'forge',
  databases: DBS,
  simulateVerifyFailure: false,
  snapshotDir: '/private/snapshots/a1b2c3d4',
  volumeClasses: VOLUME_CLASSES,
  ...overrides,
})

export function goodPreflight(world) {
  const databases = {}
  for (const [name, d] of Object.entries(world.databases)) {
    databases[name] = {
      oid: d.oid,
      allowConnections: d.allowConnections,
      ownedByRole: true,
      ownerCreatedb: true,
    }
    Object.assign(databases[name], {
      defaultAcl: true,
      defaultTablespace: true,
      settings: 0,
      slots: 0,
    })
    Object.assign(databases[name], { subscriptions: 0, publications: 0, connLimit: -1 })
  }
  // A name in world.inspectFails answers like a transient daemon error, never as "absent".
  const volumeFacts = Object.fromEntries(
    Object.values(DECLARED).map(({ name }) => [
      name,
      world.inspectFails?.has(name)
        ? { exists: null, driver: null, error: 'daemon unreachable' }
        : { exists: world.dockerVolumes.has(name), driver: 'local' },
    ])
  )
  const adopted = leftoverAdoptions(
    VOLUME_CLASSES,
    { rendered: TARGET_RENDERED, volumeFacts },
    JSON.parse(world.stateJson)
  )
  return {
    envVersion: 'next',
    rendered: TARGET_RENDERED,
    prevRendered: PREV_RENDERED,
    prevContainers: PREV_CONTAINERS,
    volumeFacts,
    createdVolumes: createdVolumes(TARGET_RENDERED, volumeFacts, adopted),
    notes: adopted.map(
      (a) =>
        `adopting volume ${a.name} (class excluded), left by rolled-back attempt ${a.attemptId}`
    ),
    images: {},
    bindProbes: [],
    freeBytes: { private: 2 ** 40 },
    frontendIndexSha: 'sha-next',
    ledgers: {},
    targetMigrations: {},
    containers: PREV_CONTAINERS,
    databases,
    agentContainers: world.agents,
    volumes: Object.keys(world.volumes),
    expect: {
      endpoints: {},
      streams: [],
      consumers: [],
      ledgers: {
        api: [{ version: 1, checksum: 'aa' }],
        orchestrator: [{ version: 1, checksum: 'aa' }],
      },
    },
    captureProblems: [],
    rollback: { configReadable: true, releaseDirExists: true, bindProbes: [], missingImages: [] },
    tagIds: { 'nats:2': 'sha256:1' },
    record: fakeRecord('next'),
  }
}

// failAt: op indices that throw before acting. crash: { at, when: 'before' | 'after' } models
// SIGKILL around an op; crashWrite: index of a journal write after which the process dies.
export function fakeExecutor(world, opts = {}) {
  const { failAt = new Set(), crash = null, crashWrite = -1, agentAppears = -1 } = opts
  const {
    helperDiesAt = -1,
    stopFails = false,
    failWritesFrom = Infinity,
    envChanged = false,
    // Round 4: one failing write, before or after its rename, and a re-read that fails after it.
    failWrite = null,
    readFails = false,
    crashAfterActivate = false,
    // Round 5: what the re-read returns after a failed commit write, what Jenkins staged, and a
    // generation whose identity or secrets changed.
    rereadAs = null,
    staged = STAGED,
    identityChanged = false,
  } = opts
  let readBroken = false
  let { beforeRollback = null } = opts
  let calls = 0
  let writes = 0
  const dbs = world.databases
  const need = (name) => {
    if (!dbs[name]) {
      throw new Error(`database ${name} does not exist`)
    }
    return dbs[name]
  }
  const sources = () => DBS.map((d) => d.name)
  // Every SQL statement runs inside the one long-lived client helper (`docker exec`).
  const sql = () => {
    if (world.sqlHelper !== 'running') {
      throw new Error('SQL helper container is not running (No such container)')
    }
  }
  const SQL_OPS = new Set([
    'disable_connections',
    'enable_connections',
    'terminate_sessions',
    'create_snapshot',
    'verify_snapshot',
    'drop_database',
    'rename',
    'prune',
  ])
  const ops = {
    stop_project: () => {
      world.running = null
    },
    check_no_agents: () => {
      if (world.agents.some((a) => a.running)) {
        throw new Error('agent containers started after preflight')
      }
    },
    remove_new_agents: (s) => {
      world.agents = world.agents.filter((a) => Date.parse(a.created) <= Date.parse(s.since))
    },
    disable_connections: (s) => {
      need(s.name).allowConnections = false
    },
    enable_connections: (s) => {
      need(s.name).allowConnections = true
    },
    terminate_sessions: (s) => need(s.name),
    create_snapshot: (s) => {
      if (dbs[s.snapshot]) {
        throw new Error(`database ${s.snapshot} already exists`)
      }
      dbs[s.snapshot] = { oid: world.nextOid, allowConnections: true, schema: need(s.name).schema }
      world.snapshots[s.name] = world.nextOid
      world.nextOid += 1
      return { oid: dbs[s.snapshot].oid }
    },
    verify_snapshot: (s) => {
      if (need(s.snapshot).schema !== need(s.name).schema) {
        throw new Error('snapshot differs')
      }
    },
    archive_volume: (s) => {
      world.tars[s.volume] = world.volumes[s.volume]
      return { sha256: `sha-${world.tars[s.volume]}` }
    },
    archive_webroot: () => {
      world.tars.webroot = world.webroot
      return { sha256: `sha-${world.webroot}` }
    },
    migrate: (s) => {
      // `compose run` of the target creates the volumes its service mounts.
      world.dockerVolumes.add(CREATED_VOLUME)
      ;['forge', 'forge_orch'].forEach((name) => {
        need(name).schema = s.rev
      })
    },
    start: (s) => {
      world.running = s.rev
      if (s.rev === NEXT) {
        sources().forEach((name) => {
          need(name).schema = NEXT
        })
        Object.keys(world.volumes).forEach((v) => {
          world.volumes[v] = NEXT
        })
        world.agents = [...world.agents.filter((a) => !NEW_AGENTS.includes(a)), ...NEW_AGENTS]
      }
    },
    verify_backend: (s) => {
      const healthy = sources().every((n) => dbs[n]?.schema === s.rev && dbs[n].allowConnections)
      const volumes = Object.values(world.volumes).every((v) => v === s.rev)
      if (s.simulateFailure || world.running !== s.rev || !healthy || !volumes) {
        throw new Error(`backend verification of ${s.rev} failed`)
      }
    },
    switch_frontend: (s) => {
      world.webroot = s.rev
    },
    verify_frontend: (s) => {
      if (world.webroot !== s.rev) {
        throw new Error('frontend mismatch')
      }
    },
    drop_database: (s) => {
      delete dbs[s.name]
    },
    rename: (s) => {
      if (dbs[s.to]) {
        throw new Error(`database ${s.to} already exists`)
      }
      dbs[s.to] = need(s.from)
      delete dbs[s.from]
    },
    restore_volume: (s) => {
      world.volumes[s.volume] = world.tars[s.volume]
    },
    restore_webroot: () => {
      world.webroot = world.tars.webroot
    },
    prune: () => null,
  }
  return {
    readState: () => {
      if (readBroken) {
        throw new Error('EIO: journal could not be re-read')
      }
      if (rereadAs && failWrite && writes > failWrite.at) {
        const st = JSON.parse(world.stateJson)
        return {
          null: null,
          invalid: { schema: 1 },
          foreign: { ...st, attempt: { ...st.attempt, attemptId: 'z9z9z9z9' } },
        }[rereadAs]
      }
      return JSON.parse(world.stateJson)
    },
    writeState: (state) => {
      if (failWrite?.at === writes) {
        writes += 1
        readBroken = readFails
        if (failWrite.mode === 'after-rename') {
          world.stateJson = JSON.stringify(state)
          throw new Error('EIO: fsync of the state directory failed after the rename')
        }
        throw new Error('EIO: writing the temporary journal failed')
      }
      // A persistent write error (ENOSPC): nothing is stored from this write on.
      if (writes >= failWritesFrom) {
        writes += 1
        throw new Error('ENOSPC: no space left on device')
      }
      world.stateJson = JSON.stringify(state)
      writes += 1
      if (writes - 1 === crashWrite) {
        throw new Crash(`crash after journal write ${crashWrite}`)
      }
    },
    removeHelpers: () => {
      world.sqlHelper = null
    },
    readStaged: () => {
      if (!/^[0-9a-f]{7,40}$/.test(staged.runner)) {
        throw new Error('the staged runner reference is not a commit id')
      }
      return { ...staged }
    },
    activateStaged: (generation) => {
      world.generations.push(generation)
      if (crashAfterActivate) {
        throw new Crash('crash after the staged inputs were renamed')
      }
      return { generation, ...staged }
    },
    inputsFacts: (generation) => {
      if (!world.generations.includes(generation)) {
        return null
      }
      const recorded = generation === 'prevgen1' ? PREV_INPUTS : staged
      return {
        envSha256: envChanged ? 'f'.repeat(64) : recorded.envSha256,
        identity: identityChanged ? 'e'.repeat(64) : recorded.identity,
      }
    },
    baseline: () => ({ findings: [], record: fakeRecord('baseline') }),
    startSql: () => {
      if (world.sqlHelper) {
        throw new Error('SQL helper name already in use')
      }
      world.sqlHelper = 'running'
      world.sqlStarts += 1
    },
    stopSql: () => {
      if (stopFails) {
        throw new Error('SQL helper still present after removal')
      }
      world.sqlHelper = null
    },
    preflight: () => {
      sql()
      return goodPreflight(world)
    },
    observe: () => {
      sql()
      return observed()
    },
    run: (step) => {
      SEEN_OPS.add(step.op)
      const op = ops[step.op]
      if (!op) {
        return {}
      }
      const k = calls
      calls += 1
      if (k === agentAppears) {
        world.agents.push(agent('00000000-0000-4000-8000-000000000001', '2026-11-23T23:59:00.000Z'))
      }
      if (k === helperDiesAt) {
        world.sqlHelper = null
      }
      if (step.phase === 'rolling_back' && beforeRollback) {
        beforeRollback(world)
        beforeRollback = null
      }
      if (failAt.has(k)) {
        throw new Error(`injected failure at ${step.op}`)
      }
      if (crash?.at === k && crash.when === 'before') {
        throw new Crash(`crash before ${step.op}`)
      }
      if (SQL_OPS.has(step.op)) {
        sql()
      }
      const result = op(step) ?? {}
      if (crash?.at === k) {
        throw new Crash(`crash after ${step.op}`)
      }
      return result
    },
    calls: () => calls,
    writes: () => writes,
  }
  function observed() {
    return {
      databases: Object.fromEntries(
        Object.entries(dbs).map(([n, d]) => [
          n,
          { oid: d.oid, allowConnections: d.allowConnections },
        ])
      ),
    }
  }
}

export function invariantErrors(world) {
  const errors = []
  const oids = new Set(Object.values(world.databases).map((d) => d.oid))
  for (const [name, oid] of Object.entries(world.originals)) {
    if (!oids.has(oid)) {
      errors.push(`original ${name} lost`)
    }
    const here = world.databases[name]?.oid
    if (here !== oid && here !== world.snapshots[name]) {
      errors.push(`${name} is neither the original nor its snapshot`)
    }
  }
  return errors
}

export function outcomeErrors(world, wanted) {
  const rev = wanted === 'next' ? NEXT : wanted
  const errors = invariantErrors(world)
  const state = JSON.parse(world.stateJson)
  const want = rev === NEXT ? 'done' : 'rolled_back'
  if (state.attempt?.phase !== want) {
    errors.push(`phase ${state.attempt?.phase}, expected ${want}`)
  }
  const dbsOk = DBS.every(
    (d) => world.databases[d.name]?.schema === rev && world.databases[d.name].allowConnections
  )
  if (world.running !== rev || world.webroot !== rev || !dbsOk) {
    errors.push(`world is not fully at ${rev}`)
  }
  if (state.current !== rev) {
    errors.push(`current release is ${state.current}, expected ${rev}`)
  }
  if (world.sqlHelper) {
    errors.push('the SQL helper container was left running')
  }
  // R4 (round 8): the tool never deletes a volume; a rolled-back attempt's volume stays (status
  // lists it, and a retry adopts it only when it is class `excluded`).
  if (rev !== 'prev' && !world.dockerVolumes.has(CREATED_VOLUME)) {
    errors.push(`volume ${CREATED_VOLUME} of the new release is missing`)
  }
  if (!['forge_nats-data', 'forge_agentforge-uploads'].every((v) => world.dockerVolumes.has(v))) {
    errors.push('a volume that existed before the attempt was removed')
  }
  if (rev === 'prev' && world.agents.some((a) => NEW_AGENTS.includes(a))) {
    errors.push('an agent container of the discarded release survived the rollback')
  }
  return errors
}

export const ctxFor = (world, opts, config = fakeConfig()) => ({
  config,
  exec: fakeExecutor(world, opts),
  log: () => null,
  now: () => T0,
})

async function crashThenRecover(world, opts) {
  try {
    await deploy(ctxFor(world, opts))
    return 'no crash'
  } catch (err) {
    if (!(err instanceof Crash)) {
      throw err
    }
  }
  return recover(ctxFor(world, {}))
}

// Crash the run that `seed` describes, then interrupt `recover` after each of its journal writes
// and check that one more `recover` ends at `rev`.
async function recoverSweep(label, seed, rev) {
  const crashed = async () => {
    const world = createWorld()
    await deploy(ctxFor(world, seed)).catch((err) => {
      if (!(err instanceof Crash)) {
        throw err
      }
    })
    return world
  }
  const probeWorld = await crashed()
  const probeCtx = ctxFor(probeWorld, {})
  await recover(probeCtx)
  for (let w = 0; w < probeCtx.exec.writes(); w += 1) {
    const world = await crashed()
    await recover(ctxFor(world, { crashWrite: w })).catch((err) => {
      if (!(err instanceof Crash)) {
        throw err
      }
    })
    const code = await recover(ctxFor(world, {}))
    const errors = outcomeErrors(world, rev)
    if (code !== EXIT.OK) {
      errors.push(`exit ${code}, expected 0`)
    }
    sweepResults.push({
      name: `${label}: recover interrupted after its write ${w}, then recover`,
      errors,
    })
  }
}
let sweepResults = []

const committedPhase = (world) =>
  ['verified', 'done'].includes(JSON.parse(world.stateJson).attempt?.phase)

// Every step op any scenario ran (round 8 review: no deploy, rollback or recover path removes a
// volume). archive_volume and restore_volume copy contents; neither removes a volume.
const SEEN_OPS = new Set()
export const VOLUME_OPS_ALLOWED = ['archive_volume', 'restore_volume']

export async function faultMatrix() {
  sweepResults = []
  SEEN_OPS.clear()
  const probe = createWorld()
  const probeCtx = ctxFor(probe, {})
  await deploy(probeCtx)
  const [total, totalWrites] = [probeCtx.exec.calls(), probeCtx.exec.writes()]
  const results = [{ name: 'happy path', errors: outcomeErrors(probe, 'next') }]
  const add = (name, world, code, wantCode, rev) => {
    const errors = outcomeErrors(world, rev)
    if (code !== wantCode) {
      errors.push(`exit ${code}, expected ${wantCode}`)
    }
    results.push({ name, errors })
  }
  const simulated = createWorld()
  const simCode = await deploy(ctxFor(simulated, {}, fakeConfig({ simulateVerifyFailure: true })))
  add('SIMULATE_VERIFY_FAILURE', simulated, simCode, EXIT.ROLLED_BACK, 'prev')
  for (let k = 0; k < total; k += 1) {
    const committed = k === total - 1
    const rev = committed ? 'next' : 'prev'
    const failed = createWorld()
    const failCode = await deploy(ctxFor(failed, { failAt: new Set([k]) }))
    add(`fail at op ${k}`, failed, failCode, committed ? EXIT.OK : EXIT.ROLLED_BACK, rev)
    for (const when of ['before', 'after']) {
      const crashed = createWorld()
      add(
        `crash ${when} op ${k}, then recover`,
        crashed,
        await crashThenRecover(crashed, { crash: { at: k, when } }),
        EXIT.OK,
        rev
      )
    }
    if (!committed) {
      const twice = createWorld()
      const code = await deploy(ctxFor(twice, { failAt: new Set([k, k + 1]) }))
      const stuck = JSON.parse(twice.stateJson).attempt.phase
      const errors = invariantErrors(twice)
      if (code !== EXIT.RECOVERY_REQUIRED || stuck !== 'recovery_required') {
        errors.push(`expected recovery_required, got exit ${code} phase ${stuck}`)
      }
      results.push({ name: `fail at op ${k} and during rollback`, errors })
      add(
        `recover after op ${k} double failure`,
        twice,
        await recover(ctxFor(twice, {})),
        EXIT.OK,
        'prev'
      )
    }
  }
  // Finding 1: whatever journal write the process dies after, recovery must agree with it.
  for (let w = 0; w < totalWrites; w += 1) {
    const world = createWorld()
    let code = null
    try {
      await deploy(ctxFor(world, { crashWrite: w }))
    } catch (err) {
      if (!(err instanceof Crash)) {
        throw err
      }
      const rev = committedPhase(world) ? 'next' : 'prev'
      code = await recover(ctxFor(world, {}))
      add(`crash after journal write ${w}, then recover`, world, code, EXIT.OK, rev)
    }
    if (code === null) {
      results.push({
        name: `crash after journal write ${w}`,
        errors: ['the crash was not injected'],
      })
    }
  }
  // The SQL helper disappears before op k: the step fails cleanly, rollback starts a new helper.
  for (let k = 0; k < total; k += 1) {
    const world = createWorld()
    const code = await deploy(ctxFor(world, { helperDiesAt: k }))
    const rev = code === EXIT.OK ? 'next' : 'prev'
    const errors = outcomeErrors(world, rev)
    if (![EXIT.OK, EXIT.ROLLED_BACK].includes(code)) {
      errors.push(`exit ${code}, expected 0 or 10`)
    }
    results.push({ name: `SQL helper gone before op ${k}`, errors })
  }
  // A crash leaves the helper running; recover must remove it before it plans (checked above via
  // outcomeErrors). A helper that cannot be removed at the end is reported, not ignored.
  const sticky = createWorld()
  const stickyCode = await deploy(ctxFor(sticky, { stopFails: true }))
  results.push({
    name: 'SQL helper removal fails at the end',
    errors:
      stickyCode === EXIT.FINALIZE_FAILED
        ? []
        : [`exit ${stickyCode}, expected ${EXIT.FINALIZE_FAILED}`],
  })
  // Round-3 finding 9: crash after EVERY journal write of the rollback and recover paths, in both
  // regimes. The seeds fail at a snapshot step (regime 1) and at backend verification (regime 2).
  const seeds = [
    ['regime 1', { failAt: new Set([4]) }],
    ['regime 2', { failAt: new Set([27]) }],
  ]
  for (const [regime, seed] of seeds) {
    const probeWorld = createWorld()
    const probeExec = ctxFor(probeWorld, seed)
    await deploy(probeExec)
    for (let w = 0; w < probeExec.exec.writes(); w += 1) {
      const world = createWorld()
      try {
        await deploy(ctxFor(world, { ...seed, crashWrite: w }))
        continue
      } catch (err) {
        if (!(err instanceof Crash)) {
          throw err
        }
      }
      const rev = committedPhase(world) ? 'next' : 'prev'
      add(
        `${regime}: crash after write ${w} of deploy+rollback, then recover`,
        world,
        await recover(ctxFor(world, {})),
        EXIT.OK,
        rev
      )
    }
    await recoverSweep(
      `${regime} (snapshot incomplete)`,
      { crash: { at: 12, when: 'before' } },
      'prev'
    )
  }
  // Round 4: interrupted recovery of a COMPLETE snapshot (full rollback) and of a COMMITTED attempt.
  await recoverSweep('complete snapshot', { crash: { at: 27, when: 'before' } }, 'prev')
  const phases = []
  const phaseProbe = createWorld()
  const phaseCtx = ctxFor(phaseProbe, {})
  const write = phaseCtx.exec.writeState
  phaseCtx.exec.writeState = (st) => {
    phases.push(st.attempt?.phase)
    return write(st)
  }
  await deploy(phaseCtx)
  const commitWrite = phases.indexOf('verified')
  await recoverSweep('committed attempt', { crashWrite: commitWrite }, 'next')

  // Round 4, C1: the commit write fails. Rename done but not durable, re-read failing, or both:
  // the outcome is unknown, nothing is stopped; recover decides from the journal on disk.
  for (const [name, opts, recoveredTo] of [
    [
      'directory fsync fails after the rename',
      { failWrite: { at: commitWrite, mode: 'after-rename' } },
      'next',
    ],
    [
      'write fails before the rename, re-read fails',
      { failWrite: { at: commitWrite, mode: 'before-rename' }, readFails: true },
      'prev',
    ],
    [
      'both: rename done, fsync and re-read fail',
      { failWrite: { at: commitWrite, mode: 'after-rename' }, readFails: true },
      'next',
    ],
  ]) {
    const world = createWorld()
    const code = await deploy(ctxFor(world, opts))
    const errors = []
    if (code !== EXIT.COMMIT_UNKNOWN || world.running !== NEXT) {
      errors.push(
        `exit ${code} (want ${EXIT.COMMIT_UNKNOWN}), running ${world.running} (want next)`
      )
    }
    const recovered = await recover(ctxFor(world, {}))
    errors.push(
      ...outcomeErrors(world, recoveredTo),
      ...(recovered === EXIT.OK ? [] : [`recover exit ${recovered}`])
    )
    results.push({ name: `commit outcome unknown: ${name}`, errors })
  }
  const provable = createWorld()
  const provableCode = await deploy(
    ctxFor(provable, { failWrite: { at: commitWrite, mode: 'before-rename' } })
  )
  add(
    'commit write failed before the rename, journal re-read shows pre-commit',
    provable,
    provableCode,
    EXIT.ROLLED_BACK,
    'prev'
  )

  // Round 4, C2: a crash right after the staged inputs became a generation, before the intent.
  const orphan = createWorld()
  await deploy(ctxFor(orphan, { crashAfterActivate: true })).catch((err) => {
    if (!(err instanceof Crash)) {
      throw err
    }
  })
  const orphanState = JSON.parse(orphan.stateJson)
  const orphanErrors = []
  if (orphanState.attempt !== null || orphanState.records.prev.inputs.generation !== 'prevgen1') {
    orphanErrors.push('the journal moved although the intent was never written')
  }
  if ((await recover(ctxFor(orphan, {}))) !== EXIT.OK || orphan.running !== 'prev') {
    orphanErrors.push('recover did not leave the previous release running')
  }
  if ((await deploy(ctxFor(orphan, {}, fakeConfig({ attemptId: 'b1b2c3d4' })))) !== EXIT.OK) {
    orphanErrors.push('a re-staged deploy after the crash did not succeed')
  }
  results.push({ name: 'crash after activating inputs, before the intent', errors: orphanErrors })
  // First-ever baseline: a crash after the rename leaves no release; re-staging lets it run again.
  const fresh = createWorld()
  fresh.stateJson = JSON.stringify(emptyState())
  await baseline(
    ctxFor(fresh, { crashAfterActivate: true }, fakeConfig({ attemptId: 'base0001' }))
  ).catch(() => null)
  const freshErrors = JSON.parse(fresh.stateJson).current
    ? ['a crashed baseline recorded a release']
    : []
  if ((await baseline(ctxFor(fresh, {}, fakeConfig({ attemptId: 'base0002' })))) !== EXIT.OK) {
    freshErrors.push('baseline did not run again after re-staging')
  }
  const based = JSON.parse(fresh.stateJson)
  if (based.activeGeneration !== 'base0002' || based.activeRunner !== 'abcdef1') {
    freshErrors.push('the journal pointer does not name the baseline generation and runner')
  }
  results.push({ name: 'first baseline interrupted after activating inputs', errors: freshErrors })

  // Round-3 finding 2: a persistent journal write failure from write w on. Before the commit the
  // deploy fails (rolled back later by recover); from the commit on the committed release keeps
  // running, nothing is stopped, and recover confirms and closes it.
  for (let w = 1; w < totalWrites; w += 1) {
    const world = createWorld()
    const code = await deploy(ctxFor(world, { failWritesFrom: w }))
    const onDisk = committedPhase(world)
    const errors = []
    if (onDisk && (code !== EXIT.FINALIZE_FAILED || world.running !== NEXT)) {
      errors.push(
        `after commit: exit ${code} (want ${EXIT.FINALIZE_FAILED}), running ${world.running}`
      )
    }
    if (!onDisk && code === EXIT.OK) {
      errors.push('a deploy whose commit never reached disk reported success')
    }
    const recovered = await recover(ctxFor(world, {}))
    errors.push(...outcomeErrors(world, onDisk ? 'next' : 'prev'))
    if (recovered !== EXIT.OK) {
      errors.push(`recover exit ${recovered}`)
    }
    results.push({ name: `journal writes fail from write ${w}`, errors })
  }
  // A committed attempt whose stack went down is started again by recover before it closes.
  const down = createWorld()
  await deploy(ctxFor(down, { failWritesFrom: totalWrites - 2 }))
  down.running = null
  add(
    'committed attempt with the stack down, then recover',
    down,
    await recover(ctxFor(down, {})),
    EXIT.OK,
    'next'
  )
  // Round 5, F4b: a rotated secret in the attempt's env does not block recover; a changed
  // deployment identity does.
  const rotated = createWorld()
  await deploy(ctxFor(rotated, { crash: { at: 12, when: 'before' } })).catch(() => null)
  add(
    'recover after a credential in the attempt env was rotated',
    rotated,
    await recover(ctxFor(rotated, { envChanged: true })),
    EXIT.OK,
    'prev'
  )
  const moved = createWorld()
  await deploy(ctxFor(moved, { crash: { at: 12, when: 'before' } })).catch(() => null)
  const movedCode = await recover(ctxFor(moved, { identityChanged: true }))
  results.push({
    name: 'recover when the attempt env names another deployment',
    errors:
      movedCode === EXIT.RECOVERY_REQUIRED
        ? []
        : [`exit ${movedCode}, expected ${EXIT.RECOVERY_REQUIRED}`],
  })
  // Round 5, F1: after a failed commit write, "not committed" needs a valid journal of the same
  // attempt that still shows the pre-commit phase. Absent, invalid or foreign is unknown.
  for (const rereadAs of ['null', 'invalid', 'foreign']) {
    const world = createWorld()
    const code = await deploy(
      ctxFor(world, { failWrite: { at: commitWrite, mode: 'before-rename' }, rereadAs })
    )
    const errors =
      code === EXIT.COMMIT_UNKNOWN && world.running === NEXT
        ? []
        : [`exit ${code}, running ${world.running}`]
    const recovered = await recover(ctxFor(world, {}))
    errors.push(
      ...outcomeErrors(world, 'prev'),
      ...(recovered === EXIT.OK ? [] : [`recover exit ${recovered}`])
    )
    results.push({
      name: `commit write failed, re-read ${rereadAs}: unknown, nothing stopped`,
      errors,
    })
  }
  // Round 5, F4a: same revision, other env content: a normal deploy of a new release.
  const sameRev = createWorld()
  const prevRecord = JSON.parse(sameRev.stateJson)
  prevRecord.records.prev.revision = 'next'
  sameRev.stateJson = JSON.stringify(prevRecord)
  const noop = await deploy(
    ctxFor(sameRev, { staged: { ...STAGED, envSha256: PREV_INPUTS.envSha256 } })
  )
  const rotatedDeploy = await deploy(ctxFor(sameRev, {}, fakeConfig({ attemptId: 'b1b2c3d4' })))
  const sameRevState = JSON.parse(sameRev.stateJson)
  results.push({
    name: 'release identity: same revision and env is a no-op; a changed env deploys',
    errors:
      noop === EXIT.OK &&
      rotatedDeploy === EXIT.OK &&
      sameRevState.current === NEXT &&
      sameRevState.previous === 'prev'
        ? []
        : [`no-op exit ${noop}, deploy exit ${rotatedDeploy}, current ${sameRevState.current}`],
  })
  // Round 5, F3: an invalid staged runner refuses before anything is persisted.
  const badRunner = createWorld()
  const before = badRunner.stateJson
  const badCode = await deploy(ctxFor(badRunner, { staged: { ...STAGED, runner: 'not-a-commit' } }))
  results.push({
    name: 'invalid staged runner refuses before anything is persisted',
    errors:
      badCode === EXIT.PREFLIGHT_REFUSED &&
      badRunner.stateJson === before &&
      badRunner.generations.length === 1
        ? []
        : [`exit ${badCode}`],
  })
  // Finding 8: an agent that appears after preflight aborts before the snapshot, untouched.
  const appeared = createWorld()
  const appearedCode = await deploy(ctxFor(appeared, { agentAppears: 0 }))
  const appearedErrors = outcomeErrors(appeared, 'prev')
  if (
    appearedCode !== EXIT.ROLLED_BACK ||
    appeared.agents.length !== 1 ||
    Object.keys(appeared.snapshots).length
  ) {
    appearedErrors.push('expected a rollback before any snapshot, keeping the pre-existing agent')
  }
  results.push({ name: 'agent appears after preflight', errors: appearedErrors })
  // Round 8 review R4: after a rollback the created (excluded-class) volume stays, and the retry
  // adopts it from the journal instead of refusing.
  const retried = createWorld()
  const firstCode = await deploy(ctxFor(retried, {}, fakeConfig({ simulateVerifyFailure: true })))
  const leftOver = retried.dockerVolumes.has(CREATED_VOLUME)
  const retryCode = await deploy(ctxFor(retried, {}))
  results.push({
    name: 'a rolled-back attempt leaves its excluded volume; the retry adopts it',
    errors:
      firstCode === EXIT.ROLLED_BACK &&
      leftOver &&
      retryCode === EXIT.OK &&
      JSON.parse(retried.stateJson).current === NEXT &&
      JSON.parse(retried.stateJson).attempt.createdVolumes.includes(CREATED_VOLUME)
        ? []
        : [`first exit ${firstCode}, left over ${leftOver}, retry exit ${retryCode}`],
  })
  const all = [...results, ...sweepResults]
  const removing = [...SEEN_OPS].filter(
    (op) => /volume/.test(op) && !VOLUME_OPS_ALLOWED.includes(op)
  )
  all.push({
    name: `no step of any scenario removes a volume (${SEEN_OPS.size} distinct ops seen)`,
    errors: removing.map((op) => `step ${op} touches volumes`),
  })
  return all
}
