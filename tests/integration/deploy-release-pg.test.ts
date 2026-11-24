/**
 * Real-PostgreSQL integration test for scripts/lib/deploy-release (issue #1319).
 *
 * Drives the REAL database effects (fence, template copy, rename swap, ledger, A5 catalog
 * facts, pruning, helper cleanup, journal I/O) through the engine. Steps that would touch a
 * Compose stack, volumes or a web root are faked. Skipped unless these are set:
 *
 *   FORGE_DEPLOY_IT_NETWORK     Docker network the throwaway server is on
 *   FORGE_DEPLOY_IT_PGHOST      its container name on that network
 *   FORGE_DEPLOY_IT_PGPASSWORD  its `postgres` superuser password
 *
 * Throwaway server (no published ports, nothing outside the `rehearsal-1319-` prefix):
 *   docker network create --internal rehearsal-1319-net
 *   docker run -d --name rehearsal-1319-pg --network rehearsal-1319-net \
 *     --mount type=tmpfs,dst=/var/lib/postgresql -e POSTGRES_PASSWORD="$PW" postgres:18-alpine
 *   FORGE_DEPLOY_IT_NETWORK=rehearsal-1319-net FORGE_DEPLOY_IT_PGHOST=rehearsal-1319-pg \
 *     FORGE_DEPLOY_IT_PGPASSWORD="$PW" npx vitest run --config tests/vitest.config.ts \
 *     --project integration tests/integration/deploy-release-pg.test.ts
 */
import { spawn, spawnSync } from 'node:child_process'
import { createHash, randomBytes } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest'
import { compareLedger, databaseFindings } from '../../scripts/lib/deploy-release/checks.js'
import { createExecutor } from '../../scripts/lib/deploy-release/effects.js'
import { Crash, EXIT, deploy, recover } from '../../scripts/lib/deploy-release/engine.js'
import { emptyState } from '../../scripts/lib/deploy-release/journal.js'
import { fakeRecord } from '../support/deploy-release/simulate.js'

const NET = process.env.FORGE_DEPLOY_IT_NETWORK ?? ''
const HOST = process.env.FORGE_DEPLOY_IT_PGHOST ?? ''
const SUPER = process.env.FORGE_DEPLOY_IT_PGPASSWORD ?? ''
const enabled = Boolean(NET && HOST && SUPER)

const PROJECT = 'rehearsal-1319-forge'
const ATTEMPT = '20261124000000abcd'
const API_DB = `it_api_${'a'.repeat(29)}` // 36 bytes: `<db>__failed_<attempt>` is exactly 63
const DATABASES = [
  { key: 'api', role: 'api', name: API_DB, migrations: 'x' },
  { key: 'orchestrator', role: 'orchestrator', name: 'it_orch', migrations: 'x' },
  { key: 'temporal', role: 'temporal', name: 'it_temporal' },
  { key: 'temporal_visibility', role: 'temporal', name: 'it_temporal_visibility' },
]
const ROLE_NAMES = { api: 'it_api', orchestrator: 'it_orch', temporal: 'it_temporal' }
// A non-owner role with object-level grants inside the API database.
const READER = 'it_reader'
const readerPassword = randomBytes(18).toString('hex')
const ownerOf = (d: { role: string }) => ROLE_NAMES[d.role as keyof typeof ROLE_NAMES]
const passwords = Object.fromEntries(
  Object.keys(ROLE_NAMES).map((k) => [k, randomBytes(18).toString('hex')])
)
const checksum = (text: string) => createHash('sha384').update(text).digest('hex')
const MIGRATIONS = [1, 2].map((v) => ({ version: v, checksum: checksum(`migration ${v}`) }))
const DB_OPS = new Set([
  'disable_connections',
  'terminate_sessions',
  'create_snapshot',
  'enable_connections',
  'verify_snapshot',
  'drop_database',
  'rename',
  'prune',
])

