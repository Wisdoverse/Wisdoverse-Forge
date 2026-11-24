import { createHash } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  agentContainerFindings,
  backendFindings,
  captureExpectations,
  compareBaseline,
  IDENTITY_KEYS,
  compareLedger,
  databaseFindings,
  fingerprint,
  identityOf,
  frontendFindings,
  identityFindings,
  isProductAgent,
  ledgerFindings,
  newAgents,
  parseEnvFile,
  parseMigrations,
  preflightFindings,
  quoteIdent,
  quoteLiteral,
  redact,
  rollbackFindings,
  secretValues,
  tagDriftFindings,
} from '../../../scripts/lib/deploy-release/checks.js'
import {
  atomicWrite,
  createExecutor,
  ensureDir,
} from '../../../scripts/lib/deploy-release/effects.js'
import { EXIT, deploy, recover } from '../../../scripts/lib/deploy-release/engine.js'
import {
  PHASES,
  StateUnreadable,
  announce,
  complete,
  activeInputs,
  emptyState,
  markRecoveryRequired,
  releaseId,
  summarize,
  transition,
  validateState,
  withPointers,
} from '../../../scripts/lib/deploy-release/journal.js'
import {
  RecoveryError,
  failedName,
  planDeploy,
  planFrontendSwitch,
  planRecovery,
  selectPrunable,
  snapshotName,
} from '../../../scripts/lib/deploy-release/plan.js'
import {
  createWorld,
  ctxFor,
  NEXT,
  fakeConfig,
  fakeExecutor,
  fakeRecord,
  faultMatrix,
  goodPreflight,
  invariantErrors,
} from '../../support/deploy-release/simulate.js'

const root = path.resolve(import.meta.dirname, '../../..')
const ops = (steps: { op: string }[]) => steps.map((s) => s.op)
const tmpDir = () => fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-test-'))

function attemptFixture(complete: boolean, phase = 'rolling_back') {
  return {
    attemptId: 'a1b2c3d4',
    target: 'next',
    previous: 'prev',
    phase,
    step: null,
    helperPrefix: 'forge-deploy-a1b2c3d4-',
    inputs: {
      generation: 'a1b2c3d4',
      envSha256: 'b'.repeat(64),
      identity: 'c'.repeat(64),
      runner: 'abcdef1',
      runnerImage: 'sha256:' + '1'.repeat(64),
    },
    startedAt: '2026-11-24T00:00:00.000Z',
    swaps: [],
    snapshot: {
      complete,
      databases: [
        {
          key: 'api',
          source: 'forge',
          sourceOid: 1,
          snapshot: 'forge__snap_a1b2c3d4',
          snapshotOid: complete ? 2 : null,
          failed: 'forge__failed_a1b2c3d4',
        },
      ],
      volumes: [{ volume: 'v', tar: '/s/v.tar', sha256: 'x', restored: false }],
      webroot: { tar: '/s/webroot.tar', sha256: 'y', restored: false },
      expect: {
        endpoints: {},
        streams: [],
        consumers: [],
        ledgers: {
          api: [{ version: 1, checksum: 'aa' }],
          orchestrator: [{ version: 1, checksum: 'aa' }],
        },
      },
    },
  }
}

const db = (oid: number, allowConnections = true) => ({ oid, allowConnections })
const PREV_INPUTS = {
  generation: 'prevgen1',
  envSha256: 'a'.repeat(64),
  identity: 'c'.repeat(64),
  runner: 'abcdef0',
  runnerImage: 'sha256:' + '1'.repeat(64),
}
const observeWith = (world: ReturnType<typeof createWorld>) => {
  const exec = fakeExecutor(world)
  exec.startSql()
  const observed = exec.observe()
  exec.stopSql()
  return observed
}

describe('journal', () => {
  it('allows the forward chain and the rollback branch, rejects skips', () => {
    const forward = [
      'quiescing',
      'snapshotting',
      'snapshotted',
      'migrating',
      'starting',
      'verifying_backend',
      'switching_frontend',
      'verifying_frontend',
      'verified',
      'done',
    ]
    forward.slice(1).forEach((to, i) => expect(transition(forward[i], to)).toBe(to))
    forward
      .slice(0, -2)
      .forEach((from) => expect(transition(from, 'rolling_back')).toBe('rolling_back'))
    expect(() => transition('quiescing', 'verified')).toThrow(/illegal/)
    expect(() => transition('verified', 'rolling_back')).toThrow(/illegal/)
    expect(() => transition('done', 'quiescing')).toThrow(/illegal/)
    expect(() => transition('rolled_back', 'rolling_back')).toThrow(/illegal/)
    expect(transition('recovery_required', 'rolling_back')).toBe('rolling_back')
  })

  it('announces a step before it runs and records results after', () => {
    const state = { ...emptyState(), current: 'prev', attempt: attemptFixture(false, 'quiescing') }
    const step = { op: 'create_snapshot', phase: 'snapshotting', db: 'api' }
    const announced = announce(state, step, 't1')
    expect(announced.attempt.phase).toBe('snapshotting')
    expect(announced.attempt.step).toEqual(step)
    expect(state.attempt.step).toBeNull()
    const done = complete(announced, step, { oid: 42 }, 't2')
    expect(done.attempt.step).toBeNull()
    expect(done.attempt.snapshot.databases[0].snapshotOid).toBe(42)
  })

  it('finding 1: the single write that enters `verified` also moves the release pointers', () => {
    const state = {
      ...emptyState(),
      current: 'baseline',
      records: { baseline: {}, old: {}, next: {} },
      attempt: { ...attemptFixture(true, 'verifying_frontend'), previous: 'baseline' },
    }
    const announced = announce(state, { op: 'commit', phase: 'verified' }, 't')
    expect(announced.attempt.phase).toBe('verified')
    expect(announced.current).toBe('next')
    expect(announced.previous).toBe('baseline')
    expect(announced.protected).toEqual({ baseline: 'a1b2c3d4', lastGood: 'a1b2c3d4' })
    expect(Object.keys(announced.records).sort()).toEqual(['baseline', 'next'])
    const prune = announce(announced, { op: 'prune', phase: 'verified' }, 't')
    expect(prune.protected).toEqual(announced.protected)
  })

  it('marks recovery_required from any in-flight phase', () => {
    for (const phase of ['migrating', 'rolling_back', 'recovery_required']) {
      const state = { ...emptyState(), attempt: attemptFixture(true, phase) }
      expect(markRecoveryRequired(state, 'boom', 't').attempt.phase).toBe('recovery_required')
    }
  })

  it('status names what the recovery section asks the operator to look at', () => {
    const text = summarize({
      ...emptyState(),
      current: 'prev',
      attempt: attemptFixture(true, 'recovery_required'),
    }).join('\n')
    for (const want of [
      'a1b2c3d4',
      'recovery_required',
      'forge__snap_a1b2c3d4',
      'forge__failed_a1b2c3d4',
      '/s/v.tar',
      '/s/webroot.tar',
      'forge-deploy-a1b2c3d4-',
    ]) {
      expect(text).toContain(want)
    }
  })
})

