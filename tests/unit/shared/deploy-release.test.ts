import { createHash } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  agentContainerFindings,
  backendFindings,
  compareBaseline,
  compareLedger,
  databaseFindings,
  frontendFindings,
  identityFindings,
  ledgerFindings,
  parseEnvFile,
  parseMigrations,
  preflightFindings,
  redact,
  secretValues,
} from '../../../scripts/lib/deploy-release/checks.js'
import { atomicWrite, createExecutor } from '../../../scripts/lib/deploy-release/effects.js'
import { Crash, EXIT, deploy, recover } from '../../../scripts/lib/deploy-release/engine.js'
import {
  PHASES,
  announce,
  complete,
  emptyState,
  markRecoveryRequired,
  transition,
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
  fakeConfig,
  fakeExecutor,
  faultMatrix,
  invariantErrors,
} from '../../../scripts/lib/deploy-release/simulate.js'

const root = path.resolve(import.meta.dirname, '../../..')
const ops = (steps: { op: string }[]) => steps.map((s) => s.op)

function attemptFixture(complete: boolean, phase = 'rolling_back') {
  return {
    attemptId: 'a1b2c3d4',
    target: 'next',
    previous: 'prev',
    phase,
    step: null,
    migrationContainer: 'forge-migrate-a1b2c3d4',
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
    },
  }
}

const db = (oid: number, allowConnections = true) => ({ oid, allowConnections })

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

  it('commit moves current/previous, protects the baseline snapshot and drops stale records', () => {
    const state = {
      ...emptyState(),
      current: 'baseline',
      records: { baseline: {}, old: {}, next: {} },
      attempt: { ...attemptFixture(true, 'verifying_frontend'), previous: 'baseline' },
    }
    const step = { op: 'commit', phase: 'verified' }
    const done = complete(announce(state, step, 't'), step, {}, 't')
    expect(done.current).toBe('next')
    expect(done.previous).toBe('baseline')
    expect(done.protected).toEqual({ baseline: 'a1b2c3d4', lastGood: 'a1b2c3d4' })
    expect(Object.keys(done.records).sort()).toEqual(['baseline', 'next'])
  })

  it('marks recovery_required from any in-flight phase', () => {
    for (const phase of ['migrating', 'rolling_back', 'recovery_required']) {
      const state = { ...emptyState(), attempt: attemptFixture(true, phase) }
      expect(markRecoveryRequired(state, 'boom', 't').attempt.phase).toBe('recovery_required')
    }
  })
})

