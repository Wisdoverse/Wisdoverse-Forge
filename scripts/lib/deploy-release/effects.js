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
  compareBaseline,
  compareLedger,
  frontendFindings,
  parseMigrations,
  redact,
  sha256,
} from './checks.js'
import { planFrontendSwitch, selectPrunable } from './plan.js'

const MAX_BUFFER = 256 * 1024 * 1024
const PASS_THROUGH = ['PATH', 'HOME', 'TMPDIR', 'DOCKER_HOST', 'DOCKER_CONFIG']

// Static argument lists are written as one string; dynamic values stay separate array items.
const words = (text) => text.split(' ')
const baseEnv = () =>
  Object.fromEntries(PASS_THROUGH.filter((k) => process.env[k]).map((k) => [k, process.env[k]]))

export function atomicWrite(file, data) {
  const tmp = `${file}.tmp`
  const fd = fs.openSync(tmp, 'w', 0o600)
  try {
    fs.writeSync(fd, data)
    fs.fsyncSync(fd)
  } finally {
    fs.closeSync(fd)
  }
  fs.renameSync(tmp, file)
  const dir = fs.openSync(path.dirname(file), 'r')
  try {
    fs.fsyncSync(dir)
  } finally {
    fs.closeSync(dir)
  }
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
  const run = (cmd, args, { env = {}, allowFail = false, inherit = false } = {}) => {
    const r = spawnSync(cmd, args, {
      env: { ...baseEnv(), ...env },
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
  const releaseDir = (rev) => path.join(cfg.releaseRoot, 'releases', rev)
  const configPath = (rev) => path.join(cfg.privateRoot, 'configs', `${rev}.json`)
  const statePath = path.join(cfg.privateRoot, 'state.json')
  const project = [...words('compose --profile external --project-name'), cfg.project]
  const composeSource = (rev) => [
    ...project,
    ...['--env-file', cfg.envFile, '-f', path.join(releaseDir(rev), 'docker/compose.yml')],
    ...['-f', path.join(releaseDir(rev), 'docker/compose.external.yml')],
  ]
  const composeRendered = (file) => [...project, '-f', file]
  const record = (rev) => {
    const r = state().records[rev]
    if (!r) {
      throw new Error(`no release record for ${rev}`)
    }
    return r
  }
  let current = null
  const state = () => current

  // Passwords reach psql only through the environment of the sibling container (never argv).
  const psql = (roleKey, database, sql) => {
    const role = cfg.roles[roleKey]
    const env = {
      PGHOST: role.host,
      PGPORT: role.port,
      PGUSER: role.user,
      PGPASSWORD: role.password,
    }
    Object.assign(env, { PGDATABASE: database }, role.sslmode ? { PGSSLMODE: role.sslmode } : {})
    const passEnv = Object.keys(env).flatMap((k) => ['-e', k])
    const args = [...words('run --rm --pull never --network'), cfg.externalNetwork, ...passEnv]
    const psqlArgs = [cfg.pgClientImage, ...words('psql -X -q -v ON_ERROR_STOP=1 -At -c'), sql]
    return docker([...args, ...psqlArgs], { env }).stdout.trim()
  }
  const q = (name) => `"${name}"`
  const lit = (names) => `array[${names.map((n) => `'${n}'`).join(',')}]::text[]`
  const roleOf = (key) => cfg.databases.find((d) => d.key === key).role
  const dbStatus = (roleKey, names) =>
    JSON.parse(
      psql(
        roleKey,
        'postgres',
        `select coalesce(json_object_agg(datname, json_build_object('oid', oid::bigint, 'allowConnections', datallowconn)), '{}') from pg_database where datname = any(${lit(names)})`
      )
    )
  const readLedger = (key, database) =>
    JSON.parse(
      psql(
        roleOf(key),
        database,
        "select coalesce(json_agg(json_build_object('version', version, 'checksum', encode(checksum, 'hex'), 'success', success) order by version), '[]') from _sqlx_migrations"
      )
    )
  const ledgerKeys = cfg.databases.filter((d) => d.migrations)

  const render = (rev) => {
    const out = docker([...composeSource(rev), 'config', '--format', 'json'], {
      env: { VERSION: rev },
    }).stdout
    fs.mkdirSync(path.dirname(configPath(rev)), { recursive: true, mode: 0o700 })
    atomicWrite(configPath(rev), out)
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
    return {
      id: c.Id,
      name: c.Name.replace(/^\//, ''),
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
      })),
      env: c.Config.Env ?? [],
      networks: Object.keys(c.NetworkSettings.Networks ?? {}),
      ports: Object.keys(c.HostConfig.PortBindings ?? {}),
      healthcheck: c.Config.Healthcheck?.Test ?? null,
    }
  }
  const serviceContainers = (rendered) =>
    Object.values(rendered.services)
      .map((s) => inspect(s.container_name))
      .filter(Boolean)
  const projectIds = () =>
    docker(['ps', '-q', '--filter', `label=com.docker.compose.project=${cfg.project}`])
      .stdout.split(/\s+/)
      .filter(Boolean)
  const loadConfig = (rev) => JSON.parse(fs.readFileSync(record(rev).config, 'utf8'))
  const volumeNames = (rendered) =>
    cfg.volumes.map((key) => rendered.volumes?.[key]?.name ?? `${cfg.project}_${key}`)
  const helper = (mounts, ...cmd) =>
    docker([...words('run --rm --pull never --network none'), ...mounts, cfg.runnerImage, ...cmd])
  const ensureSha = (file, want) => {
    if (sha256(fs.readFileSync(file)) !== want) {
      throw new Error(`snapshot file ${path.basename(file)} does not match its recorded sha256`)
    }
  }

  function switchFrontend(sourceDir) {
    const plan = planFrontendSwitch(
      listFiles(sourceDir),
      new Map(listFiles(cfg.webRoot).map((f) => [f.path, f.sha256]))
    )
    const st = fs.statSync(cfg.webRoot)
    const uid = Number(cfg.env.WEBROOT_OWNER_UID || st.uid)
    const gid = Number(cfg.env.WEBROOT_OWNER_GID || st.gid)
    for (const rel of plan) {
      const dst = path.join(cfg.webRoot, rel)
      const tmp = `${dst}.forge-tmp-${cfg.attemptId}`
      fs.mkdirSync(path.dirname(dst), { recursive: true })
      fs.copyFileSync(path.join(sourceDir, rel), tmp)
      fs.chmodSync(tmp, 0o644)
      fs.chownSync(tmp, uid, gid)
      fs.renameSync(tmp, dst)
    }
    log(`frontend: ${plan.length} file(s) switched, index.html last`)
  }

  async function backendFacts(rev) {
    const rec = record(rev)
    const base = (port) => `http://${cfg.host}:${port}`
    const containers = serviceContainers(loadConfig(rev))
    const temporal = containers.find((c) => c.service === 'temporal-ext')
    const probe = words('temporal operator cluster health --address temporal-internal:7233')
    const health = temporal ? docker(['exec', temporal.id, ...probe], { allowFail: true }) : null
    const ledgers = Object.fromEntries(
      ledgerKeys.map((d) => [
        d.key,
        compareLedger(
          readLedger(d.key, d.name),
          readMigrations(path.join(rec.releaseDir, d.migrations))
        ),
      ])
    )
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

  const ops = {
    stop_project: () => {
      const ids = projectIds()
      if (ids.length > 0) {
        docker(['stop', '--time', '40', ...ids])
      }
    },
    disable_connections: (s) =>
      psql(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.name)} WITH ALLOW_CONNECTIONS false`),
    enable_connections: (s) =>
      psql(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.name)} WITH ALLOW_CONNECTIONS true`),
    terminate_sessions: (s) =>
      psql(
        roleOf(s.db),
        'postgres',
        `select count(pg_terminate_backend(pid)) from pg_stat_activity where datname = '${s.name}' and usename = current_user and pid <> pg_backend_pid()`
      ),
    create_snapshot: (s) => {
      psql(roleOf(s.db), 'postgres', `CREATE DATABASE ${q(s.snapshot)} TEMPLATE ${q(s.name)}`)
      return { oid: dbStatus(roleOf(s.db), [s.snapshot])[s.snapshot].oid }
    },
    verify_snapshot: (s) => {
      if (!dbStatus(roleOf(s.db), [s.snapshot])[s.snapshot]) {
        throw new Error(`snapshot ${s.snapshot} does not exist`)
      }
      if (
        ledgerKeys.some((d) => d.key === s.db) &&
        JSON.stringify(readLedger(s.db, s.snapshot)) !== JSON.stringify(readLedger(s.db, s.name))
      ) {
        throw new Error(`snapshot ${s.snapshot} ledger differs from ${s.name}`)
      }
    },
    archive_volume: (s) => {
      fs.mkdirSync(path.dirname(s.tar), { recursive: true, mode: 0o700 })
      const mounts = ['-v', `${s.volume}:/v:ro`, '-v', `${path.dirname(s.tar)}:/out`]
      helper(mounts, ...words('tar -C /v -cf'), `/out/${path.basename(s.tar)}`, '.')
      return { sha256: sha256(fs.readFileSync(s.tar)) }
    },
    archive_webroot: (s) => {
      fs.mkdirSync(path.dirname(s.tar), { recursive: true, mode: 0o700 })
      run('tar', ['-C', cfg.webRoot, '-cf', s.tar, '.'])
      return { sha256: sha256(fs.readFileSync(s.tar)) }
    },
    migrate: (s) =>
      docker([
        ...composeRendered(record(s.rev).config),
        ...['run', '--rm', '--no-deps', '--name', s.name, 'agentforge-server', '--migrate-only'],
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
      const expect = {
        images: record(s.rev).images,
        requireRedis: cfg.requireRedis,
        contract: cfg.contract,
      }
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
    remove_container: (s) => docker(['rm', '-f', s.name], { allowFail: true }),
    drop_database: (s) => psql(roleOf(s.db), 'postgres', `DROP DATABASE IF EXISTS ${q(s.name)}`),
    rename: (s) =>
      psql(roleOf(s.db), 'postgres', `ALTER DATABASE ${q(s.from)} RENAME TO ${q(s.to)}`),
    restore_volume: (s) => {
      ensureSha(s.tar, s.sha256)
      const script = 'find /v -mindepth 1 -delete && tar -C /v -xpf "$1"'
      const mounts = ['-v', `${s.volume}:/v`, '-v', `${path.dirname(s.tar)}:/in:ro`]
      helper(mounts, 'sh', '-c', script, 'sh', `/in/${path.basename(s.tar)}`)
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
    for (const role of new Set(cfg.databases.map((d) => d.role))) {
      const names = JSON.parse(
        psql(
          role,
          'postgres',
          "select coalesce(json_agg(datname), '[]') from pg_database d join pg_roles r on r.oid = d.datdba where r.rolname = current_user and datname ~ '__(snap|failed)_[0-9a-z]+$'"
        )
      )
      names.forEach((name) => items.push({ kind: 'database', name, role }))
    }
    const dirs = (root, kind) =>
      fs.existsSync(root)
        ? fs.readdirSync(root).map((name) => ({ kind, name, path: path.join(root, name) }))
        : []
    items.push(...dirs(path.join(cfg.privateRoot, 'snapshots'), 'snapshotDir'))
    for (const sub of ['releases', 'frontend']) {
      items.push(...dirs(path.join(cfg.releaseRoot, sub), 'releaseDir'))
    }
    // Rendered configs carry secrets; drop them with their release.
    dirs(path.join(cfg.privateRoot, 'configs'), 'releaseDir').forEach((i) =>
      items.push({ ...i, name: i.name.replace(/\.json$/, '') })
    )
    for (const item of selectPrunable(items, state(), cfg.attemptId)) {
      if (item.kind === 'database') {
        psql(item.role, 'postgres', `DROP DATABASE IF EXISTS ${q(item.name)}`)
      } else {
        fs.rmSync(item.path, { recursive: true, force: true })
      }
      log(`pruned ${item.kind} ${item.name}`)
    }
  }

  return {
    readState: () => {
      current = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')) : null
      if (current && current.schema !== 1) {
        throw new Error(`state file schema ${current.schema} is not supported by this script`)
      }
      return current
    },
    writeState: (next) => {
      atomicWrite(statePath, JSON.stringify(next, null, 2))
      current = next
    },
    run: async (step, st) => {
      current = st
      return (await ops[step.op]?.(step)) ?? {}
    },
    observe: (attempt) => {
      const databases = {}
      for (const d of attempt.snapshot.databases) {
        Object.assign(databases, dbStatus(roleOf(d.key), [d.source, d.snapshot, d.failed]))
      }
      return { databases, migrationContainer: Boolean(inspect(attempt.migrationContainer)) }
    },
    preflight: (st) => preflight(st),
    build: (rev) => {
      docker([...composeSource(rev), 'build', 'agentforge-server', 'orchestrator'], {
        env: { VERSION: rev },
        inherit: true,
      })
      const tag = `agentforge-frontend:${rev}`
      const dockerfile = path.join(releaseDir(rev), 'docker/Dockerfile')
      const args = [
        '-f',
        dockerfile,
        '--build-arg',
        `APP_VERSION=${rev}`,
        '-t',
        tag,
        releaseDir(rev),
      ]
      docker([...words('build --target production'), ...args], { inherit: true })
      const out = path.join(cfg.releaseRoot, 'frontend', rev)
      const id = docker(['create', tag]).stdout.trim()
      try {
        fs.rmSync(out, { recursive: true, force: true })
        fs.mkdirSync(out, { recursive: true })
        docker(['cp', `${id}:/app/dist/.`, out])
      } finally {
        docker(['rm', id], { allowFail: true })
      }
    },
    baseline: () => baseline(),
  }

  function dbFacts() {
    const facts = {}
    for (const role of new Set(cfg.databases.map((d) => d.role))) {
      const names = cfg.databases.filter((d) => d.role === role).map((d) => d.name)
      const rows = psql(
        role,
        'postgres',
        `select coalesce(json_object_agg(d.datname, json_build_object('oid', d.oid::bigint, 'allowConnections', d.datallowconn, 'ownedByRole', r.rolname = current_user, 'ownerCreatedb', r.rolcreatedb, 'defaultAcl', d.datacl is null, 'defaultTablespace', d.dattablespace = (select oid from pg_tablespace where spcname = 'pg_default'), 'settings', (select count(*) from pg_db_role_setting s where s.setdatabase = d.oid or s.setrole = d.datdba), 'slots', (select count(*) from pg_replication_slots s where s.database = d.datname), 'subscriptions', (select count(s.subdbid) from pg_subscription s where s.subdbid = d.oid))), '{}') from pg_database d join pg_roles r on r.oid = d.datdba where d.datname = any(${lit(names)})`
      )
      Object.assign(facts, JSON.parse(rows))
    }
    for (const d of cfg.databases.filter((x) => facts[x.name])) {
      facts[d.name].publications = Number(
        psql(d.role, d.name, 'select count(*) from pg_publication')
      )
    }
    return facts
  }

  function preflight(st) {
    current = st
    const rev = cfg.target
    const rendered = render(rev)
    const images = Object.fromEntries(
      Object.entries(rendered.services).map(([svc, s]) => [svc, imageId(s.image)])
    )
    const bindProbes = Object.values(rendered.services)
      .flatMap((s) => (s.volumes ?? []).filter((v) => v.type === 'bind'))
      .map(({ source }) => {
        const kind = source.startsWith(releaseDir(rev)) ? '-f' : '-e'
        if (source.includes(',')) {
          return { source, ok: false }
        }
        const mount = ['--mount', `type=bind,src=${source},dst=/probe,readonly`]
        const args = [...words('run --rm --pull never --network none'), ...mount, cfg.runnerImage]
        const r = docker([...args, 'test', kind, '/probe'], { allowFail: true })
        return { source, ok: r.status === 0 }
      })
    const free = (dir) => {
      const s = fs.statfsSync(dir)
      return s.bavail * s.bsize
    }
    const indexFile = path.join(cfg.releaseRoot, 'frontend', rev, 'index.html')
    const frontendIndexSha = fs.existsSync(indexFile) ? sha256(fs.readFileSync(indexFile)) : null
    const ledgers = {}
    const notes = []
    for (const d of ledgerKeys) {
      ledgers[d.key] = compareLedger(
        readLedger(d.key, d.name),
        readMigrations(path.join(releaseDir(rev), d.migrations))
      )
      notes.push(`${d.key}: new migration versions: ${ledgers[d.key].pending.join(', ') || 'none'}`)
    }
    const agentIds = docker(['ps', '-q', '--filter', `network=${cfg.agentNetwork}`])
      .stdout.split(/\s+/)
      .filter(Boolean)
    return {
      envVersion: cfg.env.VERSION,
      rendered,
      images,
      bindProbes,
      freeBytes: { private: free(cfg.privateRoot), release: free(cfg.releaseRoot) },
      frontendIndexSha,
      ledgers,
      notes,
      containers: serviceContainers(rendered),
      databases: dbFacts(),
      agentContainers: agentIds.map(inspect).filter(Boolean),
      volumes: volumeNames(rendered),
      record: {
        rev,
        config: configPath(rev),
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
    const rendered = render(rev)
    const containers = serviceContainers(rendered)
    for (const [svc, repo] of [
      ['agentforge-server', 'agentforge-server'],
      ['orchestrator', 'agentforge-orchestrator'],
    ]) {
      const c = containers.find((x) => x.service === svc)
      if (!c) {
        return { findings: [`no running container for ${svc}`] }
      }
      docker(['tag', c.imageId, `${repo}:baseline`])
    }
    const imageIds = Object.fromEntries(
      Object.values(rendered.services).map((s) => [s.image, imageId(s.image)])
    )
    const findings = compareBaseline(rendered, containers, imageIds)
    const images = Object.fromEntries(containers.map((c) => [c.service, c.imageId]))
    const indexSha256 = sha256(fs.readFileSync(path.join(cfg.webRoot, 'index.html')))
    return {
      findings,
      record: {
        rev,
        config: configPath(rev),
        images,
        indexSha256,
        frontendDir: null,
        releaseDir: releaseDir(rev),
      },
    }
  }
}