describe('planRecovery', () => {
  const sourceOnly = { databases: { forge: db(1) } }
  const swapped = { databases: { forge: db(2), forge__failed_a1b2c3d4: db(1, false) } }

  it.each(PHASES)('plans a defined outcome for phase %s', (phase) => {
    for (const sealed of [false, true]) {
      const steps = planRecovery(attemptFixture(sealed, phase), sealed ? swapped : sourceOnly)
      if (phase === 'done' || phase === 'rolled_back') {
        expect(steps).toEqual([])
      } else if (phase === 'verified') {
        // Committed: never a rollback; confirm the committed release runs, then close.
        expect(ops(steps)).toEqual(['start', 'verify_backend', 'close'])
        expect(steps[0]).toMatchObject({ rev: 'next', phase: 'verified' })
      } else {
        expect(steps.at(-1)).toEqual({ op: 'close', phase: 'rolled_back' })
        expect(steps.find((s) => s.op === 'verify_backend')).toMatchObject({ against: 'snapshot' })
        expect(ops(steps).includes('restore_volume')).toBe(sealed)
        expect(ops(steps).includes('remove_new_agents')).toBe(sealed)
      }
    }
  })

  it.each([
    ['before disable', { forge: db(1) }, ['stop_project', 'start']],
    ['after disable', { forge: db(1, false) }, ['stop_project', 'enable_connections', 'start']],
    ['after terminate', { forge: db(1, false) }, ['stop_project', 'enable_connections', 'start']],
    [
      'after create',
      { forge: db(1, false), forge__snap_a1b2c3d4: db(2) },
      ['stop_project', 'drop_database', 'enable_connections', 'start'],
    ],
    [
      'after enable',
      { forge: db(1), forge__snap_a1b2c3d4: db(2) },
      ['stop_project', 'drop_database', 'start'],
    ],
  ])(
    'partial snapshot, interrupted %s: drops copies, never renames',
    (_label, databases, expected) => {
      const steps = planRecovery(attemptFixture(false), { databases })
      expect(ops(steps).filter((op) => !op.startsWith('verify') && op !== 'close')).toEqual(
        expected
      )
      expect(ops(steps)).not.toContain('rename')
      expect(steps.filter((s) => s.op === 'drop_database').every((s) => s.name !== 'forge')).toBe(
        true
      )
    }
  )

  it('never drops a database whose OID is the original', () => {
    const steps = planRecovery(attemptFixture(false), {
      databases: { forge: db(1), forge__snap_a1b2c3d4: db(1) },
    })
    expect(ops(steps)).not.toContain('drop_database')
  })

  const swapOps = (databases: Record<string, { oid: number; allowConnections: boolean }>) =>
    planRecovery(attemptFixture(true), { databases })
      .filter((s) => 'db' in s)
      .map((s) => `${s.op}${'from' in s ? ` ${s.from}->${s.to}` : ''}`)
  const fullSwap = [
    'disable_connections',
    'terminate_sessions',
    'rename forge->forge__failed_a1b2c3d4',
    'rename forge__snap_a1b2c3d4->forge',
    'enable_connections',
  ]

  it.each([
    ['before the swap', { forge: db(1), forge__snap_a1b2c3d4: db(2) }, fullSwap],
    ['after disable/terminate', { forge: db(1, false), forge__snap_a1b2c3d4: db(2) }, fullSwap],
    [
      'between the two renames',
      { forge__failed_a1b2c3d4: db(1, false), forge__snap_a1b2c3d4: db(2) },
      ['rename forge__snap_a1b2c3d4->forge', 'enable_connections'],
    ],
    [
      'after both renames',
      { forge__failed_a1b2c3d4: db(1, false), forge: db(2, false) },
      ['enable_connections'],
    ],
    ['after enable', { forge__failed_a1b2c3d4: db(1, false), forge: db(2) }, []],
  ])('rename swap interrupted %s is resumed by OID', (_label, databases, expected) => {
    expect(swapOps(databases)).toEqual(expected)
  })

  it('refuses to guess when the snapshot is gone', () => {
    expect(() => swapOps({ forge: db(1) })).toThrow(RecoveryError)
    expect(() => swapOps({ forge: db(3), forge__snap_a1b2c3d4: db(2) })).toThrow(RecoveryError)
  })

  it('skips completed restores', () => {
    const attempt = attemptFixture(true)
    attempt.snapshot.volumes[0].restored = true
    attempt.snapshot.webroot.restored = true
    const steps = planRecovery(attempt, swapped)
    expect(ops(steps)).not.toContain('restore_volume')
    expect(ops(steps)).not.toContain('restore_webroot')
  })

  it('is idempotent: re-planning after an executed recovery yields no database or restore steps', async () => {
    const world = createWorld()
    // op 29 = verify_frontend: stop, agent check, 4 x 5 snapshot ops, 3 archives, migrate, start, verify, switch
    expect(await deploy(ctxFor(world, { failAt: new Set([29]) }))).toBe(EXIT.ROLLED_BACK)
    const attempt = JSON.parse(world.stateJson).attempt
    expect(planRecovery(attempt, observeWith(world))).toEqual([])
    const again = ops(planRecovery({ ...attempt, phase: 'rolling_back' }, observeWith(world)))
    expect(again).not.toContain('rename')
    expect(again).not.toContain('restore_volume')
    expect(again).not.toContain('drop_database')
  })
})

