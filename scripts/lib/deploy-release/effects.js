// Effect layer: thin wrappers around docker, docker compose, psql (in a sibling container on the
// external network) and the filesystem. Decisions live in plan.js / checks.js.
/* global fetch, AbortSignal */
import { Buffer } from 'node:buffer'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import {
  backendFindings,
  binding,
  captureExpectations,
  compareBaseline,
  frontendFindings,
  isProductAgent,
  newAgents,
  identityOf,
  parseEnvFile,
  parseMigrations,
  quoteIdent as q,
  quoteLiteral as lit,
  redact,
  sha256,
  tagDriftFindings,
} from './checks.js'
import { StateUnreadable, TERMINAL, emptyState, validateState } from './journal.js'
import { planFrontendSwitch, selectPrunable } from './plan.js'

const MAX_BUFFER = 256 * 1024 * 1024
const PASS_THROUGH = ['PATH', 'HOME', 'TMPDIR', 'DOCKER_HOST', 'DOCKER_CONFIG']
const JOURNAL_ONLY = new Set(['seal', 'commit', 'close'])
const HELPER_LABEL = 'forge.deploy.project'
// Static argument lists are written as one string; dynamic values stay separate array items.
const words = (text) => text.split(' ')
const baseEnv = () =>
  Object.fromEntries(PASS_THROUGH.filter((k) => process.env[k]).map((k) => [k, process.env[k]]))

export function fsyncPath(p) {
  const fd = fs.openSync(p, 'r')
  try {
    fs.fsyncSync(fd)
  } finally {
    fs.closeSync(fd)
  }
}

// Round-3 finding 3: create a directory and fsync the parent of every directory it created, up
// to an existing ancestor, so the new entries survive a crash before anything inside is trusted.
export function ensureDir(dir, mode = 0o755) {
  const created = []
  for (let d = path.resolve(dir); !fs.existsSync(d); d = path.dirname(d)) {
    created.unshift(d)
  }
  fs.mkdirSync(dir, { recursive: true, mode })
  for (const d of [...created].reverse()) {
    fsyncPath(path.dirname(d))
  }
}

// A1: write every byte to a temp file, fsync it, rename over the target, fsync the directory.
export function atomicWrite(file, data) {
  const tmp = `${file}.tmp`
  const fd = fs.openSync(tmp, 'w', 0o600)
  try {
    fs.writeFileSync(fd, data)
    fs.fsyncSync(fd)
  } finally {
    fs.closeSync(fd)
  }
  fs.renameSync(tmp, file)
  fsyncPath(path.dirname(file))
}

function fsyncTree(root) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const full = path.join(root, entry.name)
    if (entry.isDirectory()) {
      fsyncTree(full)
    } else if (entry.isFile()) {
      fsyncPath(full)
    }
  }
  fsyncPath(root)
}

function listFiles(root) {
  const out = []
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) {
        walk(full)
      } else if (entry.isFile() && !entry.name.includes('.forge-tmp-')) {
        out.push({ path: path.relative(root, full), sha256: sha256(fs.readFileSync(full)) })
      }
    }
  }
  walk(root)
  return out
}

const readMigrations = (dir) =>
  parseMigrations(
    fs.readdirSync(dir).map((name) => ({ name, content: fs.readFileSync(path.join(dir, name)) }))
  )

async function fetchJson(url) {
  try {
    const res = await fetch(url, {
      signal: AbortSignal.timeout(5000),
      headers: { 'cache-control': 'no-cache' },
    })
    const bytes = Buffer.from(await res.arrayBuffer())
    let body = null
    try {
      body = JSON.parse(bytes.toString('utf8'))
    } catch {
      body = null
    }
    return { status: res.status, body, bytes }
  } catch (err) {
    return { error: err.name === 'TimeoutError' ? 'timeout' : 'connection failed' }
  }
}

