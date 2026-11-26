// Round 8 review, item 4: two real `build` processes contend on the real `flock` of the image lock.
// A stand-in `docker` keeps a file-backed tag registry shared by both (a daemon's tags are global),
// takes 1.5 s per build and 2 s per `docker tag`, so the second publisher must block on the lock
// while the first holds it, and then adopt the first writer's image id.
import { spawn } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const root = path.resolve(import.meta.dirname, '../../..')
const CLI = path.join(root, 'scripts/deploy-release.mjs')
const REV = '89abcdef0123456789abcdef0123456789abcdef'
let tmp = ''
const p = (...parts: string[]) => path.join(tmp, ...parts)

const STUB = `#!/usr/bin/env node
const fs = require('fs')
const path = require('path')
const here = path.dirname(fs.realpathSync(process.argv[1]))
const reg = path.join(here, 'registry')
fs.mkdirSync(reg, { recursive: true })
const argv = process.argv.slice(2)
// The tool passes only a fixed environment to docker; HOME tells the two deployments apart.
const who = path.basename(process.env.HOME)
const key = (ref) => path.join(reg, encodeURIComponent(ref))
const log = (event, extra = {}) =>
  fs.appendFileSync(path.join(here, 'calls.log'), JSON.stringify({ t: Date.now(), who, event, argv, ...extra }) + '\\n')
const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms)
const exit = (code, out = '', err = '') => { process.stdout.write(out); process.stderr.write(err); process.exit(code) }
if (argv[0] === 'compose' && argv.includes('config')) {
  exit(0, JSON.stringify({ services: { 'agentforge-server': { build: {} }, orchestrator: { build: {} } } }))
}
if (argv[0] === 'compose' && argv.includes('build')) {
  const override = JSON.parse(fs.readFileSync(argv[argv.indexOf('build') - 1], 'utf8'))
  log('compose-build', { services: Object.keys(override.services) })
  sleep(1500)
  for (const s of Object.values(override.services)) {
    fs.writeFileSync(key(s.image), 'sha256:' + who + '-' + s.image.split(':')[0])
  }
  exit(0)
}
if (argv[0] === 'build') {
  fs.writeFileSync(key(argv[argv.indexOf('-t') + 1]), 'sha256:' + who + '-agentforge-frontend')
  exit(0)
}
if (argv[0] === 'image' && argv[1] === 'inspect') {
  const ref = argv[argv.length - 1]
  if (!fs.existsSync(key(ref))) exit(1, '', 'Error: No such image: ' + ref + '\\n')
  if (argv.includes('{{json .Config.Labels}}')) {
    log('labels', { ref })
    exit(0, JSON.stringify({ 'org.opencontainers.image.revision': '${REV}' }) + '\\n')
  }
  exit(0, fs.readFileSync(key(ref), 'utf8') + '\\n')
}
if (argv[0] === 'tag') {
  log('tag-start', { ref: argv[2] })
  sleep(2000)
  fs.writeFileSync(key(argv[2]), argv[1])
  log('tagged', { ref: argv[2], id: argv[1] })
  exit(0)
}
if (argv[0] === 'image' && argv[1] === 'rm') { fs.rmSync(key(argv[2]), { force: true }); exit(0) }
if (argv[0] === 'create') exit(0, 'cid\\n')
if (argv[0] === 'inspect') exit(1, '', 'Error: No such object\\n')
exit(0)
`

function envText(who: string) {
  return (
    [
      `COMPOSE_PROJECT_NAME=lock-${who}`,
      `CONTAINER_NAME_PREFIX=lock-${who}`,
      'DATABASE_URL=postgres://api:x-secret-1@db.internal:5432/forge',
      'ORCHESTRATOR_DATABASE_URL=postgres://orch:x-secret-2@db.internal:5432/forge_orch',
      'DB_HOST=db.internal',
      'DB_PORT=5432',
      'TEMPORAL_DB_USER=temporal_lock',
      'TEMPORAL_DB_PASSWORD=x-secret-3',
      'TEMPORAL_DB_NAME=temporal_lock',
      'EXTERNAL_NETWORK=lock-ext',
      'CONTAINER_NETWORK=lock-agents',
      `WEBROOT=${p(who, 'www')}`,
      // One lock directory for every deployment on the host (runbook: Release image tags).
      `DEPLOY_LOCK_DIR=${p('lock')}`,
      `AGENTFORGE_WORKSPACE_ROOT=${p(who, 'workspaces')}`,
      `OAUTH_MOUNT_DIR=${p(who, 'oauth')}`,
      'APP_URL=http://127.0.0.1:24080',
      'BIND_ADDRESS=127.0.0.1',
      'AGENTFORGE_PORT=24003',
      'ORCHESTRATOR_PORT=24010',
      'NATS_PORT=24222',
      'NATS_MONITOR_PORT=28222',
      'TEMPORAL_PORT=27233',
      'TEMPORAL_UI_PORT=28233',
      `VERSION=${REV}`,
    ].join('\n') + '\n'
  )
}

