// Fake world + executor for `deploy-release.mjs self-test` and the unit tests. Models what
// matters for safety: databases by name -> OID, connection flags, schema versions, volumes,
// web root, services and agent containers. Loaded only by self-test and tests.
import { Crash, EXIT, deploy, recover } from '../../../scripts/lib/deploy-release/engine.js'
import { emptyState } from '../../../scripts/lib/deploy-release/journal.js'

const DBS = [
  { key: 'api', role: 'api', name: 'forge' },
  { key: 'orchestrator', role: 'orchestrator', name: 'forge_orch' },
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

export function createWorld() {
  const databases = {}
  DBS.forEach((d, i) => {
    databases[d.name] = { oid: 100 + i, allowConnections: true, schema: 'prev' }
  })
  const state = { ...emptyState(), current: 'prev', records: { prev: { rev: 'prev', images: {} } } }
  return {
    databases,
    originals: Object.fromEntries(DBS.map((d) => [d.name, databases[d.name].oid])),
    snapshots: {},
    nextOid: 200,
    running: 'prev',
    agents: [],
    volumes: { 'forge_nats-data': 'prev', 'forge_agentforge-uploads': 'prev' },
    tars: {},
    webroot: 'prev',
    sqlHelper: null,
    sqlStarts: 0,
    stateJson: JSON.stringify(state),
  }
}

export const fakeConfig = (overrides = {}) => ({
  target: 'next',
  attemptId: 'a1b2c3d4',
  project: 'forge',
  databases: DBS,
  simulateVerifyFailure: false,
  snapshotDir: '/private/snapshots/a1b2c3d4',
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
  return {
    envVersion: 'next',
    rendered: { services: {} },
    images: {},
    bindProbes: [],
    freeBytes: { private: 2 ** 40 },
    frontendIndexSha: 'sha-next',
    ledgers: {},
    targetMigrations: {},
    containers: [],
    databases,
    agentContainers: world.agents,
    volumes: Object.keys(world.volumes),
    expect: { endpoints: {}, streams: [], consumers: [], ledgers: {} },
    captureProblems: [],
    rollback: { configReadable: true, releaseDirExists: true, bindProbes: [], missingImages: [] },
    tagIds: {},
    record: { rev: 'next', images: {} },
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
  } = opts
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
      ;['forge', 'forge_orch'].forEach((name) => {
        need(name).schema = s.rev
      })
    },
    start: (s) => {
      world.running = s.rev
      if (s.rev === 'next') {
        sources().forEach((name) => {
          need(name).schema = 'next'
        })
        Object.keys(world.volumes).forEach((v) => {
          world.volumes[v] = 'next'
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
    readState: () => JSON.parse(world.stateJson),
    writeState: (state) => {
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
    promoteInputs: () => ({ envSha256: 'env-a' }),
    activeInputs: () => ({ envSha256: envChanged ? 'env-b' : 'env-a' }),
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

export function outcomeErrors(world, rev) {
  const errors = invariantErrors(world)
  const state = JSON.parse(world.stateJson)
  const want = rev === 'next' ? 'done' : 'rolled_back'
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

const committedPhase = (world) =>
  ['verified', 'done'].includes(JSON.parse(world.stateJson).attempt?.phase)

export async function faultMatrix() {
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
    // The recover path itself: crash before op 12, then crash recover after each of its writes.
    const crashed = () => {
      const world = createWorld()
      return deploy(ctxFor(world, { crash: { at: 12, when: 'before' } })).then(
        () => world,
        () => world
      )
    }
    const recoverProbe = await crashed()
    const recoverCtx = ctxFor(recoverProbe, {})
    await recover(recoverCtx)
    for (let w = 0; w < recoverCtx.exec.writes(); w += 1) {
      const world = await crashed()
      await recover(ctxFor(world, { crashWrite: w })).catch((err) => {
        if (!(err instanceof Crash)) {
          throw err
        }
      })
      add(
        `${regime}: recover interrupted after its write ${w}, then recover`,
        world,
        await recover(ctxFor(world, {})),
        EXIT.OK,
        'prev'
      )
    }
  }
  // Round-3 finding 2: a persistent journal write failure from write w on. Before the commit the
  // deploy fails (rolled back later by recover); from the commit on the committed release keeps
  // running, nothing is stopped, and recover confirms and closes it.
  for (let w = 1; w < totalWrites; w += 1) {
    const world = createWorld()
    const code = await deploy(ctxFor(world, { failWritesFrom: w }))
    const onDisk = committedPhase(world)
    const errors = []
    if (onDisk && (code !== EXIT.FINALIZE_FAILED || world.running !== 'next')) {
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
  // Round-3 finding 5: recover refuses when the active env is not the one the attempt started with.
  const swapped = createWorld()
  await deploy(ctxFor(swapped, { crash: { at: 12, when: 'before' } })).catch(() => null)
  const swappedCode = await recover(ctxFor(swapped, { envChanged: true }))
  results.push({
    name: 'recover with a different active env',
    errors:
      swappedCode === EXIT.RECOVERY_REQUIRED
        ? []
        : [`exit ${swappedCode}, expected ${EXIT.RECOVERY_REQUIRED}`],
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
  return results
}