describe('engine', () => {
  const state = { ...emptyState(), current: 'prev', records: { prev: {} } }

  it('is a no-op when the target is current and refuses without a baseline', async () => {
    const noop = createWorld()
    // Round 5, F4a: a no-op needs the same revision AND the same env content.
    const sameEnv = {
      envSha256: PREV_INPUTS.envSha256,
      identity: PREV_INPUTS.identity,
      runner: 'abcdef1',
      runnerImage: 'sha256:' + '1'.repeat(64),
    }
    expect(await deploy(ctxFor(noop, { staged: sameEnv }, fakeConfig({ target: 'prev' })))).toBe(
      EXIT.OK
    )
    expect(JSON.parse(noop.stateJson).attempt).toBeNull()
    const bare = createWorld()
    bare.stateJson = JSON.stringify(emptyState())
    expect(await deploy(ctxFor(bare, {}))).toBe(EXIT.PREFLIGHT_REFUSED)
  })

  it('round 3 finding 1: the production wiring (no injected clock) journals a real start time', async () => {
    const world = createWorld()
    const before = Date.now()
    // Exactly what scripts/deploy-release.mjs builds: { config, exec, log } and no clock.
    const ctx = {
      config: fakeConfig({ simulateVerifyFailure: true }),
      exec: fakeExecutor(world),
      log: () => null,
    }
    await deploy(ctx)
    const startedAt = Date.parse(JSON.parse(world.stateJson).attempt.startedAt)
    expect(startedAt).toBeGreaterThanOrEqual(before - 1000)
    expect(startedAt).toBeLessThanOrEqual(Date.now())
    const cli = fs.readFileSync(path.join(root, 'scripts/deploy-release.mjs'), 'utf8')
    expect(cli).not.toMatch(/\bnow\s*:/)
  })

  it('round 3 finding 1/7: a journal without a usable start time (or with a bare schema) is refused', async () => {
    const messages: string[] = []
    for (const bad of [
      { schema: 1 },
      {
        ...emptyState(),
        current: 'prev',
        records: { prev: fakeRecord('prev', PREV_INPUTS) },
        attempt: { ...attemptFixture(true), startedAt: 'yesterday' },
      },
      {
        ...emptyState(),
        current: 'prev',
        records: { prev: fakeRecord('prev', PREV_INPUTS) },
        attempt: { ...attemptFixture(true), inputs: null },
      },
      { ...emptyState(), current: 'gone', records: {} },
    ]) {
      const world = createWorld()
      world.stateJson = JSON.stringify(bad)
      const log = (m: string) => messages.push(m)
      expect(await recover({ ...ctxFor(world, {}), log })).toBe(EXIT.RECOVERY_REQUIRED)
      expect(await deploy({ ...ctxFor(world, {}), log })).toBe(EXIT.RECOVERY_REQUIRED)
    }
    expect(messages.join('\n')).toMatch(/structurally invalid/)
  })

  it('round 3 finding 2: a write failure after the commit leaves the release running and exits 14', async () => {
    const probe = createWorld()
    const probeCtx = ctxFor(probe, {})
    await deploy(probeCtx)
    const world = createWorld()
    // The two writes before `close` complete: the commit is on disk, finishing fails.
    expect(await deploy(ctxFor(world, { failWritesFrom: probeCtx.exec.writes() - 2 }))).toBe(
      EXIT.FINALIZE_FAILED
    )
    expect(world.running).toBe(NEXT)
    expect(JSON.parse(world.stateJson).attempt.phase).toBe('verified')
    world.running = null
    expect(await recover(ctxFor(world, {}))).toBe(EXIT.OK)
    expect(world.running).toBe(NEXT)
    expect(JSON.parse(world.stateJson).attempt.phase).toBe('done')
  })

  it('finding 2: an unreadable state file is a recovery-required refusal, never "no journal"', async () => {
    const messages: string[] = []
    const exec = {
      ...fakeExecutor(createWorld()),
      readState: () => {
        throw new StateUnreadable('state.json is not valid JSON')
      },
    }
    for (const run of [deploy, recover]) {
      expect(await run({ config: fakeConfig(), exec, log: (m: string) => messages.push(m) })).toBe(
        EXIT.RECOVERY_REQUIRED
      )
    }
    expect(messages.join('\n')).toMatch(/cannot be read.*\n.*Do not deploy/)
  })

  it('finding 4: removes leftover helpers before planning a recovery', async () => {
    const world = createWorld()
    const calls: string[] = []
    const base = fakeExecutor(world, { crash: { at: 25, when: 'before' } })
    await expect(deploy({ config: fakeConfig(), exec: base, log: () => null })).rejects.toThrow(
      /crash/
    )
    const exec = fakeExecutor(world)
    const spied = {
      ...exec,
      removeHelpers: () => {
        calls.push('removeHelpers')
        exec.removeHelpers()
      },
      observe: () => {
        calls.push('observe')
        return exec.observe()
      },
    }
    expect(await recover({ config: fakeConfig(), exec: spied, log: () => null })).toBe(EXIT.OK)
    expect(calls.slice(0, 2)).toEqual(['removeHelpers', 'observe'])
    const failing = {
      ...exec,
      removeHelpers: () => Promise.reject(new Error('helper containers still present')),
    }
    const stuck = createWorld()
    await expect(deploy(ctxFor(stuck, { crash: { at: 25, when: 'before' } }))).rejects.toThrow(
      /crash/
    )
    expect(
      await recover({
        config: fakeConfig(),
        exec: { ...fakeExecutor(stuck), removeHelpers: failing.removeHelpers },
        log: () => null,
      })
    ).toBe(EXIT.RECOVERY_REQUIRED)
  })

  it('SQL helper: a leftover is removed at recover start, then a single helper serves the run', async () => {
    const world = createWorld()
    await expect(deploy(ctxFor(world, { crash: { at: 12, when: 'after' } }))).rejects.toThrow(
      /crash/
    )
    expect(world.sqlHelper).toBe('running')
    expect(await recover(ctxFor(world, {}))).toBe(EXIT.OK)
    expect(world.sqlHelper).toBeNull()
    expect(world.sqlStarts).toBe(2)
  })

  it('SQL helper missing mid-run: the step fails cleanly, the journal stays consistent', async () => {
    const world = createWorld()
    // op 4 = create_snapshot of the first database
    expect(await deploy(ctxFor(world, { helperDiesAt: 4 }))).toBe(EXIT.ROLLED_BACK)
    const state = JSON.parse(world.stateJson)
    expect(state.attempt.phase).toBe('rolled_back')
    expect(state.attempt.error).toBeNull()
    expect(invariantErrors(world)).toEqual([])
    expect(world.sqlHelper).toBeNull()
    expect(world.sqlStarts).toBe(2)
  })

  it('SQL helper that cannot be removed is reported, not ignored', async () => {
    const messages: string[] = []
    const world = createWorld()
    const ctx = { ...ctxFor(world, { stopFails: true }), log: (m: string) => messages.push(m) }
    expect(await deploy(ctx)).toBe(EXIT.FINALIZE_FAILED)
    expect(JSON.parse(world.stateJson).attempt.phase).toBe('done')
    expect(messages.join('\n')).toMatch(/SQL helper container could not be removed/)
  })

  it('journals snapshot names with the attempt id, checks agents after stopping, snapshots before migrating', () => {
    const plan = planDeploy(fakeConfig(), state, goodPreflight(createWorld()), {
      at: '2026-11-24T00:00:00.000Z',
    })
    expect(plan.attempt.snapshot.databases.map((d: { snapshot: string }) => d.snapshot)).toContain(
      'forge__snap_a1b2c3d4'
    )
    expect(plan.attempt.helperPrefix).toBe('forge-deploy-a1b2c3d4-')
    const order = ops(plan.steps)
    expect(order.slice(0, 2)).toEqual(['stop_project', 'check_no_agents'])
    expect(order.lastIndexOf('create_snapshot')).toBeLessThan(order.indexOf('seal'))
    expect(order.indexOf('seal')).toBeLessThan(order.indexOf('migrate'))
    expect(order.indexOf('verify_backend')).toBeLessThan(order.indexOf('switch_frontend'))
    expect(order.indexOf('verify_frontend')).toBeLessThan(order.indexOf('commit'))
  })

  it('names never collide across attempts and respect the 63-byte limit', () => {
    expect(snapshotName('forge', 'a1b2c3d4')).not.toBe(snapshotName('forge', 'a1b2c3d5'))
    expect(failedName('forge', 'a1b2c3d4')).toBe('forge__failed_a1b2c3d4')
    expect(failedName('f'.repeat(36), '20261124000000abcd')).toHaveLength(63)
    expect(() => failedName('f'.repeat(37), '20261124000000abcd')).toThrow(/63/)
    expect(() => snapshotName('bad"name', 'a1b2c3d4')).toThrow(/invalid/)
  })
})

describe('migration ledger', () => {
  const release = [
    { version: 1, checksum: 'aa' },
    { version: 2, checksum: 'bb' },
  ]
  const row = (version: number, checksum: string, success = true) => ({
    version,
    checksum,
    success,
  })

  it('accepts identical ledgers and reports appended versions as pending', () => {
    const same = compareLedger([row(1, 'AA'), row(2, 'bb')], release)
    expect(ledgerFindings('api', same, { strict: true })).toEqual([])
    const appended = compareLedger([row(1, 'aa')], release)
    expect(appended.pending).toEqual([2])
    expect(ledgerFindings('api', appended, { strict: false })).toEqual([])
    expect(ledgerFindings('api', appended, { strict: true })).toHaveLength(1)
  })

  it('refuses checksum mismatches, failed rows and applied versions missing from the release', () => {
    const cmp = compareLedger([row(1, 'zz'), row(2, 'bb', false), row(3, 'cc')], release)
    expect(cmp.checksumMismatch).toEqual([1])
    expect(cmp.failed).toEqual([2])
    expect(cmp.missingInRelease).toEqual([3])
    expect(ledgerFindings('api', cmp, { strict: false })).toHaveLength(3)
  })

  it('parses versions and SHA-384 checksums the way sqlx-core 0.8.6 does', () => {
    const dir = path.join(root, 'rust/crates/db/migrations')
    const files = fs
      .readdirSync(dir)
      .map((name) => ({ name, content: fs.readFileSync(path.join(dir, name)) }))
    const parsed = parseMigrations(files)
    expect(parsed[0].version).toBe(0)
    expect(parsed.every((m, i) => i === 0 || m.version > parsed[i - 1].version)).toBe(true)
    const first = fs.readFileSync(path.join(dir, '000_legacy_prepare.sql'))
    expect(parsed[0].checksum).toBe(createHash('sha384').update(first).digest('hex'))
    expect(parseMigrations([{ name: 'MANIFEST.sha256', content: Buffer.from('') }])).toEqual([])
    expect(parseMigrations([{ name: '5_x.down.sql', content: Buffer.from('') }])).toEqual([])
    expect(() => parseMigrations([{ name: 'v5_x.sql', content: Buffer.from('') }])).toThrow(
      /integer/
    )
  })
})