function docker(args: string[], env: Record<string, string> = {}) {
  const r = spawnSync('docker', args, { env: { PATH: process.env.PATH, ...env }, encoding: 'utf8' })
  return { status: r.status, stdout: (r.stdout ?? '').trim(), stderr: (r.stderr ?? '').trim() }
}

function su(database: string, ...statements: string[]) {
  const env = { PGHOST: HOST, PGUSER: 'postgres', PGPASSWORD: SUPER, PGDATABASE: database }
  const args = [
    'run',
    '--rm',
    '--pull',
    'never',
    '--network',
    NET,
    '--name',
    'rehearsal-1319-it-super',
  ]
  const passEnv = Object.keys(env).flatMap((k) => ['-e', k])
  const sql = statements.flatMap((s) => ['-c', s])
  const r = docker(
    [
      ...args,
      ...passEnv,
      'postgres:18-alpine',
      'psql',
      '-X',
      '-q',
      '-v',
      'ON_ERROR_STOP=1',
      '-At',
      ...sql,
    ],
    env
  )
  if (r.status !== 0) {
    throw new Error(`superuser SQL failed: ${r.stderr}`)
  }
  return r.stdout
}

const catalog = () =>
  Object.fromEntries(
    su(
      'postgres',
      "select datname || '|' || oid || '|' || datallowconn from pg_database where datname like 'it\\_%'"
    )
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        const [name, oid, allow] = line.split('|')
        return [name, { oid: Number(oid), allowConnections: allow === 'true' }]
      })
  )
const marker = (database: string) => su(database, 'select v from marker')

// One superuser psql run per database keeps the fixture fast; each -c is its own statement.
function resetFixture() {
  const existing = su('postgres', "select datname from pg_database where datname like 'it\\_%'")
    .split('\n')
    .filter(Boolean)
  const roles = Object.entries(ROLE_NAMES).flatMap(([key, role]) => [
    `ALTER ROLE ${role} CREATEDB LOGIN NOSUPERUSER PASSWORD '${passwords[key]}'`,
    `ALTER ROLE ${role} RESET ALL`,
  ])
  su(
    'postgres',
    ...existing.map((name) => `DROP DATABASE "${name.replaceAll('"', '""')}" WITH (FORCE)`),
    ...roles,
    ...DATABASES.map((d) => `CREATE DATABASE "${d.name}" OWNER ${ownerOf(d)}`)
  )
  for (const d of DATABASES) {
    const ledger = d.migrations
      ? [
          'CREATE TABLE _sqlx_migrations (version bigint primary key, description text not null, installed_on timestamptz not null default now(), success boolean not null, checksum bytea not null, execution_time bigint not null)',
          ...MIGRATIONS.map(
            (m) =>
              `INSERT INTO _sqlx_migrations VALUES (${m.version}, 'm', now(), true, decode('${m.checksum}', 'hex'), 1)`
          ),
        ]
      : []
    const extras = d.key === 'api' ? ['CREATE EXTENSION pgcrypto', 'CREATE EXTENSION pg_trgm'] : []
    su(
      d.name,
      `${extras.join('; ')}${extras.length ? ';' : ''} SET ROLE ${ownerOf(d)}; ${d.key === 'api' ? 'CREATE SCHEMA extra;' : ''} ${ledger.join('; ')}${ledger.length ? ';' : ''} CREATE TABLE marker (v text); INSERT INTO marker VALUES ('original'); CREATE SEQUENCE seq; SELECT setval('seq', 4242);${d.key === 'api' ? ` GRANT SELECT ON marker TO ${READER};` : ''}`
    )
  }
}

function asReader(database: string, sql: string) {
  const env = { PGHOST: HOST, PGUSER: READER, PGPASSWORD: readerPassword, PGDATABASE: database }
  const args = [
    'run',
    '--rm',
    '--pull',
    'never',
    '--network',
    NET,
    '--name',
    'rehearsal-1319-it-reader',
  ]
  const r = docker(
    [
      ...args,
      ...Object.keys(env).flatMap((k) => ['-e', k]),
      'postgres:18-alpine',
      'psql',
      '-X',
      '-At',
      '-c',
      sql,
    ],
    env
  )
  return r.status === 0 ? r.stdout : `ERROR ${r.stderr}`
}