function build(who: string) {
  fs.mkdirSync(p(who, 'private/staged'), { recursive: true })
  fs.mkdirSync(p(who, 'release/tmp'), { recursive: true })
  fs.mkdirSync(p(who, 'release/releases', REV, 'docker'), { recursive: true })
  fs.writeFileSync(p(who, 'private/staged/deploy.env'), envText(who), { mode: 0o600 })
  const env = {
    PATH: `${p('bin')}:${process.env.PATH}`,
    HOME: p(who),
    TMPDIR: p(who, 'release/tmp'),
    FORGE_RELEASE_ROOT: p(who, 'release'),
    FORGE_PRIVATE_ROOT: p(who, 'private'),
    FORGE_RUNNER_IMAGE: 'forge-deploy-runner:test',
  }
  return new Promise<{ code: number | null; out: string }>((resolve) => {
    const child = spawn(process.execPath, [CLI, 'build', '--revision', REV], { env })
    let out = ''
    child.stdout.on('data', (d) => (out += d))
    child.stderr.on('data', (d) => (out += d))
    child.on('close', (code) => resolve({ code, out }))
  })
}

beforeEach(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-lock-'))
  fs.mkdirSync(p('bin'))
  fs.writeFileSync(p('bin', 'docker'), STUB, { mode: 0o755 })
})
afterEach(() => fs.rmSync(tmp, { recursive: true, force: true }))

describe('image publication lock: two real processes, the real flock', () => {
  it('both saw the tag absent and built; the second blocks until the first releases, then adopts the first id', async () => {
    const a = build('A')
    await new Promise((r) => setTimeout(r, 400))
    const b = build('B')
    const [ra, rb] = await Promise.all([a, b])
    expect(ra.code, ra.out).toBe(0)
    expect(rb.code, rb.out).toBe(0)
    const events = fs
      .readFileSync(p('bin', 'calls.log'), 'utf8')
      .trim()
      .split('\n')
      .map((l) => JSON.parse(l))
    // Both observed the server tag absent and built their own image.
    for (const who of ['A', 'B']) {
      const built = events.find((e) => e.who === who && e.event === 'compose-build')
      expect(built?.services).toEqual(['agentforge-server', 'orchestrator'])
    }
    for (const repo of ['agentforge-server', 'agentforge-orchestrator', 'agentforge-frontend']) {
      const shared = `${repo}:${REV}`
      const tagged = events.filter((e) => e.event === 'tagged' && e.ref === shared)
      expect(tagged, shared).toHaveLength(1) // first writer wins; the tag is never moved
      const [first] = tagged
      const second = first.who === 'A' ? 'B' : 'A'
      const out = second === 'A' ? ra.out : rb.out
      expect(out).toContain(`${shared} was published by another build first; adopted ${first.id}`)
      expect(out).toContain(`=${first.id}`)
      // The adopter read the labels inside the lock only after the first writer released it.
      const adoptedAt = events.find(
        (e) => e.who === second && e.event === 'labels' && e.ref === shared
      )?.t
      expect(adoptedAt).toBeGreaterThanOrEqual(first.t)
      expect(
        fs.readFileSync(path.join(p('bin', 'registry'), encodeURIComponent(shared)), 'utf8')
      ).toBe(first.id)
    }
    // The server publisher that came second really waited: its labels read follows the 2 s tag.
    const serverTag = events.find(
      (e) => e.event === 'tag-start' && e.ref === `agentforge-server:${REV}`
    )
    const waiter = serverTag.who === 'A' ? 'B' : 'A'
    const waited = events.find(
      (e) => e.who === waiter && e.event === 'labels' && e.ref === `agentforge-server:${REV}`
    )
    expect(waited.t - serverTag.t).toBeGreaterThanOrEqual(1900)
  }, 60_000)
})