describe('retention (finding 10)', () => {
  const state = {
    ...emptyState(),
    current: 'c0ffee1',
    previous: 'beef001',
    protected: { baseline: 'base0001', lastGood: 'good0001' },
  }
  const items = [
    { kind: 'database', name: 'forge__snap_base0001' },
    { kind: 'database', name: 'forge__snap_good0001' },
    { kind: 'database', name: 'forge__failed_live0001' },
    { kind: 'database', name: 'forge__snap_old00001' },
    { kind: 'database', name: 'forge__failed_old00001' },
    { kind: 'database', name: 'forge' },
    { kind: 'database', name: 'forgex__snap_old00001' },
    { kind: 'database', name: 'xforge__snap_old00001' },
    { kind: 'database', name: 'forge__snap_old00001_copy' },
    { kind: 'database', name: 'forge__snapshot_old00001' },
    { kind: 'database', name: 'other__failed_old00001' },
    { kind: 'database', name: 'forge__snap_old"0001' },
    { kind: 'database', name: 'forge__snap_OLD00001' },
    { kind: 'snapshotDir', name: 'base0001' },
    { kind: 'snapshotDir', name: 'old00001' },
    { kind: 'releaseDir', name: 'c0ffee1' },
    { kind: 'releaseDir', name: 'beef001' },
    { kind: 'releaseDir', name: 'baseline' },
    { kind: 'releaseDir', name: 'dead001' },
  ]

  it('selects only generated names of configured sources, never protected or foreign ones', () => {
    const names = selectPrunable(items, state, 'live0001', ['forge', 'forge_orch']).map(
      (i: { name: string }) => i.name
    )
    expect(names.sort()).toEqual([
      'dead001',
      'forge__failed_old00001',
      'forge__snap_old00001',
      'old00001',
    ])
    expect(
      selectPrunable(items, state, 'live0001', []).filter(
        (i: { kind: string }) => i.kind === 'database'
      )
    ).toEqual([])
  })

  it('quotes identifiers and literals and rejects NUL', () => {
    expect(quoteIdent('a"b')).toBe('"a""b"')
    expect(quoteLiteral("o'brien")).toBe("'o''brien'")
    expect(() => quoteIdent('a\0b')).toThrow(/NUL/)
    expect(() => quoteLiteral('a\0b')).toThrow(/NUL/)
  })
})

describe('frontend switch plan', () => {
  it('copies changed assets first and index.html last, skipping unchanged files', () => {
    const plan = planFrontendSwitch(
      [
        { path: 'index.html', sha256: 'new' },
        { path: 'assets/b.js', sha256: 'b2' },
        { path: 'assets/a.js', sha256: 'a1' },
        { path: 'favicon.ico', sha256: 'f' },
      ],
      new Map([
        ['index.html', 'old'],
        ['assets/a.js', 'a1'],
        ['favicon.ico', 'f0'],
      ])
    )
    expect(plan).toEqual(['assets/b.js', 'favicon.ico', 'index.html'])
    expect(() => planFrontendSwitch([{ path: 'a.js', sha256: 'x' }], new Map())).toThrow(
      /index.html/
    )
  })
})

describe('preflight predicates', () => {
  const good = {
    oid: 1,
    allowConnections: true,
    ownedByRole: true,
    ownerCreatedb: true,
    defaultAcl: true,
    defaultTablespace: true,
    connLimit: -1,
    settings: 0,
    slots: 0,
    subscriptions: 0,
    publications: 0,
  }

  it.each([
    ['ownedByRole', false],
    ['ownerCreatedb', false],
    ['defaultAcl', false],
    ['defaultTablespace', false],
    ['settings', 1],
    ['slots', 1],
    ['subscriptions', 1],
    ['publications', 1],
    ['allowConnections', false],
    ['connLimit', 5],
  ])('A5 refuses when %s is %s', (key, value) => {
    expect(databaseFindings([{ name: 'forge' }], { forge: good })).toEqual([])
    expect(
      databaseFindings([{ name: 'forge' }], { forge: { ...good, [key]: value } })
    ).toHaveLength(1)
  })

  const uuid = '0a1b2c3d-0000-4000-8000-000000000001'
  it('finding 8: selects product agent containers strictly by name pattern and matching label', () => {
    const agent = {
      name: `agentforge-agent-${uuid}`,
      labels: { 'agentforge.agent_id': uuid },
      created: '2026-11-24T01:00:00Z',
    }
    const devenv = {
      name: `agentforge-devenv-${uuid}`,
      labels: { 'agentforge.dev_environment_id': uuid },
    }
    const impostors = [
      { name: `agentforge-agent-${uuid}`, labels: {} },
      { name: 'agentforge-agent-x', labels: { 'agentforge.agent_id': 'x' } },
      { name: `my-agentforge-agent-${uuid}`, labels: { 'agentforge.agent_id': uuid } },
      {
        name: `agentforge-agent-${uuid}`,
        labels: { 'agentforge.agent_id': '0a1b2c3d-0000-4000-8000-000000000009' },
      },
      { name: 'agentforge-nats', labels: { 'com.docker.compose.service': 'nats' } },
    ]
    expect([agent, devenv].every(isProductAgent)).toBe(true)
    expect(impostors.some(isProductAgent)).toBe(false)
    expect(agentContainerFindings([agent, ...impostors])).toHaveLength(1)
    expect(newAgents([agent], '2026-11-24T00:00:00Z')).toEqual([agent])
    expect(newAgents([agent], '2026-11-24T02:00:00Z')).toEqual([])
  })

  it('checks project identity and mounted volumes', () => {
    const rendered = {
      services: {
        nats: {
          container_name: 'p-nats',
          volumes: [{ type: 'volume', source: 'nats-data', target: '/data' }],
        },
      },
      volumes: { 'nats-data': { name: 'p_nats-data' } },
    }
    const container = {
      name: 'p-nats',
      project: 'p',
      mounts: [{ target: '/data', name: 'p_nats-data' }],
    }
    expect(identityFindings('p', rendered, [container])).toEqual([])
    expect(identityFindings('other', rendered, [container])).toHaveLength(1)
    const wrongVolume = { ...container, mounts: [{ target: '/data', name: 'q_nats-data' }] }
    expect(identityFindings('p', rendered, [wrongVolume])).toHaveLength(1)
    expect(identityFindings('p', rendered, [])).toHaveLength(1)
  })

  it('finding 9: refuses when the rollback target is not complete', () => {
    const ok = {
      configReadable: true,
      releaseDirExists: true,
      bindProbes: [{ source: '/a', ok: true }],
      missingImages: [],
    }
    expect(rollbackFindings(ok)).toEqual([])
    expect(rollbackFindings({ ...ok, configReadable: false })).toHaveLength(1)
    expect(rollbackFindings({ ...ok, releaseDirExists: false })).toHaveLength(1)
    expect(rollbackFindings({ ...ok, bindProbes: [{ source: '/a', ok: false }] })).toHaveLength(1)
    expect(rollbackFindings({ ...ok, missingImages: ['sha256:1'] })).toHaveLength(1)
  })

  it('finding 6: a moved tag of a recorded release is reported', () => {
    const records = {
      prev: { rev: 'prev', refs: { nats: 'nats:2' }, images: { nats: 'sha256:1' } },
    }
    expect(tagDriftFindings(records, { 'nats:2': 'sha256:1' })).toEqual([])
    expect(tagDriftFindings(records, { 'nats:2': 'sha256:2' })).toHaveLength(1)
    expect(tagDriftFindings(records, {})).toHaveLength(1)
  })

  it('aggregates findings, including low disk, missing images, rollback and drift', () => {
    const observed = goodPreflight(createWorld())
    const config = fakeConfig()
    const state = { ...emptyState(), current: 'prev', records: { prev: {} } }
    expect(preflightFindings(config, state, observed)).toEqual([])
    expect(
      preflightFindings(config, state, { ...observed, freeBytes: { private: 10 } })
    ).toHaveLength(1)
    expect(preflightFindings(config, state, { ...observed, images: { nats: null } })).toHaveLength(
      1
    )
    const noRollback = { ...observed, rollback: { ...observed.rollback, configReadable: false } }
    expect(preflightFindings(config, state, noRollback)).toHaveLength(1)
    const drifted = {
      ...emptyState(),
      records: { prev: { rev: 'prev', refs: { a: 'a:1' }, images: { a: 'x' } } },
    }
    expect(preflightFindings(config, drifted, observed)).toHaveLength(1)
  })
})

