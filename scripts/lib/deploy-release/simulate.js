// Fake world + executor for `self-test` and the unit tests. Models what matters for safety:
// databases by name -> OID, connection flags, schema versions, volumes, web root and services.
import { Crash, EXIT, deploy, recover } from './engine.js'
import { emptyState } from './journal.js'

const DBS = [
  { key: 'api', role: 'api', name: 'forge' },
  { key: 'orchestrator', role: 'orchestrator', name: 'forge_orch' },
  { key: 'temporal', role: 'temporal', name: 'temporal' },
  { key: 'temporal_visibility', role: 'temporal', name: 'temporal_visibility' },
]

export function createWorld() {
  const databases = {}
  DBS.forEach((d, i) => {
    databases[d.name] = { oid: 100 + i, allowConnections: true, schema: 'prev' }
  })
  const state = { ...emptyState(), current: 'prev', records: { prev: { rev: 'prev' } } }
  return {
    databases,
    originals: Object.fromEntries(DBS.map((d) => [d.name, databases[d.name].oid])),
    snapshots: {},
    nextOid: 200,
    running: 'prev',
    volumes: { 'forge_nats-data': 'prev', 'forge_agentforge-uploads': 'prev' },
    tars: {},
    webroot: 'prev',
    migrationContainer: false,
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
  now: 't0',
  ...overrides,
})

function goodPreflight(world) {
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
    Object.assign(databases[name], { subscriptions: 0, publications: 0 })
  }
  return {
    envVersion: 'next',
    rendered: { services: {} },
    images: {},
    bindProbes: [],
    freeBytes: { private: 2 ** 40 },
    frontendIndexSha: 'sha-next',
    ledgers: {},
    containers: [],
    databases,
    agentContainers: [],
    volumes: Object.keys(world.volumes),
    record: { rev: 'next' },
  }
}

// failAt: op indices that throw before acting. crash: { at, when: 'before' | 'after' } models SIGKILL.
export function fakeExecutor(
  world,
  { failAt = new Set(), crash = null, beforeRollback = null } = {}
) {
  let calls = 0
  const dbs = world.databases
  const need = (name) => {
    if (!dbs[name]) {
      throw new Error(`database ${name} does not exist`)
    }
    return dbs[name]
  }
  const sources = () => DBS.map((d) => d.name)
  const ops = {
    stop_project: () => {
      world.running = null
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
    remove_container: () => {
      world.migrationContainer = false
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
      world.stateJson = JSON.stringify(state)
    },
    preflight: () => goodPreflight(world),
    observe: () => ({
      databases: Object.fromEntries(
        Object.entries(dbs).map(([n, d]) => [
          n,
          { oid: d.oid, allowConnections: d.allowConnections },
        ])
      ),
      migrationContainer: world.migrationContainer,
    }),
    run: (step) => {
      const op = ops[step.op]
      if (!op) {
        return {}
      }
      const k = calls
      calls += 1
      if (step.phase === 'rolling_back' && beforeRollback) {
        beforeRollback(world)
        beforeRollback = null
      }
      if (failAt.has(k)) {
        throw new Error(`injected failure at ${step.op}`)
      }
      if (crash?.at === k && crash.when === 'before') {
        world.migrationContainer = step.op === 'migrate'
        throw new Crash(`crash before ${step.op}`)
      }
      const result = op(step) ?? {}
      if (crash?.at === k) {
        throw new Crash(`crash after ${step.op}`)
      }
      return result
    },
    calls: () => calls,
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

function outcomeErrors(world, rev) {
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
  return errors
}

const ctxFor = (world, opts, config = fakeConfig()) => ({
  config,
  exec: fakeExecutor(world, opts),
  log: () => null,
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

export async function faultMatrix() {
  const probe = createWorld()
  const probeCtx = ctxFor(probe, {})
  await deploy(probeCtx)
  const total = probeCtx.exec.calls()
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
    add(
      `fail at op ${k}`,
      failed,
      await deploy(ctxFor(failed, { failAt: new Set([k]) })),
      committed ? EXIT.OK : EXIT.ROLLED_BACK,
      rev
    )
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
  return results
}
