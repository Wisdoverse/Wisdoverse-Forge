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

// A5: refuse what a template copy would not carry. A8: refuse while agent containers run.
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

const AGENT_LABELS = [
  'agentforge.agent_id',
  'agentforge.dev_environment_id',
  'agentforge.project_clone',
]

export function agentContainerFindings(containers) {
  return containers
    .filter((c) => AGENT_LABELS.some((label) => label in c.labels))
    .map(
      (c) => `agent container ${c.name} is running on the deployment's agent network; stop it first`
    )
}

export function preflightFindings(config, state, o) {
  const out = []
  if (!state.current || !state.records[state.current]) {
    out.push('no current release is recorded: run `baseline` once before the first deploy')
  }
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
  for (const [key, cmp] of Object.entries(o.ledgers)) {
    out.push(...ledgerFindings(key, cmp, { strict: false }))
  }
  out.push(...identityFindings(config.project, o.rendered, o.containers))
  out.push(...databaseFindings(config.databases, o.databases))
  out.push(...agentContainerFindings(o.agentContainers))
  return out
}

function httpFinding(label, res, ok) {
  if (!res || res.error) {
    return [`${label}: check could not run (${res?.error ?? 'no response'})`]
  }
  return res.status === 200 && ok(res.body ?? {})
    ? []
    : [`${label}: unhealthy (HTTP ${res.status})`]
}

export function streamFindings(jsz, contract) {
  if (!jsz || jsz.error) {
    return [`NATS JetStream: check could not run (${jsz?.error ?? 'no response'})`]
  }
  const streams = (jsz.body?.account_details ?? []).flatMap((a) => a.stream_detail ?? [])
  const names = new Set(streams.map((s) => s.name))
  const out = contract.streams.filter((s) => !names.has(s)).map((s) => `NATS stream ${s} missing`)
  const { stream, name } = contract.resultConsumer
  const consumers = streams.find((s) => s.name === stream)?.consumer_detail ?? []
  if (!consumers.some((c) => c.name === name)) {
    out.push(`NATS consumer ${name} on ${stream} missing`)
  }
  return out
}

export function backendFindings(facts, expect) {
  const deps = (b) =>
    b.checks?.database &&
    b.checks?.nats &&
    b.checks?.docker &&
    (!expect.requireRedis || b.checks?.redis)
  const out = [
    ...httpFinding('API /health', facts.apiHealth, (b) => b.ok === true),
    ...httpFinding(
      'API /api/health',
      facts.apiReady,
      (b) => b.ok === true && b.status === 'ready' && deps(b)
    ),
    ...httpFinding('orchestrator /health', facts.orchHealth, (b) => b.workflowRuntime === 'up'),
    ...httpFinding('orchestrator /health/ready', facts.orchReady, (b) => b.ok === true),
    ...httpFinding('NATS /healthz', facts.natsHealth, (b) => b.status === 'ok'),
    ...streamFindings(facts.jsz, expect.contract),
  ]
  if (!facts.temporalServing) {
    out.push('Temporal cluster health is not SERVING')
  }
  for (const [service, id] of Object.entries(expect.images)) {
    if (facts.runningImages[service] !== id) {
      out.push(`service ${service} is not running the release image`)
    }
  }
  for (const [key, cmp] of Object.entries(facts.ledgers)) {
    out.push(...ledgerFindings(key, cmp, { strict: true }))
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

// A11: the rendered baseline describes the running containers (labels and tags differ by design).
export function compareBaseline(rendered, containers, imageIds) {
  const out = []
  const byName = new Map(containers.map((c) => [c.name, c]))
  for (const [service, svc] of Object.entries(rendered.services)) {
    const c = byName.get(svc.container_name)
    if (!c) {
      out.push(`${service}: no running container ${svc.container_name}`)
      continue
    }
    const diff = (what) => out.push(`${service}: ${what} differs from the running container`)
    if (imageIds[svc.image] !== c.imageId) {
      diff('image id')
    }
    if (!envNames(svc.environment).every((k) => envNames(c.env).includes(k))) {
      diff('environment names')
    }
    if (
      !sameSet(
        (svc.volumes ?? []).map((v) => v.target),
        c.mounts.map((m) => m.target)
      )
    ) {
      diff('mounts')
    }
    const nets = Object.keys(svc.networks ?? {}).map((k) => rendered.networks?.[k]?.name ?? k)
    if (!sameSet(nets, c.networks)) {
      diff('networks')
    }
    if (
      !sameSet(
        (svc.ports ?? []).map((p) => `${p.target}/${p.protocol ?? 'tcp'}`),
        c.ports
      )
    ) {
      diff('published ports')
    }
    if (JSON.stringify(svc.healthcheck?.test ?? null) !== JSON.stringify(c.healthcheck ?? null)) {
      diff('health check')
    }
  }
  return out
}
