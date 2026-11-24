// Pure predicates over observed facts: env parsing, redaction, migration ledgers, preflight
// (D4.3, A5, A8), verification (D6, A6) and baseline equivalence (A11).
import { createHash } from 'node:crypto'

export const MIN_FREE_BYTES = 1024 ** 3

// Same dotenv subset as scripts/deploy.sh: KEY=VALUE, optional `export `, one level of quotes;
// the last assignment wins, which is how the appended VERSION=<rev> takes effect.
export function parseEnvFile(text) {
  const env = {}
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim().replace(/^export\s+/, '')
    if (line === '' || line.startsWith('#')) {
      continue
    }
    const eq = line.indexOf('=')
    const key = eq > 0 ? line.slice(0, eq).trim() : ''
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      throw new Error('env file has a line that is not KEY=VALUE (line content not shown)')
    }
    const value = line.slice(eq + 1)
    env[key] = /^(".*"|'.*')$/.test(value) ? value.slice(1, -1) : value
  }
  return env
}

const SENSITIVE_KEY = /PASS|SECRET|TOKEN|KEY|SEED|CREDENTIAL|PRIVATE|_URL$|DSN/i

export function secretValues(env) {
  const values = Object.entries(env)
    .filter(([key, value]) => SENSITIVE_KEY.test(key) && value.length >= 4)
    .map(([, value]) => value)
  for (const value of Object.values(env)) {
    const m = /^[a-z][a-z0-9+.-]*:\/\/[^:/@\s]*:([^@\s]+)@/i.exec(value)
    if (m) {
      values.push(m[1], decodeURIComponent(m[1]))
    }
  }
  return [...new Set(values)].sort((a, b) => b.length - a.length)
}

export function redact(text, secrets = []) {
  let out = String(text)
  for (const secret of secrets) {
    out = out.split(secret).join('***')
  }
  return out.replace(/(\/\/)[^/@\s:]*:[^/@\s]*@/g, '$1***@')
}

export const sha256 = (buf) => createHash('sha256').update(buf).digest('hex')

function noNul(value) {
  if (String(value).includes('\0')) {
    throw new Error('identifier or literal contains a NUL byte')
  }
  return String(value)
}
export const quoteIdent = (name) => `"${noNul(name).replaceAll('"', '""')}"`
export const quoteLiteral = (value) => `'${noNul(value).replaceAll("'", "''")}'`

// sqlx-core 0.8.6 `resolve_blocking`: `<VERSION>_<DESCRIPTION>.sql`, VERSION parsed as an integer,
// checksum = SHA-384 of the file contents. `.down.sql` files are never applied.
export function parseMigrations(files) {
  return files
    .filter((f) => /^[^_]+_.*\.sql$/.test(f.name) && !f.name.endsWith('.down.sql'))
    .map((f) => {
      const prefix = f.name.split('_', 1)[0]
      if (!/^-?\d+$/.test(prefix)) {
        throw new Error(`migration ${f.name} has no integer version prefix`)
      }
      return {
        version: Number(prefix),
        checksum: createHash('sha384').update(f.content).digest('hex'),
      }
    })
    .sort((a, b) => a.version - b.version)
}

export function compareLedger(applied, release) {
  const byVersion = new Map(release.map((m) => [m.version, m.checksum]))
  const appliedVersions = new Set(applied.map((r) => r.version))
  return {
    failed: applied.filter((r) => !r.success).map((r) => r.version),
    missingInRelease: applied.filter((r) => !byVersion.has(r.version)).map((r) => r.version),
    checksumMismatch: applied
      .filter(
        (r) => byVersion.has(r.version) && byVersion.get(r.version) !== r.checksum.toLowerCase()
      )
      .map((r) => r.version),
    pending: release.filter((m) => !appliedVersions.has(m.version)).map((m) => m.version),
  }
}

export function ledgerFindings(key, cmp, { strict }) {
  const out = []
  const add = (list, what) =>
    list.length > 0 && out.push(`${key} ledger: ${what}: ${list.join(', ')}`)
  add(cmp.failed, 'failed migrations recorded')
  add(cmp.missingInRelease, 'applied versions missing from this release')
  add(cmp.checksumMismatch, 'checksum differs from this release')
  if (strict) {
    add(cmp.pending, 'release migrations not applied')
  }
  return out
}