describe('verification predicates', () => {
  const ok = (body: unknown) => ({ status: 200, body })
  const contract = JSON.parse(
    fs.readFileSync(path.join(root, 'scripts/deploy-release.jetstream.json'), 'utf8')
  )
  const jsz = ok({
    account_details: [
      {
        stream_detail: contract.streams.map((name: string) => ({
          name,
          consumer_detail:
            name === contract.resultConsumer.stream ? [{ name: contract.resultConsumer.name }] : [],
        })),
      },
    ],
  })
  const rows = [{ version: 1, checksum: 'aa', success: true }]
  const facts = {
    apiHealth: ok({ ok: true }),
    apiReady: ok({
      ok: true,
      status: 'ready',
      checks: { database: true, nats: true, docker: true, redis: false },
    }),
    orchHealth: ok({ workflowRuntime: 'up' }),
    orchReady: ok({ ok: true }),
    natsHealth: ok({ status: 'ok' }),
    jsz,
    temporalServing: true,
    runningImages: { nats: 'sha256:1' },
    ledgers: { api: rows },
  }
  const forward = {
    endpoints: null,
    streams: contract.streams,
    consumers: [contract.resultConsumer],
    ledgers: { api: [{ version: 1, checksum: 'aa' }] },
    images: { nats: 'sha256:1' },
    requireRedis: false,
  }

  it('forward: passes only when every check passes; a check that cannot run is a failure', () => {
    expect(backendFindings(facts, forward)).toEqual([])
    expect(backendFindings(facts, { ...forward, requireRedis: true })).toHaveLength(1)
    expect(
      backendFindings({ ...facts, orchHealth: ok({ workflowRuntime: 'disabled' }) }, forward)
    ).toHaveLength(1)
    expect(backendFindings({ ...facts, natsHealth: { error: 'timeout' } }, forward)[0]).toMatch(
      /NATS \/healthz: unreachable/
    )
    expect(backendFindings({ ...facts, jsz: ok({ account_details: [] }) }, forward)).toHaveLength(5)
    expect(backendFindings({ ...facts, runningImages: {} }, forward)).toHaveLength(1)
    expect(backendFindings({ ...facts, temporalServing: false }, forward)).toHaveLength(1)
    expect(backendFindings({ ...facts, ledgers: { api: [] } }, forward)).toHaveLength(1)
  })

  it('finding 5: rollback is held to what the previous release answered before the deploy', () => {
    // An older build: no /health/ready (404), no workflowRuntime field, one stream.
    const older = {
      ...facts,
      orchHealth: ok({ status: 'healthy' }),
      orchReady: { status: 404, body: null },
      jsz: ok({ account_details: [{ stream_detail: [{ name: 'EVENTS' }] }] }),
      ledgers: { api: [{ version: 0, checksum: 'zz', success: true }] },
    }
    const { problems, expect: captured } = captureExpectations(older, false)
    expect(problems).toEqual([])
    expect(captured.endpoints['orchestrator /health']).toBe('answers')
    expect(captured.endpoints['orchestrator /health/ready']).toBe('unsupported')
    expect(captured.endpoints['API /health']).toBe('healthy')
    expect(captured.streams).toEqual(['EVENTS'])
    const expectation = { ...captured, images: { nats: 'sha256:1' }, requireRedis: false }
    expect(backendFindings(older, expectation)).toEqual([])
    expect(backendFindings(older, forward).length).toBeGreaterThan(0)
    expect(
      backendFindings({ ...older, orchHealth: { error: 'timeout' } }, expectation)
    ).toHaveLength(1)
    expect(
      backendFindings({ ...older, ledgers: { api: rows } }, expectation).length
    ).toBeGreaterThan(0)
  })

  it('round 3 finding 4: a capture that cannot observe refuses; a 404 or an empty listing is a fact', () => {
    const cases: [string, object, RegExp | null][] = [
      ['timeout', { orchReady: { error: 'timeout' } }, /unreachable/],
      ['HTTP 503', { apiReady: { status: 503, body: null } }, /is error/],
      [
        'JetStream listing failed',
        { jsz: { error: 'connection failed' } },
        /JetStream listing failed/,
      ],
      ['JetStream HTTP 500', { jsz: { status: 500, body: null } }, /JetStream listing failed/],
      ['Temporal not serving', { temporalServing: false }, /Temporal/],
      ['404', { orchReady: { status: 404, body: null } }, null],
      ['empty listing', { jsz: ok({ account_details: [] }) }, null],
    ]
    for (const [, patch, refusal] of cases) {
      const { problems, expect: captured } = captureExpectations({ ...facts, ...patch }, false)
      if (refusal) {
        expect(problems.join('\n')).toMatch(refusal)
      } else {
        expect(problems).toEqual([])
      }
      if (patch === cases[6][1]) {
        expect(captured.streams).toEqual([])
      }
    }
    const observed = {
      ...goodPreflight(createWorld()),
      captureProblems: ['running release: x is unreachable'],
    }
    const state = { ...emptyState(), current: 'prev', records: { prev: {} } }
    expect(preflightFindings(fakeConfig(), state, observed)).toEqual([
      'running release: x is unreachable',
    ])
  })

  it('compares the served and on-disk index.html with the release copy', () => {
    expect(frontendFindings({ diskSha: 'a', served: { sha: 'a' } }, 'a')).toEqual([])
    expect(frontendFindings({ diskSha: 'a', served: null }, 'a')[0]).toMatch(/could not run/)
    expect(frontendFindings({ diskSha: 'b', served: { sha: 'c' } }, 'a')).toHaveLength(2)
  })

  describe('finding 7: baseline equivalence', () => {
    const release = '/srv/r/releases/baseline'
    const rendered = {
      services: {
        nats: {
          container_name: 'p-nats',
          image: 'nats:2',
          environment: { A: '1' },
          volumes: [
            { type: 'volume', source: 'nats-data', target: '/data' },
            {
              type: 'bind',
              source: `${release}/docker/nats.conf`,
              target: '/etc/nats/nats.conf',
              read_only: true,
            },
            {
              type: 'bind',
              source: '/var/run/docker.sock',
              target: '/var/run/docker.sock',
              read_only: true,
            },
          ],
          networks: { default: {} },
          ports: [{ target: 4222, published: '4222', host_ip: '127.0.0.1', protocol: 'tcp' }],
          healthcheck: { test: ['CMD', 'true'] },
        },
      },
      networks: { default: { name: 'p_default' } },
      volumes: { 'nats-data': { name: 'p_nats-data' } },
    }
    const container = {
      name: 'p-nats',
      imageId: 'sha256:1',
      env: ['A=1', 'PATH=/bin'],
      mounts: [
        { type: 'volume', name: 'p_nats-data', target: '/data', rw: true },
        {
          type: 'bind',
          source: '/opt/hand/docker/nats.conf',
          target: '/etc/nats/nats.conf',
          rw: false,
        },
        { type: 'bind', source: '/var/run/docker.sock', target: '/var/run/docker.sock', rw: false },
      ],
      networks: ['p_default'],
      portBindings: ['127.0.0.1:4222->4222/tcp'],
      healthcheck: ['CMD', 'true'],
    }
    const ids = { 'nats:2': 'sha256:1' }
    // Daemon-side `test -f` proved this source is a regular file inside the release directory.
    const files = new Set([`${release}/docker/nats.conf`])
    const withMount = (i: number, patch: object) => ({
      ...container,
      mounts: container.mounts.map((m, j) => (j === i ? { ...m, ...patch } : m)),
    })

    it('accepts the running stack, including the release-relocated config file', () => {
      expect(compareBaseline(rendered, [container], ids, files)).toEqual([])
    })

    it('round 3 finding 7: a release-directory source that is not a proven regular file is not relocated', () => {
      expect(compareBaseline(rendered, [container], ids, new Set())).toHaveLength(1)
    })

    it.each([
      ['another named volume', withMount(0, { name: 'q_nats-data' })],
      ['a writable mount that should be read-only', withMount(2, { rw: true })],
      [
        'a different bind source outside the release directory',
        withMount(2, { source: '/run/other.sock' }),
      ],
      [
        'a relocated file with another name',
        withMount(1, { source: '/opt/hand/docker/other.conf' }),
      ],
      ['another host address', { ...container, portBindings: ['0.0.0.0:4222->4222/tcp'] }],
      ['another host port', { ...container, portBindings: ['127.0.0.1:14222->4222/tcp'] }],
      ['another protocol', { ...container, portBindings: ['127.0.0.1:4222->4222/udp'] }],
      ['another network', { ...container, networks: [] }],
      ['another image', { ...container, imageId: 'sha256:2' }],
    ])('refuses %s', (_label, changed) => {
      expect(compareBaseline(rendered, [changed], ids, files)).toHaveLength(1)
    })
  })
})