// Each helper container takes seconds on a loaded host; these limits are deliberately generous.
const SLOW = 1_500_000

describe.skipIf(!enabled)('deploy-release against a real PostgreSQL', () => {
  let tmp = ''
  const logs: string[] = []
  const cfg = () => ({
    project: PROJECT,
    target: 'next',
    attemptId: ATTEMPT,
    externalNetwork: NET,
    pgClientImage: 'postgres:18-alpine',
    agentNetwork: 'rehearsal-1319-none',
    roles: Object.fromEntries(
      Object.entries(ROLE_NAMES).map(([key, user]) => [
        key,
        { host: HOST, port: '5432', user, password: passwords[key], sslmode: '' },
      ])
    ),
    databases: DATABASES,
    secrets: Object.values(passwords),
    env: {},
    privateRoot: tmp,
    releaseRoot: tmp,
    webRoot: path.join(tmp, 'www'),
    envFile: path.join(tmp, 'staged', 'deploy.env'),
    volumes: [],
  })

  interface Options {
    failVerify?: boolean
    crashAfter?: { op: string; nth: number }
    killHelperDuring?: { op: string; nth: number }
    beforeRollback?: () => void
  }
  function hybrid(real: ReturnType<typeof createExecutor>, opts: Options = {}) {
    let seen = 0
    let killSeen = 0
    let { beforeRollback } = opts
    return {
      ...real,
      preflight: () => ({
        envVersion: 'next',
        rendered: { services: {} },
        images: {},
        bindProbes: [],
        freeBytes: { private: 2 ** 40 },
        frontendIndexSha: 'sha-next',
        ledgers: real.ledgerRows(),
        targetMigrations: {
          api: [...MIGRATIONS, { version: 3, checksum: 'c3' }],
          orchestrator: MIGRATIONS,
        },
        containers: [],
        databases: real.databaseFacts(),
        agentContainers: [],
        volumes: [],
        expect: {
          endpoints: {},
          streams: [],
          consumers: [],
          ledgers: Object.fromEntries(
            Object.entries(real.ledgerRows()).map(([k, rows]) => [
              k,
              (rows as { version: number; checksum: string }[]).map((r) => ({
                version: r.version,
                checksum: r.checksum,
              })),
            ])
          ),
        },
        captureProblems: [],
        rollback: {
          configReadable: true,
          releaseDirExists: true,
          bindProbes: [],
          missingImages: [],
        },
        tagIds: { 'nats:2': 'sha256:1' },
        record: fakeRecord('next'),
      }),
      run: async (step: { op: string; phase: string; rev?: string }, st: unknown) => {
        if (step.phase === 'rolling_back' && beforeRollback) {
          beforeRollback()
          beforeRollback = undefined
        }
        if (opts.killHelperDuring?.op === step.op && ++killSeen === opts.killHelperDuring.nth) {
          // Kill the SQL helper while this statement is in flight (from another process).
          const name = `${PROJECT}-deploy-${ATTEMPT}-sql`
          spawn('sh', ['-c', `sleep 0.3; docker kill ${name} >/dev/null 2>&1`], {
            detached: true,
            stdio: 'ignore',
          }).unref()
        }
        if (DB_OPS.has(step.op)) {
          const result = await real.run(step, st)
          if (opts.crashAfter?.op === step.op && ++seen === opts.crashAfter.nth) {
            throw new Crash(`crash after ${step.op}`)
          }
          return result
        }
        if (step.op === 'start' && step.rev?.startsWith('next-')) {
          DATABASES.forEach((d) => su(d.name, "UPDATE marker SET v = 'written by next'"))
        }
        if (step.op === 'verify_backend' && step.rev?.startsWith('next-') && opts.failVerify) {
          throw new Error('verification of next failed (test)')
        }
        return step.op === 'archive_webroot' ? { sha256: 'webroot' } : {}
      },
    }
  }
  const ctx = (opts: Options = {}) => {
    const real = createExecutor(cfg(), (m: string) => logs.push(m))
    const config = {
      ...cfg(),
      simulateVerifyFailure: false,
      snapshotDir: path.join(tmp, 'snapshots', ATTEMPT),
      now: new Date().toISOString(),
    }
    return { config, exec: hybrid(real, opts), log: (m: string) => logs.push(m) }
  }
  const state = () => JSON.parse(fs.readFileSync(path.join(tmp, 'state.json'), 'utf8'))
  // Direct effect calls outside the engine need the SQL helper that the engine normally starts.
  async function session<T>(body: (real: ReturnType<typeof createExecutor>) => T | Promise<T>) {
    const real = createExecutor(cfg(), () => null)
    real.startSql()
    try {
      return await body(real)
    } finally {
      real.stopSql()
    }
  }
  const named = (name: string, fact: unknown) => ({ name, fact })
  const expectRolledBack = (before: Record<string, { oid: number }>) => {
    const after = catalog()
    const attempt = state().attempt
    expect(attempt.phase).toBe('rolled_back')
    for (const d of attempt.snapshot.databases) {
      expect(named(d.source, after[d.source])).toEqual(
        named(d.source, { oid: d.snapshotOid, allowConnections: true })
      )
      expect(named(d.failed, after[d.failed])).toEqual(
        named(d.failed, { oid: before[d.source].oid, allowConnections: false })
      )
      expect(marker(d.source)).toBe('original')
    }
    expect(Buffer.byteLength(attempt.snapshot.databases[0].failed)).toBe(63)
    expect(
      su(
        API_DB,
        "select string_agg(extname, ',' order by extname) from pg_extension where extname in ('pgcrypto','pg_trgm')"
      )
    ).toBe('pg_trgm,pgcrypto')
    expect(su(API_DB, "select count(*) from pg_namespace where nspname = 'extra'")).toBe('1')
    expect(su(API_DB, 'select last_value from seq')).toBe('4242')
  }

  beforeAll(() => {
    const create = (role: string) =>
      `DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '${role}') THEN CREATE ROLE ${role} LOGIN; END IF; END $$`
    su(
      'postgres',
      ...[...Object.values(ROLE_NAMES), READER].map(create),
      `ALTER ROLE ${READER} LOGIN NOCREATEDB PASSWORD '${readerPassword}'`
    )
  }, SLOW)

  beforeEach(() => {
    resetFixture()
    tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-it-'))
    // Staged inputs, as Jenkins leaves them; the deploy turns them into a generation.
    fs.mkdirSync(path.join(tmp, 'staged'))
    fs.writeFileSync(
      path.join(tmp, 'staged', 'deploy.env'),
      'COMPOSE_PROJECT_NAME=rehearsal-1319-forge\n'
    )
    fs.writeFileSync(path.join(tmp, 'staged', 'runner'), 'abcdef1\n')
    fs.writeFileSync(path.join(tmp, 'staged', 'runner-image'), `sha256:${'1'.repeat(64)}\n`)
    const real = createExecutor(cfg(), () => null)
    real.writeState({
      ...emptyState(),
      current: 'prev',
      records: {
        prev: fakeRecord('prev', {
          generation: 'prevgen1',
          envSha256: 'a'.repeat(64),
          identity: 'c'.repeat(64),
          runner: 'abcdef0',
          runnerImage: 'sha256:' + '1'.repeat(64),
        }),
      },
    })
  }, SLOW)

  afterAll(() => {
    docker(['rm', '-f', '-v', 'rehearsal-1319-it-blocker'])
  })

  it(
    'A5 preflight facts from the real catalog, each assertion failing in turn',
    () =>
      session((real) => {
        const facts = () => databaseFindings(DATABASES, real.databaseFacts())
        expect(facts()).toEqual([])
        su('postgres', `GRANT CONNECT ON DATABASE "${API_DB}" TO it_orch`)
        expect(facts()).toEqual([`database ${API_DB} has a non-default ACL`])
        su('postgres', `UPDATE pg_database SET datacl = NULL WHERE datname = '${API_DB}'`)
        su('postgres', "ALTER DATABASE it_orch SET work_mem = '8MB'")
        expect(facts()).toEqual(['database it_orch has database or role settings'])
        su(
          'postgres',
          'ALTER DATABASE it_orch RESET ALL',
          "ALTER ROLE it_temporal SET work_mem = '8MB'"
        )
        expect(facts()).toEqual([
          'database it_temporal has database or role settings',
          'database it_temporal_visibility has database or role settings',
        ])
        su('postgres', 'ALTER ROLE it_temporal RESET ALL', 'ALTER ROLE it_orch NOCREATEDB')
        expect(facts()).toEqual(['database it_orch owner role lacks CREATEDB'])
        su(
          'postgres',
          'ALTER ROLE it_orch CREATEDB',
          'ALTER DATABASE it_temporal CONNECTION LIMIT 5'
        )
        expect(facts()).toEqual([
          'database it_temporal has a non-default connection limit (a template copy does not carry it)',
        ])
        su('postgres', 'ALTER DATABASE it_temporal CONNECTION LIMIT -1')
        expect(facts()).toEqual([])
      }),
    SLOW
  )

  it(
    'ledger capture and comparison',
    () =>
      session((real) => {
        const rows = real.ledgerRows()
        expect(rows.api.map((r: { version: number }) => r.version)).toEqual([1, 2])
        expect(
          compareLedger(rows.api, [...MIGRATIONS, { version: 3, checksum: 'c3' }]).pending
        ).toEqual([3])
        su(
          'it_orch',
          "UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 2"
        )
        expect(compareLedger(real.ledgerRows().orchestrator, MIGRATIONS).checksumMismatch).toEqual([
          2,
        ])
      }),
    SLOW
  )

  it(
    'fence + template copy + manifest with OIDs on a successful deploy',
    async () => {
      const before = catalog()
      expect(await deploy(ctx())).toBe(EXIT.OK)
      const after = catalog()
      const { attempt, current, records } = state()
      // Round 5, F4a: the release id is (revision, env hash).
      expect(current).toMatch(/^next-[0-9a-f]{12}$/)
      expect(records[current].revision).toBe('next')
      expect(attempt.phase).toBe('done')
      for (const d of attempt.snapshot.databases) {
        expect(d.sourceOid).toBe(before[d.source].oid)
        expect(named(d.snapshot, after[d.snapshot])).toEqual(
          named(d.snapshot, { oid: d.snapshotOid, allowConnections: true })
        )
        expect(named(d.source, after[d.source])).toEqual(
          named(d.source, { oid: d.sourceOid, allowConnections: true })
        )
        expect(marker(d.snapshot)).toBe('original')
      }
      expect(su(`${API_DB}__snap_${ATTEMPT}`, 'select last_value from seq')).toBe('4242')
    },
    SLOW
  )

  it(
    'full rollback by rename swap restores every database, with ALLOW_CONNECTIONS on',
    async () => {
      const before = catalog()
      expect(asReader(API_DB, 'select v from marker')).toBe('original')
      expect(await deploy(ctx({ failVerify: true }))).toBe(EXIT.ROLLED_BACK)
      expectRolledBack(before)
      // Object-level grants live inside the database, so the restored copy carries them.
      expect(asReader(API_DB, 'select v from marker')).toBe('original')
    },
    SLOW
  )

  it(
    'interrupted after the first rename and before the second, then recover',
    async () => {
      const before = catalog()
      await expect(
        deploy(ctx({ failVerify: true, crashAfter: { op: 'rename', nth: 1 } }))
      ).rejects.toBeInstanceOf(Crash)
      const mid = catalog()
      expect(mid[API_DB]).toBeUndefined()
      expect(mid[`${API_DB}__failed_${ATTEMPT}`].oid).toBe(before[API_DB].oid)
      expect(state().attempt.phase).toBe('rolling_back')
      expect(await recover(ctx())).toBe(EXIT.OK)
      expectRolledBack(before)
    },
    SLOW
  )

  it(
    'a session that blocks a rename -> recovery_required, then recover once it ends',
    async () => {
      const before = catalog()
      const env = {
        PGHOST: HOST,
        PGUSER: 'postgres',
        PGPASSWORD: SUPER,
        PGDATABASE: API_DB,
        PGAPPNAME: 'forge-it-blocker',
      }
      const blocker = () => {
        const args = [
          'run',
          '-d',
          '--rm',
          '--pull',
          'never',
          '--network',
          NET,
          '--name',
          'rehearsal-1319-it-blocker',
        ]
        const passEnv = Object.keys(env).flatMap((k) => ['-e', k])
        expect(
          docker(
            [...args, ...passEnv, 'postgres:18-alpine', 'psql', '-c', 'select pg_sleep(600)'],
            env
          ).status
        ).toBe(0)
        const query =
          "select count(*) from pg_stat_activity where application_name = 'forge-it-blocker'"
        for (let i = 0; i < 50 && su('postgres', query) !== '1'; i += 1) {
          spawnSync('sleep', ['0.2'])
        }
      }
      expect(await deploy(ctx({ failVerify: true, beforeRollback: blocker }))).toBe(
        EXIT.RECOVERY_REQUIRED
      )
      const stuck = state().attempt
      expect(stuck.phase).toBe('recovery_required')
      expect(stuck.error).toMatch(/rename/)
      expect(stuck.error).not.toContain(passwords.api)
      expect(logs.join('\n')).not.toContain(SUPER)
      expect(await deploy(ctx())).toBe(EXIT.PREFLIGHT_REFUSED)
      su(
        'postgres',
        "select pg_terminate_backend(pid) from pg_stat_activity where application_name = 'forge-it-blocker'"
      )
      docker(['rm', '-f', '-v', 'rehearsal-1319-it-blocker'])
      expect(await recover(ctx())).toBe(EXIT.OK)
      expectRolledBack(before)
    },
    SLOW
  )

  it(
    'failure during the snapshot keeps the originals: copies dropped, no renames, connections back on',
    async () => {
      const before = catalog()
      await expect(
        deploy(ctx({ crashAfter: { op: 'create_snapshot', nth: 2 } }))
      ).rejects.toBeInstanceOf(Crash)
      expect(catalog()[`it_orch__snap_${ATTEMPT}`]).toBeDefined()
      expect(await recover(ctx())).toBe(EXIT.OK)
      const after = catalog()
      expect(Object.keys(after).sort()).toEqual(Object.keys(before).sort())
      for (const [name, fact] of Object.entries(before)) {
        expect(after[name]).toEqual({ ...fact, allowConnections: true })
      }
    },
    SLOW
  )

  it(
    'pruning drops only generated names of configured sources, at the 63-byte limit too',
    async () => {
      const create = (name: string, owner: string) =>
        su('postgres', `CREATE DATABASE "${name.replaceAll('"', '""')}" OWNER ${owner}`)
      const eligible = ['it_orch__snap_old0000001', `${API_DB}__failed_old000000000000abc`]
      const foreign = [
        'it_orch__snap_good000001',
        'it_orchx__snap_old0000001',
        'it_it_orch__snap_old0000001',
        'it_orch__snap_old0000001_copy',
        'it_orch__snapshot_old0000001',
        'it_orch__snap_old"0000001',
        'it_other__failed_old0000001',
      ]
      expect(Buffer.byteLength(eligible[1])).toBe(63)
      eligible.forEach((n) => create(n, n.startsWith('it_orch') ? 'it_orch' : 'it_api'))
      foreign.forEach((n) => create(n, 'it_orch'))
      const st = {
        ...emptyState(),
        current: 'next',
        previous: 'prev',
        protected: { baseline: null, lastGood: 'good000001' },
      }
      await session((real) => real.run({ op: 'prune', phase: 'verified' }, st))
      const left = Object.keys(catalog())
      eligible.forEach((n) => expect(left).not.toContain(n))
      foreign.forEach((n) => expect(left).toContain(n))
      DATABASES.forEach((d) => expect(left).toContain(d.name))
    },
    SLOW
  )

  it(
    'PostgreSQL fact: a template copy does not carry a database-level ACL, so preflight refuses one',
    () => {
      su(
        'postgres',
        'CREATE DATABASE it_acl_src OWNER it_orch',
        `GRANT CONNECT ON DATABASE it_acl_src TO ${READER}`,
        'CREATE DATABASE it_acl_copy TEMPLATE it_acl_src OWNER it_orch'
      )
      const acl = su(
        'postgres',
        "select datname || '=' || coalesce(datacl::text, 'default') from pg_database where datname like 'it\\_acl\\_%' order by 1"
      )
      expect(acl.split('\n')[0]).toBe('it_acl_copy=default')
      expect(acl.split('\n')[1]).toMatch(/^it_acl_src=\{.*it_reader=c/)
    },
    SLOW
  )

  it(
    'an error inside a batched read fails the step; no partial result is accepted',
    () =>
      session(async (real) => {
        const copy = 'it_batch_copy'
        su('postgres', `CREATE DATABASE ${copy} TEMPLATE "${API_DB}" OWNER it_api`)
        su(copy, 'DROP TABLE _sqlx_migrations')
        const step = {
          op: 'verify_snapshot',
          phase: 'snapshotting',
          db: 'api',
          name: API_DB,
          snapshot: copy,
        }
        await expect(real.run(step, { ...emptyState() })).rejects.toThrow(/exited/)
      }),
    SLOW
  )

  it(
    'the SQL helper killed during an in-flight rename, then recover finishes from the real names/OIDs',
    async () => {
      const before = catalog()
      const code = await deploy(
        ctx({ failVerify: true, killHelperDuring: { op: 'rename', nth: 1 } })
      )
      expect(code).toBe(EXIT.RECOVERY_REQUIRED)
      expect(state().attempt.phase).toBe('recovery_required')
      expect(await recover(ctx())).toBe(EXIT.OK)
      expectRolledBack(before)
    },
    SLOW
  )

  it(
    'the maintenance database unreachable -> clean refusal before any mutation',
    async () => {
      const before = catalog()
      su('template1', 'ALTER DATABASE postgres WITH ALLOW_CONNECTIONS false')
      let code
      try {
        code = await deploy(ctx())
      } finally {
        su('template1', 'ALTER DATABASE postgres WITH ALLOW_CONNECTIONS true')
      }
      expect(code).toBe(EXIT.PREFLIGHT_REFUSED)
      expect(state().attempt).toBeNull()
      expect(catalog()).toEqual(before)
      expect(logs.join('\n')).toMatch(/preflight could not run/)
    },
    SLOW
  )

  it(
    'one SQL helper per session, named after project and attempt, gone afterwards',
    async () => {
      const filter = ['--filter', `label=forge.deploy.project=${PROJECT}`]
      const listed = () => docker(['ps', '-a', '--format', '{{.Names}}', ...filter]).stdout
      const stats = await session((real) => {
        expect(listed()).toBe(`${PROJECT}-deploy-${ATTEMPT}-sql`)
        real.databaseFacts()
        real.ledgerRows()
        return real.stats()
      })
      // databaseFacts: catalog + connectable databases per role (3 roles); ledgerRows: 2
      expect(stats).toEqual({ helperStarts: 1, sqlInvocations: 6 + 2 })
      expect(listed()).toBe('')
    },
    SLOW
  )
})
