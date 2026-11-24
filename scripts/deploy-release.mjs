#!/usr/bin/env node
/**
 * Wisdoverse Forge release pipeline (issue #1319). Runs inside the deploy runner image
 * (docker/Dockerfile.deploy-runner), started by the Jenkinsfile or by the docker run line in
 * docs/runbooks/boe-deploy.md.
 *
 * Usage: node scripts/deploy-release.mjs <command> [--revision <commit>]
 *   build      build server, orchestrator and frontend images tagged <commit>; live stack untouched
 *   deploy     preflight, quiesce, cold snapshot, migrate, start, verify, switch frontend, commit;
 *              any failure after quiesce rolls back to the previous release
 *   recover    finish or roll back an interrupted attempt, driven by the journal and observed state
 *   baseline   record the hand-deployed release once, so the first deploy can roll back to it
 *   status     print the journal summary and current/previous release (no secrets); with
 *              --revision, also `revision <commit>: recorded|not recorded`
 *   self-test  run the planner against the fault scenarios in tests/support (fake executor)
 *
 * Environment (paths are host paths, mounted at the same path in the runner):
 *   FORGE_RELEASE_ROOT        holds releases/<commit>/ (git archive, no secrets) and frontend/<commit>/
 *   FORGE_PRIVATE_ROOT        mode 700: deploy.env (active), staged.env (from Jenkins, promoted
 *                             with a new attempt), state.json, configs/, snapshots/
 *   FORGE_RUNNER_IMAGE        tag of this runner image (volume archives and bind-source probes)
 *   SIMULATE_VERIFY_FAILURE   1 or true forces the rollback path after the backend starts
 *
 * Exit codes:
 *   0   success, already-current no-op, or nothing to recover
 *   1   usage or unexpected error before any state change
 *   10  deploy failed and was rolled back; the previous release was verified
 *   11  recovery required: project services stopped; run status, repair, then recover
 *   12  preflight refused; nothing was changed
 *   13  lock busy: another deploy, recover or baseline is running for this project
 *   14  finalize failed: the result was reached (a committed release is left running) but finishing
 *       work failed, for example a journal write or removing the SQL helper; run recover
 */
import { spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { URL, fileURLToPath } from 'node:url'
import { parseEnvFile, redact, secretValues } from './lib/deploy-release/checks.js'
import { createExecutor } from './lib/deploy-release/effects.js'
import { EXIT, deploy, loadState, recover } from './lib/deploy-release/engine.js'
import { StateUnreadable, summarize } from './lib/deploy-release/journal.js'

const COMMANDS = ['build', 'deploy', 'recover', 'baseline', 'status', 'self-test']
const LOCKED = new Set(['deploy', 'recover', 'baseline'])
const SCRIPT = fileURLToPath(import.meta.url)
const IDENT = /^[A-Za-z0-9_]{1,40}$/

function required(env, key) {
  const value = env[key]
  if (!value) {
    throw new Error(`${key} must be set in the env file`)
  }
  if (value.includes('${')) {
    throw new Error(`${key} uses \${...} interpolation, which this script does not evaluate`)
  }
  return value
}

function absolute(value, what) {
  if (!value || !path.isAbsolute(value)) {
    throw new Error(`${what} must be an absolute path`)
  }
  return value
}

function fromUrl(env, key) {
  const url = new URL(required(env, key))
  const name = decodeURIComponent(url.pathname.slice(1))
  const role = {
    host: url.hostname,
    port: url.port || '5432',
    user: decodeURIComponent(url.username),
  }
  Object.assign(role, {
    password: decodeURIComponent(url.password),
    sslmode: url.searchParams.get('sslmode') ?? '',
  })
  return { role, name }
}

function loadConfig(command, args) {
  const releaseRoot = absolute(process.env.FORGE_RELEASE_ROOT, 'FORGE_RELEASE_ROOT')
  const privateRoot = absolute(process.env.FORGE_PRIVATE_ROOT, 'FORGE_PRIVATE_ROOT')
  // deploy/build read the inputs Jenkins staged; status/recover only the active ones. The very
  // first baseline has no active inputs yet and promotes the staged ones.
  const staged = path.join(privateRoot, 'staged.env')
  const active = path.join(privateRoot, 'deploy.env')
  const wantsStaged =
    ['deploy', 'build'].includes(command) || (command === 'baseline' && !fs.existsSync(active))
  const envFile = wantsStaged && fs.existsSync(staged) ? staged : active
  const env = parseEnvFile(fs.readFileSync(envFile, 'utf8'))
  const at = args.indexOf('--revision')
  const target = at >= 0 ? args[at + 1] : env.VERSION
  if (target && !/^([0-9a-f]{7,40}|baseline)$/.test(target)) {
    throw new Error('--revision must be a commit id')
  }
  const api = fromUrl(env, 'DATABASE_URL')
  const orchestrator = fromUrl(env, 'ORCHESTRATOR_DATABASE_URL')
  // Mirrors the Temporal settings and defaults of docker/compose.external.yml.
  const temporal = { host: env.DB_HOST || 'db', port: env.DB_PORT || '5432', sslmode: '' }
  Object.assign(temporal, {
    user: env.TEMPORAL_DB_USER || 'temporal',
    password: env.TEMPORAL_DB_PASSWORD || 'temporal',
  })
  const temporalDb = env.TEMPORAL_DB_NAME || 'temporal'
  const databases = [
    { key: 'api', role: 'api', name: api.name, migrations: 'rust/crates/db/migrations' },
    {
      key: 'orchestrator',
      role: 'orchestrator',
      name: orchestrator.name,
      migrations: 'rust/crates/orchestrator/migrations',
    },
    { key: 'temporal', role: 'temporal', name: temporalDb },
    { key: 'temporal_visibility', role: 'temporal', name: `${temporalDb}_visibility` },
  ]
  const names = databases.map((d) => d.name)
  if (!names.every((n) => IDENT.test(n)) || new Set(names).size !== names.length) {
    throw new Error('database names must be distinct and use only letters, digits and _ (max 40)')
  }
  const attemptId = `${new Date().toISOString().replace(/\D/g, '').slice(0, 14)}${randomBytes(2).toString('hex')}`
  const roles = { api: api.role, orchestrator: orchestrator.role, temporal }
  const bind = env.BIND_ADDRESS && env.BIND_ADDRESS !== '0.0.0.0' ? env.BIND_ADDRESS : '127.0.0.1'
  return {
    env,
    target,
    attemptId,
    releaseRoot,
    privateRoot,
    project: required(env, 'COMPOSE_PROJECT_NAME'),
    webRoot: absolute(env.WEBROOT, 'WEBROOT'),
    lockDir: absolute(env.DEPLOY_LOCK_DIR || '/var/lock/forge-deploy', 'DEPLOY_LOCK_DIR'),
    externalNetwork: env.EXTERNAL_NETWORK || 'external-network',
    agentNetwork: env.CONTAINER_NETWORK || 'agentforge-agents',
    pgClientImage: env.DEPLOY_PG_CLIENT_IMAGE || 'postgres:18-alpine',
    runnerImage: process.env.FORGE_RUNNER_IMAGE,
    roles,
    databases,
    volumes: ['nats-data', 'agentforge-uploads'],
    snapshotDir: path.join(privateRoot, 'snapshots', attemptId),
    host: bind,
    ports: {
      api: env.AGENTFORGE_HOST_PORT || env.AGENTFORGE_PORT || '4003',
      orchestrator: env.ORCHESTRATOR_PORT || '4010',
      natsMonitor: env.NATS_MONITOR_PORT || '8222',
    },
    appUrl: env.APP_URL,
    requireRedis: env.REQUIRE_EXTERNAL_STATE === 'true',
    simulateVerifyFailure: ['1', 'true'].includes(process.env.SIMULATE_VERIFY_FAILURE ?? ''),
    contract: JSON.parse(
      fs.readFileSync(path.join(path.dirname(SCRIPT), 'deploy-release.jetstream.json'), 'utf8')
    ),
    secrets: [...secretValues(env), ...Object.values(roles).map((r) => r.host)].filter(
      (s) => s.length >= 4
    ),
  }
}

function underLock(cfg, argv) {
  fs.mkdirSync(cfg.lockDir, { recursive: true })
  const lock = path.join(cfg.lockDir, `${cfg.project}.lock`)
  const args = ['-n', '-E', String(EXIT.LOCK_BUSY), lock, process.execPath, SCRIPT, ...argv]
  const r = spawnSync('flock', args, {
    stdio: 'inherit',
    env: { ...process.env, FORGE_DEPLOY_LOCKED: '1' },
  })
  if (r.error) {
    throw new Error(`flock could not start: ${r.error.message}`)
  }
  if (r.status === EXIT.LOCK_BUSY) {
    console.error(`lock busy: another deploy, recover or baseline holds ${lock}`)
  }
  return r.status ?? EXIT.FAILED
}

async function baseline(ctx) {
  if (!(await loadState(ctx))) {
    return EXIT.RECOVERY_REQUIRED
  }
  const { state } = ctx
  if (state.current) {
    ctx.log(`refused: a current release (${state.current}) is already recorded; baseline runs once`)
    return EXIT.PREFLIGHT_REFUSED
  }
  const { findings, record } = ctx.exec.baseline()
  if (findings.length > 0) {
    findings.forEach((f) => ctx.log(`refused: ${f}`))
    return EXIT.PREFLIGHT_REFUSED
  }
  ctx.exec.promoteInputs()
  await ctx.exec.writeState({
    ...state,
    current: 'baseline',
    records: { ...state.records, baseline: record },
  })
  ctx.log('baseline recorded: the next deploy can roll back to the running release')
  return EXIT.OK
}

async function selfTest() {
  // The fake executor lives with the tests; the release directory is a full `git archive`.
  const simulate = path.join(path.dirname(SCRIPT), '../tests/support/deploy-release/simulate.js')
  const { faultMatrix } = await import(simulate)
  const results = await faultMatrix()
  const failed = results.filter((r) => r.errors.length > 0)
  failed.forEach((r) => console.error(`FAIL ${r.name}: ${r.errors.join('; ')}`))
  console.log(`self-test: ${results.length - failed.length}/${results.length} scenarios passed`)
  return failed.length > 0 ? EXIT.FAILED : EXIT.OK
}

async function main(argv) {
  const [command, ...rest] = argv
  if (!COMMANDS.includes(command)) {
    console.error(`usage: deploy-release.mjs <${COMMANDS.join('|')}> [--revision <commit>]`)
    return EXIT.FAILED
  }
  if (command === 'self-test') {
    return selfTest()
  }
  let secrets = []
  try {
    const cfg = loadConfig(command, rest)
    secrets = cfg.secrets
    if (['build', 'deploy'].includes(command) && !(cfg.target && cfg.runnerImage)) {
      throw new Error(`${command} needs --revision <commit> and FORGE_RUNNER_IMAGE`)
    }
    if (LOCKED.has(command) && process.env.FORGE_DEPLOY_LOCKED !== '1') {
      return underLock(cfg, argv)
    }
    const log = (message) => console.log(redact(message, secrets))
    const ctx = { config: cfg, exec: createExecutor(cfg, log), log }
    switch (command) {
      case 'build':
        ctx.exec.build(cfg.target)
        return EXIT.OK
      case 'deploy':
        return await deploy(ctx)
      case 'recover':
        return await recover(ctx)
      case 'baseline':
        return await baseline(ctx)
      default:
        if (!(await loadState(ctx))) {
          return EXIT.RECOVERY_REQUIRED
        }
        summarize(ctx.state).forEach(log)
        if (cfg.target) {
          log(
            `revision ${cfg.target}: ${ctx.state.records[cfg.target] ? 'recorded' : 'not recorded'}`
          )
        }
        return EXIT.OK
    }
  } catch (err) {
    if (err instanceof StateUnreadable) {
      console.error(
        `RECOVERY REQUIRED: ${redact(err.message, secrets)}; inspect state.json before any deploy`
      )
      return EXIT.RECOVERY_REQUIRED
    }
    console.error(`error: ${redact(err.message, secrets)}`)
    return EXIT.FAILED
  }
}

process.exitCode = await main(process.argv.slice(2))