describe('env parsing and redaction', () => {
  it('parses the deploy.sh dotenv subset with last-assignment-wins', () => {
    const env = parseEnvFile('# c\nexport A="x y"\nB=\'q\'\nVERSION=1\n\nVERSION=abc1234\n')
    expect(env).toEqual({ A: 'x y', B: 'q', VERSION: 'abc1234' })
    expect(() => parseEnvFile('not a pair')).toThrow(/KEY=VALUE/)
  })

  it('redacts secret values and URL credentials', () => {
    const env = {
      DATABASE_URL: 'postgres://u:p%40ss@db:5432/forge',
      JWT_SECRET: 'jwt-secret-value',
      PORT: '4003',
    }
    const secrets = secretValues(env)
    const text = `url=${env.DATABASE_URL} jwt=jwt-secret-value raw=p@ss port=4003 other=https://a:b@x/`
    const out = redact(text, secrets)
    expect(out).not.toContain('jwt-secret-value')
    expect(out).not.toContain('p@ss')
    expect(out).not.toContain('p%40ss')
    expect(out).not.toContain('a:b@')
    expect(out).toContain('port=4003')
  })
})

describe('JetStream contract', () => {
  const rust = (rel: string) => fs.readFileSync(path.join(root, rel), 'utf8')
  const constants = new Map<string, string>()
  for (const dir of ['rust/bins/server/src', 'rust/crates/jobs/src', 'rust/crates/core/src']) {
    for (const file of fs.readdirSync(path.join(root, dir)).filter((f) => f.endsWith('.rs'))) {
      for (const m of rust(`${dir}/${file}`).matchAll(/pub const (\w+): &str = "([^"]+)";/g)) {
        constants.set(m[1], m[2])
      }
    }
  }
  const contract = JSON.parse(rust('scripts/deploy-release.jetstream.json'))

  it('lists every stream ensured by rust/bins/server/src/streams.rs', () => {
    const ensure = rust('rust/bins/server/src/streams.rs').split('#[cfg(test)]')[0]
    const idents = [...ensure.matchAll(/name: (\w+)\.to_string\(\)/g)].map((m) => m[1])
    expect(idents.length).toBeGreaterThan(0)
    const names = idents.map((id) => constants.get(id))
    expect(names.every(Boolean)).toBe(true)
    expect([...contract.streams].sort()).toEqual([...names].sort())
  })

  it('names the orchestration result consumer the jobs crate binds', () => {
    const source = rust('rust/crates/jobs/src/orchestration_result_consumer.rs')
    expect(source).toMatch(/stream_name: ORCHESTRATION_RESULTS_STREAM\.to_string\(\)/)
    expect(source).toMatch(/durable_name: ORCHESTRATION_RESULTS_DURABLE\.to_string\(\)/)
    expect(contract.resultConsumer).toEqual({
      stream: constants.get('ORCHESTRATION_RESULTS_STREAM'),
      name: constants.get('ORCHESTRATION_RESULTS_DURABLE'),
    })
  })
})

describe('state machine end to end (fake executor)', () => {
  it('every injected failure and crash (op or journal write) ends consistent, nothing lost', async () => {
    const results = await faultMatrix()
    expect(results.length).toBeGreaterThan(200)
    expect(results.filter((r) => r.errors.length > 0)).toEqual([])
  })

  it('rehearsal D: snapshot unavailable during rollback -> recovery_required, deploy refuses, recover after repair', async () => {
    const world = createWorld()
    let hidden: unknown = null
    const hide = (w: typeof world) => {
      hidden = w.databases['forge__snap_a1b2c3d4']
      delete w.databases['forge__snap_a1b2c3d4']
    }
    const failing = ctxFor(
      world,
      { beforeRollback: hide },
      fakeConfig({ simulateVerifyFailure: true })
    )
    expect(await deploy(failing)).toBe(EXIT.RECOVERY_REQUIRED)
    expect(JSON.parse(world.stateJson).attempt.phase).toBe('recovery_required')
    expect(world.running).toBeNull()
    expect(await deploy(ctxFor(world, {}, fakeConfig({ attemptId: 'b1b2c3d4' })))).toBe(
      EXIT.PREFLIGHT_REFUSED
    )
    world.databases['forge__snap_a1b2c3d4'] = hidden as (typeof world.databases)[string]
    expect(await recover(ctxFor(world, {}))).toBe(EXIT.OK)
    const state = JSON.parse(world.stateJson)
    expect(state.attempt.phase).toBe('rolled_back')
    expect(state.current).toBe('prev')
    expect(invariantErrors(world)).toEqual([])
  })
})