// D4.3 identity: the containers and named volumes we are about to replace belong to this project.
export function identityFindings(project, rendered, containers) {
  const out = []
  const byName = new Map(containers.map((c) => [c.name, c]))
  for (const [service, svc] of Object.entries(rendered.services)) {
    const c = byName.get(svc.container_name)
    if (!c) {
      out.push(`no running container ${svc.container_name} for service ${service}`)
      continue
    }
    if (c.project !== project) {
      out.push(`container ${c.name} belongs to compose project ${c.project}, not ${project}`)
    }
    for (const v of (svc.volumes ?? []).filter((m) => m.type === 'volume')) {
      const want = rendered.volumes?.[v.source]?.name ?? v.source
      const have = c.mounts.find((m) => m.target === v.target)
      if (have?.name !== want) {
        out.push(
          `${c.name} mounts ${have?.name ?? 'nothing'} at ${v.target}; the release expects ${want}`
        )
      }
    }
  }
  return out
}

// A5: refuse what a template copy would not carry.
export function databaseFindings(databases, facts) {
  const out = []
  for (const { name } of databases) {
    const f = facts[name]
    if (!f) {
      out.push(`database ${name} not found or not visible to its owning role`)
      continue
    }
    const bad = [
      [!f.ownedByRole, 'is not owned by the role in the env file'],
      [!f.ownerCreatedb, 'owner role lacks CREATEDB'],
      [!f.defaultAcl, 'has a non-default ACL'],
      [!f.defaultTablespace, 'is not in the default tablespace'],
      [f.settings > 0, 'has database or role settings'],
      [
        f.slots + f.subscriptions + f.publications > 0,
        'has replication slots, subscriptions or publications',
      ],
      [!f.allowConnections, 'does not allow connections (an earlier run did not finish)'],
    ]
    bad.filter(([cond]) => cond).forEach(([, why]) => out.push(`database ${name} ${why}`))
  }
  return out
}

// The names and labels the API gives the containers it creates (agent_container_control.rs,
// dev_environment.rs, clone_runtime.rs). Selection is exact: name pattern AND matching label.
const PRODUCT_CONTAINERS = [
  ['agent', 'agentforge.agent_id'],
  ['devenv', 'agentforge.dev_environment_id'],
  ['clone', 'agentforge.project_clone'],
]
const UUID = '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'

export function isProductAgent(c) {
  return PRODUCT_CONTAINERS.some(([kind, label]) => {
    const m = new RegExp(`^agentforge-${kind}-(${UUID})$`).exec(c.name)
    return Boolean(m) && c.labels[label] === m[1]
  })
}

// A8: the inventory is already scoped to the deployment's agent network by the caller.
export const agentContainerFindings = (containers) =>
  containers
    .filter(isProductAgent)
    .map(
      (c) => `agent container ${c.name} is running on the deployment's agent network; stop it first`
    )

// Containers the new release created belong to the discarded state of a rolled-back attempt.
export const newAgents = (containers, since) =>
  containers.filter((c) => isProductAgent(c) && Date.parse(c.created) > Date.parse(since))

// A recorded release must start the exact images it was verified with (finding 6).
export function tagDriftFindings(records, tagIds) {
  const out = []
  for (const rec of Object.values(records)) {
    for (const [service, ref] of Object.entries(rec.refs ?? {})) {
      if (tagIds[ref] !== rec.images[service]) {
        out.push(
          `release ${rec.rev}: tag ${ref} no longer points at the recorded image of ${service}`
        )
      }
    }
  }
  return out
}

// D4/A9: everything a rollback to the current release needs must exist before we quiesce.
export function rollbackFindings(rollback) {
  const out = []
  if (!rollback.configReadable) {
    out.push('the rendered config of the current release is missing or unreadable')
  }
  if (!rollback.releaseDirExists) {
    out.push('the release directory of the current release is missing')
  }
  rollback.bindProbes
    .filter((b) => !b.ok)
    .forEach((b) => out.push(`rollback bind source unusable for the daemon: ${b.source}`))
  rollback.missingImages.forEach((id) =>
    out.push(`image ${id} of the current release is not present`)
  )
  return out
}