describe('planRecovery', () => {
  const sourceOnly = { databases: { forge: db(1) }, migrationContainer: false }
  const swapped = {
    databases: { forge: db(2), forge__failed_a1b2c3d4: db(1, false) },
    migrationContainer: false,
  }

  it.each(PHASES)('plans a defined outcome for phase %s', (phase) => {
    for (const sealed of [false, true]) {
      const steps = planRecovery(attemptFixture(sealed, phase), sealed ? swapped : sourceOnly)
      if (phase === 'done' || phase === 'rolled_back') {
        expect(steps).toEqual([])
      } else if (phase === 'verified') {
        expect(ops(steps)).toEqual(['close'])
      } else {
        expect(steps.at(-1)).toEqual({ op: 'close', phase: 'rolled_back' })
        expect(ops(steps)).toContain('verify_backend')
        expect(ops(steps).includes('restore_volume')).toBe(sealed)
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
      const steps = planRecovery(attemptFixture(false), { databases, migrationContainer: false })
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
      migrationContainer: false,
    })
    expect(ops(steps)).not.toContain('drop_database')
  })

  const swapOps = (databases: Record<string, { oid: number; allowConnections: boolean }>) =>
    planRecovery(attemptFixture(true), { databases, migrationContainer: false })
      .filter((s) => 'db' in s)
      .map((s) => `${s.op}${'from' in s ? ` ${s.from}->${s.to}` : ''}`)

  it.each([
    [
      'before the swap',
      { forge: db(1), forge__snap_a1b2c3d4: db(2) },
      [
        'disable_connections',
        'terminate_sessions',
        'rename forge->forge__failed_a1b2c3d4',
        'rename forge__snap_a1b2c3d4->forge',
        'enable_connections',
      ],
    ],
    [
      'after disable/terminate',
      { forge: db(1, false), forge__snap_a1b2c3d4: db(2) },
      [
        'disable_connections',
        'terminate_sessions',
        'rename forge->forge__failed_a1b2c3d4',
        'rename forge__snap_a1b2c3d4->forge',
        'enable_connections',
      ],
    ],
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

  it('removes a leftover migration container and skips completed restores', () => {
    const attempt = attemptFixture(true)
    attempt.snapshot.volumes[0].restored = true
    attempt.snapshot.webroot.restored = true
    const steps = planRecovery(attempt, { ...swapped, migrationContainer: true })
    expect(ops(steps)[0]).toBe('remove_container')
    expect(ops(steps)).not.toContain('restore_volume')
    expect(ops(steps)).not.toContain('restore_webroot')
  })

  it('is idempotent: re-planning after an executed recovery yields no database or restore steps', async () => {
    const world = createWorld()
    const failing = {
      config: fakeConfig(),
      exec: fakeExecutor(world, { failAt: new Set([28]) }),
      log: () => {},
    }
    expect(await deploy(failing)).toBe(EXIT.ROLLED_BACK)
    const attempt = JSON.parse(world.stateJson).attempt
    expect(planRecovery(attempt, fakeExecutor(world).observe())).toEqual([])
    const reopened = { ...attempt, phase: 'rolling_back' }
    const again = ops(planRecovery(reopened, fakeExecutor(world).observe()))
    expect(again).not.toContain('rename')
    expect(again).not.toContain('restore_volume')
    expect(again).not.toContain('drop_database')
  })
})

describe('planDeploy', () => {
  const state = { ...emptyState(), current: 'prev', records: { prev: {} } }

  it('is a no-op when the target is current and refuses on any preflight finding', () => {
    const world = createWorld()
    const observed = fakeExecutor(world).preflight()
    expect(planDeploy(fakeConfig({ target: 'prev' }), state, observed).status).toBe('noop')
    const refused = planDeploy(fakeConfig(), state, { ...observed, envVersion: 'other' })
    expect(refused.status).toBe('refused')
  })

  it('journals snapshot names with the attempt id and snapshots before migrating', () => {
    const world = createWorld()
    const plan = planDeploy(fakeConfig(), state, fakeExecutor(world).preflight())
    expect(plan.attempt.snapshot.databases.map((d: { snapshot: string }) => d.snapshot)).toContain(
      'forge__snap_a1b2c3d4'
    )
    const order = ops(plan.steps)
    expect(order.lastIndexOf('create_snapshot')).toBeLessThan(order.indexOf('seal'))
    expect(order.indexOf('seal')).toBeLessThan(order.indexOf('migrate'))
    expect(order.indexOf('verify_backend')).toBeLessThan(order.indexOf('switch_frontend'))
    expect(order.indexOf('verify_frontend')).toBeLessThan(order.indexOf('commit'))
  })

  it('names never collide across attempts and respect the 63-byte limit', () => {
    expect(snapshotName('forge', 'a1b2c3d4')).not.toBe(snapshotName('forge', 'a1b2c3d5'))
    expect(failedName('forge', 'a1b2c3d4')).toBe('forge__failed_a1b2c3d4')
    expect(() => snapshotName('x'.repeat(50), '202611221015300a1b')).toThrow(/63/)
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

describe('retention', () => {
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
    { kind: 'snapshotDir', name: 'base0001' },
    { kind: 'snapshotDir', name: 'old00001' },
    { kind: 'releaseDir', name: 'c0ffee1' },
    { kind: 'releaseDir', name: 'beef001' },
    { kind: 'releaseDir', name: 'baseline' },
    { kind: 'releaseDir', name: 'dead001' },
  ]

  it('never selects current, previous, last-good, baseline, the active attempt or unmarked databases', () => {
    const names = selectPrunable(items, state, 'live0001').map((i: { name: string }) => i.name)
    expect(names.sort()).toEqual([
      'dead001',
      'forge__failed_old00001',
      'forge__snap_old00001',
      'old00001',
    ])
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
  ])('A5 refuses when %s is %s', (key, value) => {
    expect(databaseFindings([{ name: 'forge' }], { forge: good })).toEqual([])
    expect(
      databaseFindings([{ name: 'forge' }], { forge: { ...good, [key]: value } })
    ).toHaveLength(1)
  })

  it('refuses a missing database and running agent containers (A8)', () => {
    expect(databaseFindings([{ name: 'forge' }], {})).toHaveLength(1)
    const agent = { name: 'agent-1', labels: { 'agentforge.agent_id': 'x' } }
    const compose = { name: 'agentforge-nats', labels: { 'com.docker.compose.service': 'nats' } }
    expect(agentContainerFindings([agent, compose])).toHaveLength(1)
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

  it('aggregates findings, including a missing baseline and low disk', () => {
    const observed = fakeExecutor(createWorld()).preflight()
    const config = fakeConfig()
    const state = { ...emptyState(), current: 'prev', records: { prev: {} } }
    expect(preflightFindings(config, state, observed)).toEqual([])
    expect(preflightFindings(config, emptyState(), observed)).toHaveLength(1)
    expect(
      preflightFindings(config, state, { ...observed, freeBytes: { private: 10 } })
    ).toHaveLength(1)
    expect(preflightFindings(config, state, { ...observed, images: { nats: null } })).toHaveLength(
      1
    )
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
    ledgers: {},
  }
  const expectations = { images: { nats: 'sha256:1' }, requireRedis: false, contract }

  it('passes only when every check passes; a check that cannot run is a failure', () => {
    expect(backendFindings(facts, expectations)).toEqual([])
    expect(backendFindings(facts, { ...expectations, requireRedis: true })).toHaveLength(1)
    expect(
      backendFindings({ ...facts, orchHealth: ok({ workflowRuntime: 'disabled' }) }, expectations)
    ).toHaveLength(1)
    expect(
      backendFindings({ ...facts, natsHealth: { error: 'timeout' } }, expectations)[0]
    ).toMatch(/could not run/)
    expect(
      backendFindings({ ...facts, jsz: ok({ account_details: [] }) }, expectations)
    ).toHaveLength(5)
    expect(backendFindings({ ...facts, runningImages: {} }, expectations)).toHaveLength(1)
    expect(backendFindings({ ...facts, temporalServing: false }, expectations)).toHaveLength(1)
  })

  it('compares the served and on-disk index.html with the release copy', () => {
    expect(frontendFindings({ diskSha: 'a', served: { sha: 'a' } }, 'a')).toEqual([])
    expect(frontendFindings({ diskSha: 'a', served: null }, 'a')[0]).toMatch(/could not run/)
    expect(frontendFindings({ diskSha: 'b', served: { sha: 'c' } }, 'a')).toHaveLength(2)
  })

  it('compares the rendered baseline with the inspected running containers', () => {
    const rendered = {
      services: {
        nats: {
          container_name: 'p-nats',
          image: 'nats:2',
          environment: { A: '1' },
          volumes: [{ target: '/data' }],
          networks: { default: {} },
          ports: [{ target: 4222, protocol: 'tcp' }],
          healthcheck: { test: ['CMD', 'true'] },
        },
      },
      networks: { default: { name: 'p_default' } },
    }
    const container = {
      name: 'p-nats',
      imageId: 'sha256:1',
      env: ['A=1', 'PATH=/bin'],
      mounts: [{ target: '/data' }],
      networks: ['p_default'],
      ports: ['4222/tcp'],
      healthcheck: ['CMD', 'true'],
    }
    expect(compareBaseline(rendered, [container], { 'nats:2': 'sha256:1' })).toEqual([])
    expect(
      compareBaseline(rendered, [{ ...container, networks: [] }], { 'nats:2': 'sha256:1' })
    ).toHaveLength(1)
    expect(compareBaseline(rendered, [container], { 'nats:2': 'sha256:2' })).toHaveLength(1)
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
  it('every injected failure and crash ends original-or-snapshot under each name, nothing lost', async () => {
    const results = await faultMatrix()
    expect(results.length).toBeGreaterThan(100)
    expect(results.filter((r) => r.errors.length > 0)).toEqual([])
  })

  it('a crash during the migration leaves a container that recovery removes', async () => {
    const world = createWorld()
    // stop_project, 4 databases x 5 snapshot steps, 2 volume archives, the web root archive
    const migrateAt = 1 + 4 * 5 + 2 + 1
    const crashing = {
      config: fakeConfig(),
      exec: fakeExecutor(world, { crash: { at: migrateAt, when: 'before' } }),
      log: () => {},
    }
    await expect(deploy(crashing)).rejects.toBeInstanceOf(Crash)
    expect(world.migrationContainer).toBe(true)
    expect(await recover({ config: fakeConfig(), exec: fakeExecutor(world), log: () => {} })).toBe(
      EXIT.OK
    )
    expect(world.migrationContainer).toBe(false)
    expect(invariantErrors(world)).toEqual([])
  })

  it('rehearsal D: snapshot unavailable during rollback -> recovery_required, deploy refuses, recover after repair', async () => {
    const world = createWorld()
    let hidden: unknown = null
    const hide = (w: typeof world) => {
      hidden = w.databases['forge__snap_a1b2c3d4']
      delete w.databases['forge__snap_a1b2c3d4']
    }
    const failing = {
      config: fakeConfig({ simulateVerifyFailure: true }),
      exec: fakeExecutor(world, { beforeRollback: hide }),
      log: () => {},
    }
    expect(await deploy(failing)).toBe(EXIT.RECOVERY_REQUIRED)
    expect(JSON.parse(world.stateJson).attempt.phase).toBe('recovery_required')
    expect(world.running).toBeNull()
    expect(
      await deploy({
        config: fakeConfig({ attemptId: 'b1b2c3d4' }),
        exec: fakeExecutor(world),
        log: () => {},
      })
    ).toBe(EXIT.PREFLIGHT_REFUSED)
    world.databases['forge__snap_a1b2c3d4'] = hidden as (typeof world.databases)[string]
    expect(await recover({ config: fakeConfig(), exec: fakeExecutor(world), log: () => {} })).toBe(
      EXIT.OK
    )
    const state = JSON.parse(world.stateJson)
    expect(state.attempt.phase).toBe('rolled_back')
    expect(state.current).toBe('prev')
    expect(invariantErrors(world)).toEqual([])
  })
})

describe('filesystem effects (temporary directory only)', () => {
  it('switches the web root with index.html last, archives it and restores it', async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-test-'))
    const write = (file: string, text: string) => {
      fs.mkdirSync(path.dirname(file), { recursive: true })
      fs.writeFileSync(file, text)
    }
    const webRoot = path.join(tmp, 'www')
    const next = path.join(tmp, 'frontend/next')
    write(path.join(webRoot, 'index.html'), 'old index')
    write(path.join(webRoot, 'assets/old.js'), 'old asset')
    write(path.join(next, 'index.html'), 'new index')
    write(path.join(next, 'assets/new.js'), 'new asset')
    const cfg = {
      webRoot,
      env: {},
      attemptId: 'a1b2c3d4',
      secrets: [],
      databases: [],
      releaseRoot: tmp,
      privateRoot: tmp,
    }
    const exec = createExecutor(cfg, () => null)
    const state = { ...emptyState(), records: { next: { frontendDir: next } } }
    const tar = path.join(tmp, 'webroot.tar')
    const { sha256: archived } = await exec.run({ op: 'archive_webroot', tar }, state)
    await exec.run({ op: 'switch_frontend', rev: 'next' }, state)
    expect(fs.readFileSync(path.join(webRoot, 'index.html'), 'utf8')).toBe('new index')
    expect(fs.readFileSync(path.join(webRoot, 'assets/new.js'), 'utf8')).toBe('new asset')
    expect(fs.existsSync(path.join(webRoot, 'assets/old.js'))).toBe(true)
    await exec.run({ op: 'restore_webroot', tar, sha256: archived }, state)
    expect(fs.readFileSync(path.join(webRoot, 'index.html'), 'utf8')).toBe('old index')
    await expect(exec.run({ op: 'restore_webroot', tar, sha256: 'wrong' }, state)).rejects.toThrow(
      /sha256/
    )
    expect(fs.readdirSync(webRoot).some((f) => f.includes('.forge-tmp-'))).toBe(false)
    atomicWrite(path.join(tmp, 'state.json'), '{"schema":1}')
    expect(fs.statSync(path.join(tmp, 'state.json')).mode & 0o777).toBe(0o600)
    expect(fs.existsSync(path.join(tmp, 'state.json.tmp'))).toBe(false)
    fs.rmSync(tmp, { recursive: true, force: true })
  })
})