describe('filesystem effects (temporary directory only)', () => {
  const write = (file: string, text: string) => {
    fs.mkdirSync(path.dirname(file), { recursive: true })
    fs.writeFileSync(file, text)
  }
  const cfgFor = (tmp: string) => ({
    webRoot: path.join(tmp, 'www'),
    env: {},
    attemptId: 'a1b2c3d4',
    secrets: [],
    databases: [],
    project: 'forge',
    releaseRoot: tmp,
    privateRoot: tmp,
  })

  it('switches the web root with index.html last, archives it and restores it', async () => {
    const tmp = tmpDir()
    const cfg = cfgFor(tmp)
    const next = path.join(tmp, 'frontend/next')
    write(path.join(cfg.webRoot, 'index.html'), 'old index')
    write(path.join(cfg.webRoot, 'assets/old.js'), 'old asset')
    write(path.join(next, 'index.html'), 'new index')
    write(path.join(next, 'assets/new.js'), 'new asset')
    const exec = createExecutor(cfg, () => null)
    const state = { ...emptyState(), records: { next: { frontendDir: next } } }
    const tar = path.join(tmp, 'webroot.tar')
    const { sha256: archived } = await exec.run({ op: 'archive_webroot', tar }, state)
    await exec.run({ op: 'switch_frontend', rev: 'next' }, state)
    expect(fs.readFileSync(path.join(cfg.webRoot, 'index.html'), 'utf8')).toBe('new index')
    expect(fs.readFileSync(path.join(cfg.webRoot, 'assets/new.js'), 'utf8')).toBe('new asset')
    expect(fs.existsSync(path.join(cfg.webRoot, 'assets/old.js'))).toBe(true)
    await exec.run({ op: 'restore_webroot', tar, sha256: archived }, state)
    expect(fs.readFileSync(path.join(cfg.webRoot, 'index.html'), 'utf8')).toBe('old index')
    await expect(exec.run({ op: 'restore_webroot', tar, sha256: 'wrong' }, state)).rejects.toThrow(
      /sha256/
    )
    expect(fs.readdirSync(cfg.webRoot).some((f) => f.includes('.forge-tmp-'))).toBe(false)
    await expect(exec.run({ op: 'no_such_step' }, state)).rejects.toThrow(/unknown step/)
    fs.rmSync(tmp, { recursive: true, force: true })
  })

  it('finding 2: writes every byte atomically with mode 600 and refuses a corrupt state file', () => {
    const tmp = tmpDir()
    const big = JSON.stringify({ schema: 1, pad: 'x'.repeat(8 * 1024 * 1024) })
    atomicWrite(path.join(tmp, 'state.json'), big)
    expect(fs.readFileSync(path.join(tmp, 'state.json'), 'utf8')).toBe(big)
    expect(fs.statSync(path.join(tmp, 'state.json')).mode & 0o777).toBe(0o600)
    expect(fs.existsSync(path.join(tmp, 'state.json.tmp'))).toBe(false)
    const exec = createExecutor(cfgFor(tmp), () => null)
    fs.writeFileSync(path.join(tmp, 'state.json'), '{"schema":1,"attempt":{"pha')
    expect(() => exec.readState()).toThrow(StateUnreadable)
    fs.writeFileSync(path.join(tmp, 'state.json'), '{"schema":7}')
    expect(() => exec.readState()).toThrow(StateUnreadable)
    fs.rmSync(tmp, { recursive: true, force: true })
  })

  it('round 3 finding 3: ensureDir creates nested directories with the given mode', () => {
    const tmp = tmpDir()
    ensureDir(path.join(tmp, 'a/b/c'), 0o700)
    expect(fs.statSync(path.join(tmp, 'a/b/c')).isDirectory()).toBe(true)
    expect(fs.statSync(path.join(tmp, 'a/b/c')).mode & 0o777).toBe(0o700)
    ensureDir(path.join(tmp, 'a/b/c'))
    fs.rmSync(tmp, { recursive: true, force: true })
  })

  it('round 4 C2: one rename turns the staged directory into a generation; an unfinished attempt leaves both alone', async () => {
    const tmp = tmpDir()
    const cfg = { ...cfgFor(tmp), envFile: path.join(tmp, 'staged/deploy.env') }
    fs.mkdirSync(path.join(tmp, 'staged'))
    fs.writeFileSync(path.join(tmp, 'staged/deploy.env'), 'A=staged\n')
    fs.writeFileSync(path.join(tmp, 'staged/runner'), 'abcdef1\n')
    fs.writeFileSync(path.join(tmp, 'staged/runner-image'), `sha256:${'1'.repeat(64)}\n`)
    fs.mkdirSync(path.join(tmp, 'inputs/a1b2c3d4'), { recursive: true })
    fs.writeFileSync(path.join(tmp, 'inputs/a1b2c3d4/deploy.env'), 'A=attempt\n')
    const exec = createExecutor(cfg, () => null)
    const attempt = {
      ...attemptFixture(false, 'snapshotting'),
      inputs: {
        generation: 'a1b2c3d4',
        ...exec.inputsFacts('a1b2c3d4'),
        runner: 'abcdef1',
        runnerImage: 'sha256:' + '1'.repeat(64),
      },
    }
    const unfinished = {
      ...emptyState(),
      current: 'prev',
      records: { prev: fakeRecord('prev', PREV_INPUTS) },
      attempt,
    }
    atomicWrite(path.join(tmp, 'state.json'), JSON.stringify(unfinished))
    const messages: string[] = []
    expect(await deploy({ config: fakeConfig(), exec, log: (m: string) => messages.push(m) })).toBe(
      EXIT.PREFLIGHT_REFUSED
    )
    expect(messages.join('\n')).toMatch(/unfinished.*run recover/)
    expect(fs.readFileSync(path.join(tmp, 'staged/deploy.env'), 'utf8')).toBe('A=staged\n')
    expect(fs.readFileSync(path.join(tmp, 'inputs/a1b2c3d4/deploy.env'), 'utf8')).toBe(
      'A=attempt\n'
    )
    const activated = exec.activateStaged('b1b2c3d4')
    expect(fs.existsSync(path.join(tmp, 'staged'))).toBe(false)
    expect(fs.readFileSync(path.join(tmp, 'inputs/b1b2c3d4/deploy.env'), 'utf8')).toBe('A=staged\n')
    expect(activated).toEqual({
      generation: 'b1b2c3d4',
      ...exec.inputsFacts('b1b2c3d4'),
      runner: 'abcdef1',
      runnerImage: 'sha256:' + '1'.repeat(64),
    })
    fs.mkdirSync(path.join(tmp, 'staged'))
    fs.writeFileSync(path.join(tmp, 'staged/deploy.env'), 'A=x\n')
    fs.writeFileSync(path.join(tmp, 'staged/runner'), 'not a commit\n')
    expect(() => exec.readStaged()).toThrow(/not a commit id/)
    // Round 7, T2: the runner is recorded by image id; a tag or nothing is refused.
    fs.writeFileSync(path.join(tmp, 'staged/runner'), 'abcdef1\n')
    expect(() => exec.readStaged()).toThrow(/runner image .* missing or not an image id/)
    fs.writeFileSync(path.join(tmp, 'staged/runner-image'), 'forge-deploy-runner:abcdef1\n')
    expect(() => exec.readStaged()).toThrow(/runner image .* missing or not an image id/)
    fs.rmSync(path.join(tmp, 'staged'), { recursive: true })
    expect(() => exec.activateStaged('c1b2c3d4')).toThrow(/no staged inputs/)
    fs.rmSync(tmp, { recursive: true, force: true })
  })

  it('round 4 C2: the journal is the pointer; an orphan generation is never active and may be pruned', () => {
    const committed = {
      ...emptyState(),
      current: 'prev',
      records: { prev: fakeRecord('prev', PREV_INPUTS) },
    }
    expect(activeInputs(committed)).toEqual(PREV_INPUTS)
    const unfinished = { ...committed, attempt: attemptFixture(true, 'migrating') }
    expect(activeInputs(unfinished)?.generation).toBe('a1b2c3d4')
    expect(
      activeInputs({ ...unfinished, attempt: { ...unfinished.attempt, phase: 'rolled_back' } })
    ).toEqual(PREV_INPUTS)
    expect(withPointers(unfinished)).toMatchObject({
      activeGeneration: 'a1b2c3d4',
      activeRunner: 'abcdef1',
    })
    const items = ['prevgen1', 'a1b2c3d4', 'orphan001'].map((name) => ({ kind: 'inputs', name }))
    const names = selectPrunable(items, unfinished, 'a1b2c3d4', ['forge']).map(
      (i: { name: string }) => i.name
    )
    expect(names).toEqual(['orphan001'])
  })

  it('round 4 C3: an expectation or release record a later step needs is validated first', async () => {
    const base = {
      ...emptyState(),
      current: 'prev',
      records: { prev: fakeRecord('prev', PREV_INPUTS) },
    }
    expect(() => validateState(base)).not.toThrow()
    const sealed = attemptFixture(true, 'migrating')
    const bad = [
      { ...base, attempt: { ...sealed, snapshot: { ...sealed.snapshot, expect: {} } } },
      {
        ...base,
        attempt: {
          ...sealed,
          snapshot: { ...sealed.snapshot, expect: { ...sealed.snapshot.expect, streams: [1] } },
        },
      },
      {
        ...base,
        attempt: {
          ...sealed,
          snapshot: {
            ...sealed.snapshot,
            expect: { ...sealed.snapshot.expect, endpoints: { 'API /health': 'maybe' } },
          },
        },
      },
      { ...base, records: { prev: { ...fakeRecord('prev', PREV_INPUTS), config: undefined } } },
      { ...base, records: { prev: { ...fakeRecord('prev', PREV_INPUTS), images: { api: 7 } } } },
      { ...base, records: { prev: fakeRecord('prev') } },
      // Round 5, F3: empty maps would silently disable the image and ledger checks.
      { ...base, records: { prev: { ...fakeRecord('prev', PREV_INPUTS), images: {} } } },
      {
        ...base,
        attempt: {
          ...sealed,
          snapshot: { ...sealed.snapshot, expect: { ...sealed.snapshot.expect, ledgers: {} } },
        },
      },
      {
        ...base,
        attempt: {
          ...sealed,
          snapshot: {
            ...sealed.snapshot,
            expect: { ...sealed.snapshot.expect, ledgers: { api: [] } },
          },
        },
      },
    ]
    for (const state of bad) {
      expect(() => validateState(state, { ledgerKeys: ['api', 'orchestrator'] })).toThrow(
        StateUnreadable
      )
      const world = createWorld()
      world.stateJson = JSON.stringify(state)
      expect(await recover(ctxFor(world, {}))).toBe(EXIT.RECOVERY_REQUIRED)
      expect(await deploy(ctxFor(world, {}))).toBe(EXIT.RECOVERY_REQUIRED)
      expect(world.running).toBe('prev')
    }
  })

  it('finding 6: build is a no-op for a revision that already has a release record', () => {
    const tmp = tmpDir()
    const messages: string[] = []
    const state = {
      ...emptyState(),
      current: 'abc1234',
      records: { abc1234: fakeRecord('abc1234', PREV_INPUTS) },
    }
    atomicWrite(path.join(tmp, 'state.json'), JSON.stringify(state))
    // A stub `docker` on PATH answers `image inspect` with a fixed id: hermetic, no daemon used.
    const bin = path.join(tmp, 'bin')
    fs.mkdirSync(bin)
    const stub = (id: string) =>
      fs.writeFileSync(path.join(bin, 'docker'), `#!/bin/sh\necho ${id}\n`, { mode: 0o755 })
    const savedPath = process.env.PATH
    process.env.PATH = `${bin}:${savedPath}`
    try {
      stub('sha256:1')
      createExecutor(cfgFor(tmp), (m: string) => messages.push(m)).build('abc1234')
      expect(messages.join('\n')).toMatch(/already has a release record; build skipped/)
      // A recorded release whose tag now points elsewhere fails the build (round 2, finding 6).
      stub('sha256:2')
      expect(() => createExecutor(cfgFor(tmp), () => null).build('abc1234')).toThrow(
        /no longer points at the recorded image/
      )
    } finally {
      process.env.PATH = savedPath
      fs.rmSync(tmp, { recursive: true, force: true })
    }
  })
})