export function createExecutor(cfg, log) {
  const run = (cmd, args, { env = {}, allowFail = false, inherit = false, input } = {}) => {
    const r = spawnSync(cmd, args, {
      env: { ...baseEnv(), ...env },
      input,
      encoding: 'utf8',
      maxBuffer: MAX_BUFFER,
      stdio: inherit ? ['ignore', 'inherit', 'inherit'] : 'pipe',
    })
    if (r.error) {
      throw new Error(`${cmd} could not start: ${r.error.message}`)
    }
    if (r.status !== 0 && !allowFail) {
      const tail = (r.stderr ?? '').trim().split('\n').slice(-4).join(' | ')
      throw new Error(
        `${cmd} ${args.slice(0, 2).join(' ')} exited ${r.status}: ${redact(tail, cfg.secrets)}`
      )
    }
    return r
  }
  const docker = (args, opts) => run('docker', args, opts)
  const lines = (r) => r.stdout.split(/\s+/).filter(Boolean)
  const releaseDir = (rev) => path.join(cfg.releaseRoot, 'releases', rev)
  const configPath = (rev) => path.join(cfg.privateRoot, 'configs', `${rev}.json`)
  const statePath = path.join(cfg.privateRoot, 'state.json')
  // Round 4, C2: Jenkins stages one directory (deploy.env + runner). One rename turns it into the
  // attempt's generation `inputs/<generation>/`; the journal names it. Nothing else is "active".
  const stagedDir = path.join(cfg.privateRoot, 'staged')
  const generationDir = (generation) => path.join(cfg.privateRoot, 'inputs', generation)
  let envPath = cfg.envFile
  const envFile = () => envPath
  // Facts of an input directory: env hash, deployment identity (round 5, F4b) and runner.
  const envFacts = (dir) => {
    const file = path.join(dir, 'deploy.env')
    if (!fs.existsSync(file)) {
      return null
    }
    const text = fs.readFileSync(file)
    return { envSha256: sha256(text), identity: identityOf(parseEnvFile(text.toString('utf8'))) }
  }
  const inputsFacts = (generation) => envFacts(generationDir(generation))
  function readStaged() {
    const facts = envFacts(stagedDir)
    const runnerFile = path.join(stagedDir, 'runner')
    if (!facts || !fs.existsSync(runnerFile)) {
      throw new Error(
        'no staged inputs (private/staged/deploy.env and runner): run the stage action'
      )
    }
    const runner = fs.readFileSync(runnerFile, 'utf8').trim()
    if (!/^[0-9a-f]{7,40}$/.test(runner)) {
      throw new Error('the staged runner reference is not a commit id')
    }
    return { ...facts, runner }
  }
  function activateStaged(generation) {
    const env = path.join(stagedDir, 'deploy.env')
    const staged = readStaged()
    ;[env, path.join(stagedDir, 'runner'), stagedDir].forEach(fsyncPath)
    ensureDir(path.dirname(generationDir(generation)), 0o700)
    fs.renameSync(stagedDir, generationDir(generation))
    fsyncPath(path.dirname(generationDir(generation)))
    fsyncPath(cfg.privateRoot)
    if (envPath === env) {
      envPath = path.join(generationDir(generation), 'deploy.env')
    }
    return { generation, ...staged, ...inputsFacts(generation) }
  }
  const project = [...words('compose --profile external --project-name'), cfg.project]
  const composeSource = (rev) => [
    ...project,
    ...['--env-file', envFile(), '-f', path.join(releaseDir(rev), 'docker/compose.yml')],
    ...['-f', path.join(releaseDir(rev), 'docker/compose.external.yml')],
  ]
  const composeRendered = (file) => [...project, '-f', file]
  let current = null
  const record = (rev) => {
    const r = current?.records[rev]
    if (!r) {
      throw new Error(`no release record for ${rev}`)
    }
    return r
  }
  // Helpers are named after the attempt they serve, so a later recovery can find and stop them.
  const attemptId = () =>
    current?.attempt && !TERMINAL.has(current.attempt.phase)
      ? current.attempt.attemptId
      : cfg.attemptId
  const helperName = (purpose) => `${cfg.project}-deploy-${attemptId()}-${purpose}`
  const helperArgs = (purpose) => [
    ...['--name', helperName(purpose), '--label', `${HELPER_LABEL}=${cfg.project}`],
    ...['--label', `forge.deploy.attempt=${attemptId()}`],
  ]

  const stats = { helperStarts: 0, sqlInvocations: 0 }
  // Round-3 finding 6: the helper is addressed by the container ID recorded when it started,
  // never by a name re-derived later.
  let sqlHelper = null
  // One long-lived client container per attempt; every SQL invocation is a `docker exec` into it.
  // Connection settings reach psql only through the environment (pass-through `-e NAME`), the
  // SQL text through stdin. Each query of a script prints exactly one line.
  const psqlLines = (roleKey, database, script, lines) => {
    const role = cfg.roles[roleKey]
    const env = {
      PGHOST: role.host,
      PGPORT: role.port,
      PGUSER: role.user,
      PGPASSWORD: role.password,
    }
    Object.assign(env, { PGDATABASE: database }, role.sslmode ? { PGSSLMODE: role.sslmode } : {})
    const passEnv = Object.keys(env).flatMap((k) => ['-e', k])
    stats.sqlInvocations += 1
    const args = [
      'exec',
      '-i',
      ...passEnv,
      sqlHelper?.id ?? 'no-sql-helper-started',
      ...words('psql -X -q -v ON_ERROR_STOP=1 -At -f -'),
    ]
    const out = docker(args, { env, input: script }).stdout.split('\n').filter(Boolean)
    if (lines !== undefined && out.length !== lines) {
      throw new Error(`SQL batch returned ${out.length} line(s), expected ${lines}`)
    }
    return out
  }
  const psql = (roleKey, database, sql) => psqlLines(roleKey, database, sql, 1)[0]
  const sqlDo = (roleKey, database, sql) => psqlLines(roleKey, database, sql, 0)
  const connect = (database) => `\\c ${q(database)}`
  const names = (list) => `array[${list.map(lit).join(',')}]::text[]`
  const roleOf = (key) => cfg.databases.find((d) => d.key === key).role
  const roles = () => [...new Set(cfg.databases.map((d) => d.role))]
  const ledgerKeys = cfg.databases.filter((d) => d.migrations)
  // jsonb output never contains newlines, so one query is one line.
  const STATUS_SQL = (list) =>
    `select coalesce(jsonb_object_agg(datname, jsonb_build_object('oid', oid::bigint, 'allowConnections', datallowconn)), '{}') from pg_database where datname = any(${names(list)});`
  const LEDGER_SQL =
    "select coalesce(jsonb_agg(jsonb_build_object('version', version, 'checksum', encode(checksum, 'hex'), 'success', success) order by version), '[]') from _sqlx_migrations;"
  const CATALOG_SQL = (list) =>
    `select coalesce(jsonb_object_agg(d.datname, jsonb_build_object('oid', d.oid::bigint, 'allowConnections', d.datallowconn, 'ownedByRole', r.rolname = current_user, 'ownerCreatedb', r.rolcreatedb, 'defaultAcl', d.datacl is null, 'connLimit', d.datconnlimit, 'defaultTablespace', d.dattablespace = (select oid from pg_tablespace where spcname = 'pg_default'), 'settings', (select count(*) from pg_db_role_setting s where s.setdatabase = d.oid or s.setrole = d.datdba), 'slots', (select count(*) from pg_replication_slots s where s.database = d.datname), 'subscriptions', (select count(s.subdbid) from pg_subscription s where s.subdbid = d.oid))), '{}') from pg_database d join pg_roles r on r.oid = d.datdba where d.datname = any(${names(list)});`
  const dbStatus = (roleKey, list) => JSON.parse(psql(roleKey, 'postgres', STATUS_SQL(list)))
  const readLedger = (key, database) => JSON.parse(psql(roleOf(key), database, LEDGER_SQL))
  const ledgerRows = () =>
    Object.fromEntries(ledgerKeys.map((d) => [d.key, readLedger(d.key, d.name)]))
  const releaseMigrations = (dir) =>
    Object.fromEntries(ledgerKeys.map((d) => [d.key, readMigrations(path.join(dir, d.migrations))]))

  // Read-only preflight facts: two invocations per role (catalog, then the connectable databases).
  function catalog() {
    const facts = {}
    const ledgers = {}
    for (const role of roles()) {
      const dbs = cfg.databases.filter((d) => d.role === role)
      Object.assign(facts, JSON.parse(psql(role, 'postgres', CATALOG_SQL(dbs.map((d) => d.name)))))
      const open = dbs.filter((d) => facts[d.name]?.allowConnections)
      // PGDATABASE already selects the first database; `\c` only switches to the next ones.
      const script = open.flatMap((d, i) => [
        ...(i === 0 ? [] : [connect(d.name)]),
        'select count(*) from pg_publication;',
        ...(d.migrations ? [LEDGER_SQL] : []),
      ])
      const lines = open.reduce((n, d) => n + (d.migrations ? 2 : 1), 0)
      const out = open.length > 0 ? psqlLines(role, open[0].name, script.join('\n'), lines) : []
      for (const d of open) {
        facts[d.name].publications = Number(out.shift())
        if (d.migrations) {
          ledgers[d.key] = JSON.parse(out.shift())
        }
      }
    }
    return { facts, ledgers }
  }

  // Renders the revision's Compose files; the result is stored under the release id, because two
  // releases of one revision differ by their env (round 5, F4a).
  const render = (rev, id = rev, version = rev) => {
    const out = docker([...composeSource(rev), 'config', '--format', 'json'], {
      env: { VERSION: version },
    }).stdout
    ensureDir(path.dirname(configPath(id)), 0o700)
    atomicWrite(configPath(id), out)
    return JSON.parse(out)
  }
  const imageId = (ref) =>
    docker(['image', 'inspect', '--format', '{{.Id}}', ref], { allowFail: true }).stdout.trim() ||
    null
  const inspect = (nameOrId) => {
    const r = docker(['inspect', nameOrId], { allowFail: true })
    if (r.status !== 0) {
      return null
    }
    const [c] = JSON.parse(r.stdout)
    const bindings = Object.entries(c.HostConfig.PortBindings ?? {}).flatMap(([port, list]) =>
      (list ?? []).map((b) => binding(b.HostIp, b.HostPort, port))
    )
    return {
      id: c.Id,
      name: c.Name.replace(/^\//, ''),
      created: c.Created,
      project: c.Config.Labels?.['com.docker.compose.project'] ?? null,
      service: c.Config.Labels?.['com.docker.compose.service'] ?? null,
      labels: c.Config.Labels ?? {},
      imageId: c.Image,
      running: c.State.Running,
      mounts: c.Mounts.map((m) => ({
        type: m.Type,
        name: m.Name,
        source: m.Source,
        target: m.Destination,
        rw: m.RW,
      })),
      env: c.Config.Env ?? [],
      networks: Object.keys(c.NetworkSettings.Networks ?? {}),
      portBindings: bindings,
      healthcheck: c.Config.Healthcheck?.Test ?? null,
    }
  }
  const serviceContainers = (rendered) =>
    Object.values(rendered.services)
      .map((s) => inspect(s.container_name))
      .filter(Boolean)
  // Round 6, S2b: image tags are global to the daemon. A tag is created or confirmed, never moved.
  const tagImage = (id, tag) => {
    const existing = imageId(tag)
    if (existing && existing !== id) {
      throw new Error(`tag ${tag} already points at another image; refusing to move it`)
    }
    if (!existing) {
      docker(['tag', id, tag])
    }
  }
  const loadConfig = (rev) => JSON.parse(fs.readFileSync(record(rev).config, 'utf8'))
  // Running agents refuse a deploy; stopped ones of a discarded release are removed too.
  const agentInventory = ({ all = false } = {}) =>
    lines(docker(['ps', ...(all ? ['-a'] : []), '-q', '--filter', `network=${cfg.agentNetwork}`]))
      .map(inspect)
      .filter((c) => c && isProductAgent(c))
  const removeContainers = (ids, what) => {
    if (ids.length === 0) {
      return
    }
    docker(['rm', '-f', '-v', ...ids])
    const left = ids.map(inspect).filter(Boolean)
    if (left.length > 0) {
      throw new Error(`${what} still present after removal: ${left.map((c) => c.name).join(', ')}`)
    }
  }
  const tagIdsFor = (records) =>
    Object.fromEntries(
      Object.values(records)
        .flatMap((r) => Object.values(r.refs ?? {}))
        .map((ref) => [ref, imageId(ref)])
    )
  const bindProbes = (rendered, dir) =>
    Object.values(rendered.services)
      .flatMap((s) => (s.volumes ?? []).filter((v) => v.type === 'bind'))
      .map(({ source }) => {
        if (source.includes(',')) {
          return { source, ok: false }
        }
        const mount = ['--mount', `type=bind,src=${source},dst=/probe,readonly`]
        const args = [
          ...words('run --rm --pull never --network none'),
          ...helperArgs('probe'),
          ...mount,
        ]
        const kind = source.startsWith(`${dir}/`) ? '-f' : '-e'
        return {
          source,
          ok:
            docker([...args, cfg.runnerImage, 'test', kind, '/probe'], { allowFail: true })
              .status === 0,
        }
      })
  const volumeHelper = (purpose, mounts, ...cmd) =>
    docker([
      ...words('run --rm --pull never --network none'),
      ...helperArgs(purpose),
      ...mounts,
      cfg.runnerImage,
      ...cmd,
    ])
  const ensureSha = (file, want) => {
    if (sha256(fs.readFileSync(file)) !== want) {
      throw new Error(`snapshot file ${path.basename(file)} does not match its recorded sha256`)
    }
  }
  const syncedArchive = (tar) => {
    fsyncPath(tar)
    fsyncPath(path.dirname(tar))
    return { sha256: sha256(fs.readFileSync(tar)) }
  }

  function switchFrontend(sourceDir) {
    const plan = planFrontendSwitch(
      listFiles(sourceDir),
      new Map(listFiles(cfg.webRoot).map((f) => [f.path, f.sha256]))
    )
    const st = fs.statSync(cfg.webRoot)
    const uid = Number(cfg.env.WEBROOT_OWNER_UID || st.uid)
    const gid = Number(cfg.env.WEBROOT_OWNER_GID || st.gid)
    const dirs = new Set()
    for (const rel of plan) {
      const dst = path.join(cfg.webRoot, rel)
      const tmp = `${dst}.forge-tmp-${attemptId()}`
      ensureDir(path.dirname(dst))
      fs.copyFileSync(path.join(sourceDir, rel), tmp)
      fs.chmodSync(tmp, 0o644)
      fs.chownSync(tmp, uid, gid)
      fsyncPath(tmp)
      fs.renameSync(tmp, dst)
      dirs.add(path.dirname(dst))
    }
    dirs.forEach(fsyncPath)
    log(`frontend: ${plan.length} file(s) switched, index.html last`)
  }

  async function backendFacts(rev, ledgers = ledgerRows()) {
    const base = (port) => `http://${cfg.host}:${port}`
    const containers = serviceContainers(loadConfig(rev))
    const temporal = containers.find((c) => c.service === 'temporal-ext')
    const probe = words('temporal operator cluster health --address temporal-internal:7233')
    const health = temporal ? docker(['exec', temporal.id, ...probe], { allowFail: true }) : null
    return {
      apiHealth: await fetchJson(`${base(cfg.ports.api)}/health`),
      apiReady: await fetchJson(`${base(cfg.ports.api)}/api/health`),
      orchHealth: await fetchJson(`${base(cfg.ports.orchestrator)}/health`),
      orchReady: await fetchJson(`${base(cfg.ports.orchestrator)}/health/ready`),
      natsHealth: await fetchJson(`${base(cfg.ports.natsMonitor)}/healthz`),
      jsz: await fetchJson(
        `${base(cfg.ports.natsMonitor)}/jsz?accounts=true&streams=true&consumers=true`
      ),
      temporalServing: health?.status === 0 && /SERVING/.test(health.stdout),
      runningImages: Object.fromEntries(
        containers.filter((c) => c.running).map((c) => [c.service, c.imageId])
      ),
      ledgers,
    }
  }

  async function retry(label, attempts, check) {
    let findings = []
    for (let i = 0; i < attempts; i += 1) {
      findings = await check()
      if (findings.length === 0) {
        log(`${label}: all checks passed`)
        return {}
      }
      await sleep(3000)
    }
    throw new Error(`${label} failed: ${findings.join('; ')}`)
  }

  // Forward deploys are held to the target's sources; rollbacks to what the snapshot recorded.
  // Round 6, S2d: the image ids come from the release record (taken at preflight, or from the
  // running containers for the baseline), never from a fresh tag lookup: a tag is global.
  const expectations = (s) => {
    const base = { images: record(s.rev).images, requireRedis: cfg.requireRedis }
    if (s.against === 'snapshot') {
      return { ...current.attempt.snapshot.expect, ...base }
    }
    const { streams, resultConsumer } = cfg.contract
    const ledgers = releaseMigrations(record(s.rev).releaseDir)
    return { ...base, endpoints: null, streams, consumers: [resultConsumer], ledgers }
  }

  const ops = {
    stop_project: () => {
      const ids = lines(
        docker(['ps', '-q', '--filter', `label=com.docker.compose.project=${cfg.project}`])
      )
      if (ids.length > 0) {
        docker(['stop', '--time', '40', ...ids])
      }
    },
    check_no_agents: () => {
      const agents = agentInventory()
      if (agents.length > 0) {
        throw new Error(
          `agent containers started after preflight: ${agents.map((c) => c.name).join(', ')}`
        )
      }
    },
    remove_new_agents: (s) =>
      removeContainers(
        newAgents(agentInventory({ all: true }), s.since).map((c) => c.id),
        'agent containers'
      ),
    disable_connections: (s) =>
      sqlDo(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.name)} WITH ALLOW_CONNECTIONS false`),
    enable_connections: (s) =>
      sqlDo(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.name)} WITH ALLOW_CONNECTIONS true`),
    terminate_sessions: (s) =>
      psql(
        roleOf(s.db),
        'postgres',
        `select count(pg_terminate_backend(pid)) from pg_stat_activity where datname = ${lit(s.name)} and usename = current_user and pid <> pg_backend_pid()`
      ),
    // The copy and the read of its OID are one journaled step, so they share one invocation.
    create_snapshot: (s) => {
      const script = `CREATE DATABASE ${q(s.snapshot)} TEMPLATE ${q(s.name)};\n${STATUS_SQL([s.snapshot])}`
      return { oid: JSON.parse(psql(roleOf(s.db), 'postgres', script))[s.snapshot].oid }
    },
    verify_snapshot: (s) => {
      const ledger = ledgerKeys.some((d) => d.key === s.db)
      const ledgers = ledger ? [connect(s.snapshot), LEDGER_SQL, connect(s.name), LEDGER_SQL] : []
      const [status, copy, source] = psqlLines(
        roleOf(s.db),
        'postgres',
        [STATUS_SQL([s.snapshot]), ...ledgers].join('\n')
      )
      if (!JSON.parse(status)[s.snapshot]) {
        throw new Error(`snapshot ${s.snapshot} does not exist`)
      }
      if (ledger && copy !== source) {
        throw new Error(`snapshot ${s.snapshot} ledger differs from ${s.name}`)
      }
    },
    archive_volume: (s) => {
      ensureDir(path.dirname(s.tar), 0o700)
      const mounts = ['-v', `${s.volume}:/v:ro`, '-v', `${path.dirname(s.tar)}:/out`]
      volumeHelper('tar', mounts, ...words('tar -C /v -cf'), `/out/${path.basename(s.tar)}`, '.')
      return syncedArchive(s.tar)
    },
    archive_webroot: (s) => {
      ensureDir(path.dirname(s.tar), 0o700)
      run('tar', ['-C', cfg.webRoot, '-cf', s.tar, '.'])
      return syncedArchive(s.tar)
    },
    migrate: (s) =>
      docker([
        ...composeRendered(record(s.rev).config),
        ...['run', '--rm', '--no-deps', '--name', helperName('migrate')],
        ...['--label', `${HELPER_LABEL}=${cfg.project}`, 'agentforge-server', '--migrate-only'],
      ]),
    start: (s) =>
      docker([
        ...composeRendered(record(s.rev).config),
        ...words('up -d --no-build --pull never --wait --wait-timeout 300'),
      ]),
    verify_backend: (s) => {
      if (s.simulateFailure) {
        throw new Error('SIMULATE_VERIFY_FAILURE is set: forcing the rollback path')
      }
      const expect = expectations(s)
      return retry('backend verification', 20, async () =>
        backendFindings(await backendFacts(s.rev), expect)
      )
    },
    switch_frontend: (s) => switchFrontend(record(s.rev).frontendDir),
    verify_frontend: (s) =>
      retry('frontend verification', 5, async () => {
        const res = cfg.appUrl ? await fetchJson(`${cfg.appUrl.replace(/\/$/, '')}/`) : null
        const served =
          res && !res.error
            ? { sha: res.status === 200 ? sha256(res.bytes) : `HTTP ${res.status}` }
            : res
        const diskSha = sha256(fs.readFileSync(path.join(cfg.webRoot, 'index.html')))
        return frontendFindings({ diskSha, served }, record(s.rev).indexSha256)
      }),
    drop_database: (s) => sqlDo(roleOf(s.db), 'postgres', `DROP DATABASE IF EXISTS ${q(s.name)}`),
    rename: (s) =>
      sqlDo(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.from)} RENAME TO ${q(s.to)}`),
    restore_volume: (s) => {
      ensureSha(s.tar, s.sha256)
      // `sync` flushes the extracted files before the journal records the restore as done.
      const script = 'find /v -mindepth 1 -delete && tar -C /v -xpf "$1" && sync'
      const mounts = ['-v', `${s.volume}:/v`, '-v', `${path.dirname(s.tar)}:/in:ro`]
      volumeHelper('untar', mounts, 'sh', '-c', script, 'sh', `/in/${path.basename(s.tar)}`)
    },
    restore_webroot: (s) => {
      ensureSha(s.tar, s.sha256)
      const dir = fs.mkdtempSync(
        path.join(process.env.TMPDIR ?? cfg.releaseRoot, 'webroot-restore-')
      )
      try {
        run('tar', ['-C', dir, '-xf', s.tar])
        switchFrontend(dir)
      } finally {
        fs.rmSync(dir, { recursive: true, force: true })
      }
    },
    prune: () => prune(),
  }

  function prune() {
    const items = []
    for (const role of roles()) {
      const sql =
        "select coalesce(jsonb_agg(datname), '[]') from pg_database d join pg_roles r on r.oid = d.datdba where r.rolname = current_user;"
      JSON.parse(psql(role, 'postgres', sql)).forEach((name) =>
        items.push({ kind: 'database', name, role })
      )
    }
    const dirs = (root, kind) =>
      fs.existsSync(root)
        ? fs.readdirSync(root).map((name) => ({ kind, name, path: path.join(root, name) }))
        : []
    items.push(...dirs(path.join(cfg.privateRoot, 'snapshots'), 'snapshotDir'))
    items.push(...dirs(path.join(cfg.privateRoot, 'inputs'), 'inputs'))
    for (const sub of ['releases', 'frontend']) {
      items.push(...dirs(path.join(cfg.releaseRoot, sub), 'releaseDir'))
    }
    // Rendered configs carry secrets; drop them with their release.
    dirs(path.join(cfg.privateRoot, 'configs'), 'releaseDir').forEach((i) =>
      items.push({ ...i, name: i.name.replace(/\.json$/, '') })
    )
    const sources = cfg.databases.map((d) => d.name)
    for (const item of selectPrunable(items, current, current.attempt?.attemptId, sources)) {
      if (item.kind === 'database') {
        sqlDo(item.role, 'postgres', `DROP DATABASE IF EXISTS ${q(item.name)}`)
      } else {
        fs.rmSync(item.path, { recursive: true, force: true })
      }
      log(`pruned ${item.kind} ${item.name}`)
    }
  }

  async function preflight(st, targetId) {
    current = st
    const rev = cfg.target
    const rendered = render(rev, targetId)
    const refs = Object.fromEntries(
      Object.entries(rendered.services).map(([svc, s]) => [svc, s.image])
    )
    const images = Object.fromEntries(Object.entries(refs).map(([svc, ref]) => [svc, imageId(ref)]))
    const indexFile = path.join(cfg.releaseRoot, 'frontend', rev, 'index.html')
    const frontendIndexSha = fs.existsSync(indexFile) ? sha256(fs.readFileSync(indexFile)) : null
    const free = (dir) => {
      const s = fs.statfsSync(dir)
      return s.bavail * s.bsize
    }
    const prev = record(st.current)
    let prevConfig = null
    try {
      prevConfig = JSON.parse(fs.readFileSync(prev.config, 'utf8'))
    } catch {
      prevConfig = null
    }
    const present = (id) => docker(['image', 'inspect', id], { allowFail: true }).status === 0
    const { facts: databases, ledgers } = catalog()
    const facts = await backendFacts(st.current, ledgers)
    const captured = captureExpectations(facts, cfg.requireRedis)
    const targetMigrations = releaseMigrations(releaseDir(rev))
    const notes = Object.entries(targetMigrations).map(([key, files]) => {
      const applied = new Set(facts.ledgers[key].map((r) => r.version))
      const pending = files.filter((m) => !applied.has(m.version)).map((m) => m.version)
      return `${key}: new migration versions: ${pending.join(', ') || 'none'}`
    })
    return {
      envVersion: cfg.env.VERSION,
      rendered,
      images,
      bindProbes: bindProbes(rendered, releaseDir(rev)),
      freeBytes: { private: free(cfg.privateRoot), release: free(cfg.releaseRoot) },
      frontendIndexSha,
      ledgers: facts.ledgers,
      targetMigrations,
      notes,
      containers: serviceContainers(rendered),
      databases,
      agentContainers: agentInventory(),
      volumes: cfg.volumes.map((key) => rendered.volumes?.[key]?.name ?? `${cfg.project}_${key}`),
      expect: captured.expect,
      captureProblems: captured.problems,
      rollback: {
        configReadable: Boolean(prevConfig),
        releaseDirExists: fs.existsSync(prev.releaseDir),
        bindProbes: prevConfig ? bindProbes(prevConfig, prev.releaseDir) : [],
        missingImages: Object.values(prev.images).filter((id) => !present(id)),
      },
      tagIds: tagIdsFor(st.records),
      record: {
        rev: targetId,
        revision: rev,
        config: configPath(targetId),
        refs,
        images,
        indexSha256: frontendIndexSha,
        frontendDir: path.join(cfg.releaseRoot, 'frontend', rev),
        releaseDir: releaseDir(rev),
      },
    }
  }

  // D5 + A11: record the hand-deployed release so the first deploy can roll back to it.
  function baseline() {
    const rev = 'baseline'
    // Round 6, S2a: the baseline tag is scoped by project, so a rehearsal on the same host can
    // never replace the images the live stack's baseline record points at.
    const version = `baseline-${cfg.project}`
    const rendered = render(rev, rev, version)
    const containers = serviceContainers(rendered)
    for (const [svc, repo] of Object.entries({
      'agentforge-server': 'agentforge-server',
      orchestrator: 'agentforge-orchestrator',
    })) {
      const c = containers.find((x) => x.service === svc)
      if (!c) {
        return { findings: [`no running container for ${svc}`] }
      }
      tagImage(c.imageId, `${repo}:${version}`)
    }
    const refs = Object.fromEntries(
      Object.entries(rendered.services).map(([svc, s]) => [svc, s.image])
    )
    const imageIds = Object.fromEntries(Object.values(refs).map((ref) => [ref, imageId(ref)]))
    // Only regular files (daemon-side `test -f`) inside the release directory may be relocated.
    const releaseFiles = new Set(
      bindProbes(rendered, releaseDir(rev))
        .filter((b) => b.ok && b.source.startsWith(`${releaseDir(rev)}/`))
        .map((b) => b.source)
    )
    const findings = compareBaseline(rendered, containers, imageIds, releaseFiles)
    const images = Object.fromEntries(containers.map((c) => [c.service, c.imageId]))
    const indexSha256 = sha256(fs.readFileSync(path.join(cfg.webRoot, 'index.html')))
    const rec = {
      rev,
      config: configPath(rev),
      refs,
      images,
      indexSha256,
      frontendDir: null,
      releaseDir: releaseDir(rev),
    }
    return { findings, record: rec }
  }

  function readState() {
    // Only "does not exist" means no state; an inaccessible file is unreadable (round 5, F1).
    try {
      fs.statSync(statePath)
    } catch (err) {
      if (err.code === 'ENOENT') {
        current = null
        return null
      }
      throw new StateUnreadable(`state.json cannot be accessed (${err.code})`)
    }
    try {
      current = JSON.parse(fs.readFileSync(statePath, 'utf8'))
    } catch {
      // The parser's message can quote file content; say only what is wrong.
      throw new StateUnreadable('state.json is not valid JSON (truncated or edited)')
    }
    return validateState(current)
  }

  return {
    readState,
    writeState: (next) => {
      atomicWrite(statePath, JSON.stringify(next, null, 2))
      current = next
    },
    run: async (step, st) => {
      current = st
      if (JOURNAL_ONLY.has(step.op)) {
        return {}
      }
      if (!ops[step.op]) {
        throw new Error(`unknown step ${step.op}`)
      }
      return (await ops[step.op](step)) ?? {}
    },
    removeHelpers: () =>
      removeContainers(
        lines(docker(['ps', '-aq', '--filter', `label=${HELPER_LABEL}=${cfg.project}`])),
        'helper containers'
      ),
    observe: (attempt) => {
      const databases = {}
      for (const role of roles()) {
        const mine = attempt.snapshot.databases.filter((d) => roleOf(d.key) === role)
        Object.assign(
          databases,
          dbStatus(
            role,
            mine.flatMap((d) => [d.source, d.snapshot, d.failed])
          )
        )
      }
      return { databases }
    },
    startSql: () => {
      stats.helperStarts += 1
      const args = [
        ...words('run -d --pull never --network'),
        cfg.externalNetwork,
        ...helperArgs('sql'),
      ]
      const id = docker([...args, cfg.pgClientImage, 'sleep', 'infinity']).stdout.trim()
      sqlHelper = { id, name: helperName('sql') }
    },
    stopSql: () => {
      if (sqlHelper) {
        removeContainers([sqlHelper.id], `SQL helper ${sqlHelper.name}`)
        sqlHelper = null
      }
    },
    stats: () => ({ ...stats }),
    activateStaged,
    inputsFacts,
    readStaged,
    preflight,
    databaseFacts: () => catalog().facts,
    ledgerRows,
    baseline,
    // Finding 6: a recorded release keeps its images; a moved tag is an error, not a rebuild.
    build: (rev) => {
      const st = readState() ?? emptyState()
      const drift = () => {
        const findings = tagDriftFindings(st.records, tagIdsFor(st.records))
        if (findings.length > 0) {
          throw new Error(findings.join('; '))
        }
      }
      drift()
      if (Object.values(st.records).some((r) => r.revision === rev)) {
        log(`${rev} already has a release record; build skipped so its images stay pinned`)
        return
      }
      // Round 6, S2c: revision tags are shared by every deployment on this host by design. An
      // existing tag is adopted as it is (preflight records its id); only missing ones are built.
      const services = {
        'agentforge-server': 'agentforge-server',
        orchestrator: 'agentforge-orchestrator',
      }
      const missing = Object.entries(services)
        .filter(([, repo]) => !imageId(`${repo}:${rev}`))
        .map(([svc]) => svc)
      if (missing.length > 0) {
        docker([...composeSource(rev), 'build', ...missing], {
          env: { VERSION: rev },
          inherit: true,
        })
      }
      const tag = `agentforge-frontend:${rev}`
      if (!imageId(tag)) {
        const args = [
          '-f',
          path.join(releaseDir(rev), 'docker/Dockerfile'),
          '--build-arg',
          `APP_VERSION=${rev}`,
        ]
        docker([...words('build --target production'), ...args, '-t', tag, releaseDir(rev)], {
          inherit: true,
        })
      }
      const adopted = Object.keys(services).filter((svc) => !missing.includes(svc))
      if (adopted.length > 0) {
        log(
          `adopted existing images for ${adopted.join(', ')} at ${rev} (tags are shared on this host)`
        )
      }
      const out = path.join(cfg.releaseRoot, 'frontend', rev)
      const id = docker(['create', ...helperArgs('frontend'), tag]).stdout.trim()
      try {
        fs.rmSync(out, { recursive: true, force: true })
        ensureDir(out)
        docker(['cp', `${id}:/app/dist/.`, out])
        fsyncTree(out)
      } finally {
        removeContainers([id], 'frontend extraction helper')
      }
      drift()
      log(`built images and frontend for ${rev}`)
    },
  }
}
