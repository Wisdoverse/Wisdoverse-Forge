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
 *   FORGE_PRIVATE_ROOT        mode 700: staged/ (inputs Jenkins prepared), inputs/<generation>/
 *                             (deploy.env + runner; the journal names the active one),
 *                             state.json, configs/, snapshots/
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
 *   15  commit outcome unknown: the commit write was attempted and may not be durable; nothing
 *       was stopped or rolled back; run recover (it decides from the journal on disk)
 */
import { spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { URL, fileURLToPath } from 'node:url'
import { parseEnvFile, redact, secretValues } from './lib/deploy-release/checks.js'
import { createExecutor } from './lib/deploy-release/effects.js'
import { EXIT, baseline, deploy, loadState, recover } from './lib/deploy-release/engine.js'
import { StateUnreadable, activeInputs, summarize } from './lib/deploy-release/journal.js'

const COMMANDS = ['build', 'deploy', 'recover', 'baseline', 'status', 'self-test']
const LOCKED = new Set(['deploy', 'recover', 'baseline'])
const SCRIPT = fileURLToPath(import.meta.url)
const IDENT = /^[A-Za-z0-9_]{1,40}$/

// Round 6, S3: a problem with the inputs (env file, its variables) is reported by variable name,
// never by value. With staged inputs it is a preflight refusal (12): nothing was changed.
export class InputsError extends Error {}

// Every config field some module reads (round 6, S1). The entrypoint test derives the fields from
// the code and fails when one is missing here or from what loadConfig returns. `revision` is
// derived by the engine at plan time and is not part of the loaded config.
export const CONFIG_KEYS = [
  'agentNetwork',
  'appUrl',
  'attemptId',
  'contract',
  'databases',
  'env',
  'envFile',
  'externalNetwork',
  'host',
  'lockDir',
  'pgClientImage',
  'ports',
  'privateRoot',
  'project',
  'releaseRoot',
  'requireRedis',
  'roles',
  'runnerImage',
  'secrets',
  'simulateVerifyFailure',
  'snapshotDir',
  'target',
  'volumes',
  'webRoot',
]

// Round 7, T1: every value that locates state, a network, a lock or a probe target is required.
// No defaults: several Compose defaults are the names of a live deployment on a shared host.
export const REQUIRED_KEYS = [
  'COMPOSE_PROJECT_NAME',
  'CONTAINER_NAME_PREFIX',
  'DATABASE_URL',
  'ORCHESTRATOR_DATABASE_URL',
  'DB_HOST',
  'DB_PORT',
  'TEMPORAL_DB_USER',
  'TEMPORAL_DB_PASSWORD',
  'TEMPORAL_DB_NAME',
  'EXTERNAL_NETWORK',
  'CONTAINER_NETWORK',
  'DEPLOY_LOCK_DIR',
  'WEBROOT',
  'AGENTFORGE_WORKSPACE_ROOT',
  'OAUTH_MOUNT_DIR',
  'APP_URL',
  'BIND_ADDRESS',
  'AGENTFORGE_PORT',
  'ORCHESTRATOR_PORT',
  'NATS_PORT',
  'NATS_MONITOR_PORT',
  'TEMPORAL_PORT',
  'TEMPORAL_UI_PORT',
]

function required(env, key) {
  const value = env[key]
  if (!value) {
    throw new InputsError(`${key} must be set in the env file`)
  }
  if (value.includes('${')) {
    throw new InputsError(`${key} uses \${...} interpolation, which this script does not evaluate`)
  }
  return value
}

function absolute(value, what) {
  if (!value || !path.isAbsolute(value)) {
    throw new InputsError(`${what} must be an absolute path`)
  }
  return value
}

// Round 7, T3: parsing AND percent-decoding happen inside the sanitized error path, so a bad
// URL or a bad `%xx` escape is reported by variable name, never by value.
function fromUrl(env, key) {
  const value = required(env, key)
  try {
    const url = new URL(value)
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
  } catch {
    throw new InputsError(`${key} is not a valid postgres:// URL (or contains an invalid %-escape)`)
  }
}

// Round 4, C2: deploy/build read the staged generation (`private/staged/`), and so does the first
// baseline; everything else resolves the active generation only through the journal pointer.
function resolveEnvFile(privateRoot, command) {
  const staged = path.join(privateRoot, 'staged', 'deploy.env')
  const statePath = path.join(privateRoot, 'state.json')
  let state = null
  if (fs.existsSync(statePath)) {
    try {
      state = JSON.parse(fs.readFileSync(statePath, 'utf8'))
    } catch {
      throw new StateUnreadable('state.json is not valid JSON (truncated or edited)')
    }
  }
  // status (read-only) may show a staged-only setup before any baseline exists.
  const useStaged =
    ['deploy', 'build'].includes(command) ||
    (command === 'baseline' && !state?.current) ||
    (command === 'status' && !activeInputs(state))
  if (useStaged && fs.existsSync(staged)) {
    return staged
  }
  const active = activeInputs(state)
  if (!active) {
    throw new InputsError(
      'no staged or active inputs: run the stage action, then baseline (see the runbook)'
    )
  }
  return path.join(privateRoot, 'inputs', active.generation, 'deploy.env')
}

export function loadConfig(command, args) {
  const releaseRoot = absolute(process.env.FORGE_RELEASE_ROOT, 'FORGE_RELEASE_ROOT')
  const privateRoot = absolute(process.env.FORGE_PRIVATE_ROOT, 'FORGE_PRIVATE_ROOT')
  const envFile = resolveEnvFile(privateRoot, command)
  let env
  try {
    env = parseEnvFile(fs.readFileSync(envFile, 'utf8'))
  } catch (err) {
    throw new InputsError(
      `the env file ${path.relative(privateRoot, envFile)} is malformed: ${err.message}`
    )
  }
  const missing = REQUIRED_KEYS.filter((key) => !env[key])
  if (missing.length > 0) {
    throw new InputsError(
      `required variables missing from the env file (no defaults are used): ${missing.join(', ')}`
    )
  }
  const at = args.indexOf('--revision')
  const target = at >= 0 ? args[at + 1] : env.VERSION
  if (target && !/^([0-9a-f]{7,40}|baseline)$/.test(target)) {
    throw new Error('--revision must be a commit id')
  }
  const api = fromUrl(env, 'DATABASE_URL')
  const orchestrator = fromUrl(env, 'ORCHESTRATOR_DATABASE_URL')
  // The Temporal settings of docker/compose.external.yml, all required (T1).
  const temporal = { host: required(env, 'DB_HOST'), port: required(env, 'DB_PORT'), sslmode: '' }
  Object.assign(temporal, {
    user: required(env, 'TEMPORAL_DB_USER'),
    password: required(env, 'TEMPORAL_DB_PASSWORD'),
  })
  const temporalDb = required(env, 'TEMPORAL_DB_NAME')
  absolute(env.AGENTFORGE_WORKSPACE_ROOT, 'AGENTFORGE_WORKSPACE_ROOT')
  absolute(env.OAUTH_MOUNT_DIR, 'OAUTH_MOUNT_DIR')
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
    throw new InputsError(
      'database names must be distinct and use only letters, digits and _ (max 40)'
    )
  }
  const attemptId = `${new Date().toISOString().replace(/\D/g, '').slice(0, 14)}${randomBytes(2).toString('hex')}`
  const roles = { api: api.role, orchestrator: orchestrator.role, temporal }
  const bind = env.BIND_ADDRESS === '0.0.0.0' ? '127.0.0.1' : env.BIND_ADDRESS
  return {
    env,
    envFile,
    target,
    attemptId,
    releaseRoot,
    privateRoot,
    project: required(env, 'COMPOSE_PROJECT_NAME'),
    webRoot: absolute(env.WEBROOT, 'WEBROOT'),
    lockDir: absolute(env.DEPLOY_LOCK_DIR, 'DEPLOY_LOCK_DIR'),
    externalNetwork: required(env, 'EXTERNAL_NETWORK'),
    agentNetwork: required(env, 'CONTAINER_NETWORK'),
    pgClientImage: env.DEPLOY_PG_CLIENT_IMAGE || 'postgres:18-alpine',
    runnerImage: process.env.FORGE_RUNNER_IMAGE,
    roles,
    databases,
    volumes: ['nats-data', 'agentforge-uploads'],
    snapshotDir: path.join(privateRoot, 'snapshots', attemptId),
    host: bind,
    // `<host>:<port>` for each probe. A port variable may also use Compose's `<address>:<port>`
    // form (the compose file publishes NATS without BIND_ADDRESS); the probe then uses that address.
    ports: {
      api: probeAddress(
        env,
        env.AGENTFORGE_HOST_PORT ? 'AGENTFORGE_HOST_PORT' : 'AGENTFORGE_PORT',
        bind
      ),
      orchestrator: probeAddress(env, 'ORCHESTRATOR_PORT', bind),
      natsMonitor: probeAddress(env, 'NATS_MONITOR_PORT', bind),
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

function probeAddress(env, key, bind) {
  const m = /^(?:([0-9.]+|\[[0-9a-f:]+\]):)?([0-9]{1,5})$/i.exec(env[key])
  if (!m) throw new InputsError(`${key} must be a port number or <address>:<port>`)
  const address = !m[1] || m[1] === '0.0.0.0' ? bind : m[1]
  return `${address}:${m[2]}`
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

// Lines Jenkins greps (recorded / current) and the round-5 integrity note; no secret values.
function statusNotes(ctx, revision) {
  const { state, exec } = ctx
  const notes = []
  const active = activeInputs(state)
  const facts = active ? exec.inputsFacts(active.generation) : null
  if (active && facts?.envSha256 !== active.envSha256) {
    const same = facts?.identity === active.identity
    notes.push(
      `note: the active env file differs from its recorded hash (deployment identity ${same ? 'unchanged' : 'CHANGED'})`
    )
  }
  if (revision) {
    const recorded = Object.values(state.records).some((r) => r.revision === revision)
    notes.push(`revision ${revision}: ${recorded ? 'recorded' : 'not recorded'}`)
    let staged = null
    try {
      staged = exec.readStaged()
    } catch {
      staged = null
    }
    const live = state.records[state.current]
    const current =
      staged && live?.revision === revision && live.inputs.envSha256 === staged.envSha256
    notes.push(`release ${revision} with the staged env: ${current ? 'current' : 'not current'}`)
  }
  return notes
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
        statusNotes(ctx, cfg.target).forEach(log)
        return EXIT.OK
    }
  } catch (err) {
    if (err instanceof InputsError) {
      const staged = command !== 'recover'
      console.error(
        `${staged ? 'refused' : 'RECOVERY REQUIRED'}: ${redact(err.message, secrets)} (nothing was changed)`
      )
      return staged ? EXIT.PREFLIGHT_REFUSED : EXIT.RECOVERY_REQUIRED
    }
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

// Runs only as the entrypoint, so tests can import loadConfig and CONFIG_KEYS.
if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT) {
  process.exitCode = await main(process.argv.slice(2))
}