describe('round 5', () => {
  it('F3: a bad new intent refuses with 12 and persists nothing; bad persisted state is 11', async () => {
    const world = createWorld()
    const before = world.stateJson
    const exec = fakeExecutor(world)
    const bad = {
      ...exec,
      preflight: () => ({ ...goodPreflight(world), record: { ...fakeRecord('next'), images: {} } }),
    }
    expect(
      await deploy({
        config: fakeConfig(),
        exec: bad,
        log: () => null,
        now: () => '2026-11-24T00:00:00.000Z',
      })
    ).toBe(EXIT.PREFLIGHT_REFUSED)
    expect(world.stateJson).toBe(before)
    expect(world.generations).toEqual(['prevgen1'])
    const broken = createWorld()
    const st = JSON.parse(broken.stateJson)
    st.records.prev.images = {}
    broken.stateJson = JSON.stringify(st)
    expect(await deploy(ctxFor(broken, {}))).toBe(EXIT.RECOVERY_REQUIRED)
  })

  it('F4a: a release id is (revision, env content)', () => {
    expect(releaseId('abc1234', 'f'.repeat(64))).toBe('abc1234-ffffffffffff')
    expect(releaseId('abc1234', 'a'.repeat(64))).not.toBe(releaseId('abc1234', 'f'.repeat(64)))
    expect(NEXT).toBe(releaseId('next', 'b'.repeat(64)))
  })

  it('F4b: the deployment identity ignores secrets and follows locating keys', () => {
    const env = {
      COMPOSE_PROJECT_NAME: 'forge',
      DATABASE_URL: 'postgres://api:old-secret@db.internal:5432/forge',
      ORCHESTRATOR_DATABASE_URL: 'postgres://orch:x@db.internal:5432/forge_orch',
      DB_HOST: 'db.internal',
      TEMPORAL_DB_PASSWORD: 'temporal-old',
      JWT_SECRET: 'jwt-old',
      WEBROOT: '/srv/www',
    }
    const id = identityOf(env)
    const rotated = {
      ...env,
      DATABASE_URL: 'postgres://api:new-secret@db.internal:5432/forge',
      TEMPORAL_DB_PASSWORD: 'temporal-new',
      JWT_SECRET: 'jwt-new',
    }
    expect(identityOf(rotated)).toBe(id)
    // Object-storage credentials are not identity either.
    const keys = {
      S3_ACCESS_KEY: 'a1',
      S3_SECRET_KEY: 's1',
      MINIO_ACCESS_KEY: 'a2',
      MINIO_SECRET_KEY: 's2',
    }
    expect(identityOf({ ...env, ...keys })).toBe(id)
    for (const change of [
      { COMPOSE_PROJECT_NAME: 'other' },
      { DATABASE_URL: 'postgres://api:old-secret@db2.internal:5432/forge' },
      { DATABASE_URL: 'postgres://api:old-secret@db.internal:5432/forge_copy' },
      { ORCHESTRATOR_DATABASE_URL: 'postgres://someone:x@db.internal:5432/forge_orch' },
      { WEBROOT: '/srv/other' },
      { EXTERNAL_NETWORK: 'other-net' },
      { NATS_MONITOR_PORT: '18222' },
      // Round 6, S4: everything that selects where state lives.
      { AGENTFORGE_WORKSPACE_ROOT: '/srv/other-workspaces' },
      { OAUTH_MOUNT_DIR: '/srv/other-oauth' },
      { STORAGE_PROVIDER: 's3' },
      { STORAGE_LOCAL_PATH: '/var/lib/other-uploads' },
      { S3_ENDPOINT: 'https://s3.example.com' },
      { S3_BUCKET: 'other-bucket' },
      { MINIO_ENDPOINT: 'minio.example.com:9000' },
      { MINIO_BUCKET: 'other-bucket' },
    ]) {
      expect(identityOf({ ...env, ...change })).not.toBe(id)
    }
    expect(IDENTITY_KEYS).toContain('COMPOSE_PROJECT_NAME')
  })

  it('F5: the baseline report shows a health-check command only as a fingerprint', () => {
    const secretCmd = ['CMD', 'curl', '-sf', 'http://user:s3cret@localhost:4003/health']
    const rendered = {
      services: {
        api: { container_name: 'p-api', image: 'a:1', healthcheck: { test: ['CMD', 'true'] } },
      },
    }
    const container = {
      name: 'p-api',
      imageId: 'sha256:1',
      env: [],
      mounts: [],
      networks: [],
      portBindings: [],
      healthcheck: secretCmd,
    }
    const [line] = compareBaseline(rendered, [container], { 'a:1': 'sha256:1' }, new Set())
    expect(line).toMatch(/^api: health check command: rendered \d+ chars, sha256 [0-9a-f]{12}/)
    expect(line).toContain(fingerprint(secretCmd))
    expect(line).toContain("docker inspect p-api --format '{{json .Config.Healthcheck.Test}}'")
    expect(line).not.toContain('s3cret')
    expect(line).not.toContain('curl')
  })
})

describe('runner image', () => {
  it('installs the system trust store in the final stage (buildx fetches registry tokens with it)', () => {
    const text = fs.readFileSync(path.join(root, 'docker/Dockerfile.deploy-runner'), 'utf8')
    const final = text.slice(text.lastIndexOf('\nFROM '))
    expect(final).toMatch(/apt-get install -y --no-install-recommends[^\n]*\bca-certificates\b/)
  })
})