export function preflightFindings(config, state, o) {
  const out = []
  if (o.envVersion !== config.target) {
    out.push('the env file VERSION is not the target revision')
  }
  for (const [service, id] of Object.entries(o.images)) {
    if (!id) {
      out.push(`image for service ${service} is not present locally (run build)`)
    }
  }
  o.bindProbes
    .filter((b) => !b.ok)
    .forEach((b) => out.push(`bind source unusable for the daemon: ${b.source}`))
  for (const [where, bytes] of Object.entries(o.freeBytes)) {
    if (bytes < MIN_FREE_BYTES) {
      out.push(`less than 1 GiB free in the ${where} root`)
    }
  }
  if (!o.frontendIndexSha) {
    out.push('frontend bytes for the target are missing (run build)')
  }
  for (const [key, rows] of Object.entries(o.ledgers)) {
    out.push(
      ...ledgerFindings(key, compareLedger(rows, o.targetMigrations[key]), { strict: false })
    )
  }
  out.push(...identityFindings(config.project, o.rendered, o.containers))
  out.push(...databaseFindings(config.databases, o.databases))
  out.push(...agentContainerFindings(o.agentContainers))
  out.push(...rollbackFindings(o.rollback))
  out.push(...tagDriftFindings(state.records, o.tagIds))
  return out
}

const deps = (b, requireRedis) =>
  b.checks?.database && b.checks?.nats && b.checks?.docker && (!requireRedis || b.checks?.redis)
const ENDPOINTS = [
  ['API /health', 'apiHealth', (b) => b.ok === true],
  ['API /api/health', 'apiReady', (b, r) => b.ok === true && b.status === 'ready' && deps(b, r)],
  ['orchestrator /health', 'orchHealth', (b) => b.workflowRuntime === 'up'],
  ['orchestrator /health/ready', 'orchReady', (b) => b.ok === true],
  ['NATS /healthz', 'natsHealth', (b) => b.status === 'ok'],
]

// Each endpoint yields two checks so a rollback can require exactly what the release answered
// before the deploy (an older build may answer an endpoint without the newer body fields).
export function endpointChecks(facts, requireRedis) {
  const checks = ENDPOINTS.flatMap(([label, key, healthy]) => {
    const res = facts[key]
    if (!res || res.error) {
      const problem = `check could not run (${res?.error ?? 'no response'})`
      return [
        { label: `${label} answers`, problem },
        { label: `${label} healthy`, problem },
      ]
    }
    return [
      { label: `${label} answers`, problem: res.status === 200 ? null : `HTTP ${res.status}` },
      {
        label: `${label} healthy`,
        problem: res.status === 200 && healthy(res.body ?? {}, requireRedis) ? null : 'unhealthy',
      },
    ]
  })
  return [
    ...checks,
    { label: 'Temporal SERVING', problem: facts.temporalServing ? null : 'not SERVING' },
  ]
}

export function observedStreams(jsz) {
  if (!jsz || jsz.error || jsz.status !== 200) {
    return null
  }
  const streams = (jsz.body?.account_details ?? []).flatMap((a) => a.stream_detail ?? [])
  return {
    streams: streams.map((s) => s.name),
    consumers: streams.flatMap((s) =>
      (s.consumer_detail ?? []).map((c) => ({ stream: s.name, name: c.name }))
    ),
  }
}

// Finding 5: what "healthy" meant for the release that runs before the deploy.
export function captureExpectations(facts, requireRedis) {
  const js = observedStreams(facts.jsz) ?? { streams: [], consumers: [] }
  return {
    checks: endpointChecks(facts, requireRedis)
      .filter((c) => !c.problem)
      .map((c) => c.label),
    streams: js.streams,
    consumers: js.consumers,
    ledgers: Object.fromEntries(
      Object.entries(facts.ledgers).map(([k, rows]) => [
        k,
        rows.map((r) => ({ version: r.version, checksum: r.checksum })),
      ])
    ),
  }
}

// expect.checks === null requires every check (forward deploy); otherwise only the listed ones.
export function backendFindings(facts, expect) {
  const required = (label) => expect.checks === null || expect.checks.includes(label)
  const out = endpointChecks(facts, expect.requireRedis)
    .filter((c) => c.problem && required(c.label))
    .map((c) => `${c.label}: ${c.problem}`)
  if (expect.streams.length + expect.consumers.length > 0) {
    const js = observedStreams(facts.jsz)
    if (!js) {
      out.push('NATS JetStream: check could not run')
    } else {
      expect.streams
        .filter((s) => !js.streams.includes(s))
        .forEach((s) => out.push(`NATS stream ${s} missing`))
      expect.consumers
        .filter((c) => !js.consumers.some((o) => o.stream === c.stream && o.name === c.name))
        .forEach((c) => out.push(`NATS consumer ${c.name} on ${c.stream} missing`))
    }
  }
  for (const [service, id] of Object.entries(expect.images)) {
    if (facts.runningImages[service] !== id) {
      out.push(`service ${service} is not running the release image`)
    }
  }
  for (const [key, want] of Object.entries(expect.ledgers)) {
    out.push(
      ...ledgerFindings(key, compareLedger(facts.ledgers[key] ?? [], want), { strict: true })
    )
  }
  return out
}

export function frontendFindings(facts, expectedSha) {
  const out = []
  if (facts.diskSha !== expectedSha) {
    out.push('web root index.html does not match the release')
  }
  if (facts.served?.error || !facts.served) {
    out.push(`served index.html: check could not run (${facts.served?.error ?? 'APP_URL not set'})`)
  } else if (facts.served.sha !== expectedSha) {
    out.push('served index.html does not match the release')
  }
  return out
}

const envNames = (env) =>
  Array.isArray(env) ? env.map((e) => e.split('=')[0]) : Object.keys(env ?? {})
const sameSet = (a, b) => a.length === b.length && a.every((x) => b.includes(x))
export const binding = (ip, port, target) => `${ip || '0.0.0.0'}:${port}->${target}`

function mountKey(rendered, releaseDir, m) {
  const mode = m.read_only ? 'ro' : 'rw'
  if (m.type === 'volume') {
    return `volume ${rendered.volumes?.[m.source]?.name ?? m.source} ${m.target} ${mode}`
  }
  // Files bind-mounted from the release directory are relocated on purpose: the baseline is
  // re-materialized from `git archive`, so only the file name must match the running source.
  const inRelease = m.source.startsWith(`${releaseDir}/`)
  return `bind ${inRelease ? `*/${m.source.split('/').pop()}` : m.source} ${m.target} ${mode}`
}

function containerMountKey(m, relocatedTargets) {
  const mode = m.rw ? 'rw' : 'ro'
  if (m.type === 'volume') {
    return `volume ${m.name} ${m.target} ${mode}`
  }
  const source = relocatedTargets.has(m.target) ? `*/${m.source.split('/').pop()}` : m.source
  return `${m.type} ${source} ${m.target} ${mode}`
}

// A11: the rendered baseline describes the running containers (labels and tags differ by design).
export function compareBaseline(rendered, containers, imageIds, releaseDir) {
  const out = []
  const byName = new Map(containers.map((c) => [c.name, c]))
  for (const [service, svc] of Object.entries(rendered.services)) {
    const c = byName.get(svc.container_name)
    if (!c) {
      out.push(`${service}: no running container ${svc.container_name}`)
      continue
    }
    const diff = (what) => out.push(`${service}: ${what} differs from the running container`)
    const volumes = svc.volumes ?? []
    const relocated = new Set(
      volumes
        .filter((v) => v.type === 'bind' && v.source.startsWith(`${releaseDir}/`))
        .map((v) => v.target)
    )
    if (imageIds[svc.image] !== c.imageId) {
      diff('image id')
    }
    if (!envNames(svc.environment).every((k) => envNames(c.env).includes(k))) {
      diff('environment names')
    }
    const wantMounts = volumes.map((v) => mountKey(rendered, releaseDir, v))
    if (
      !sameSet(
        wantMounts,
        c.mounts.map((m) => containerMountKey(m, relocated))
      )
    ) {
      diff('mounts (identity, source or read-only flag)')
    }
    const nets = Object.keys(svc.networks ?? {}).map((k) => rendered.networks?.[k]?.name ?? k)
    if (!sameSet(nets, c.networks)) {
      diff('networks')
    }
    const ports = (svc.ports ?? []).map((p) =>
      binding(p.host_ip, p.published, `${p.target}/${p.protocol ?? 'tcp'}`)
    )
    if (!sameSet(ports, c.portBindings)) {
      diff('published port bindings')
    }
    if (JSON.stringify(svc.healthcheck?.test ?? null) !== JSON.stringify(c.healthcheck ?? null)) {
      diff('health check')
    }
  }
  return out
}
