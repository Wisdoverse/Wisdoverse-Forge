// Round 8 review, item 4: the Jenkinsfile's runner-publish shell block, extracted and run for real
// with `sh` against a stand-in `docker`. The stand-in keeps a file-backed tag registry and
// implements the lock-holder container with the real `flock` on the real lock file, so the
// block's lock discipline is exercised, not simulated. No Docker daemon is used.
import { spawn, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const root = path.resolve(import.meta.dirname, '../../..')
const REV = 'fedcba9876543210fedcba9876543210fedcba98'
const SHARED = `forge-deploy-runner:${REV}`
let tmp = ''
const p = (...parts: string[]) => path.join(tmp, ...parts)

// The block as the shell sees it: Groovy ''' strings turn `\\` into `\`.
function runnerBlock() {
  const text = fs.readFileSync(path.join(root, 'Jenkinsfile'), 'utf8')
  const block = [...text.matchAll(/'''(#!\/bin\/sh[\s\S]*?)'''/g)]
    .map((m) => m[1])
    .find((b) => b.includes('forge-image-lock-'))
  if (!block) {
    throw new Error('runner publish block not found in the Jenkinsfile')
  }
  return block.replaceAll('\\\\', '\\')
}

const STUB = String.raw`#!/usr/bin/env node
const fs = require('fs')
const path = require('path')
const { spawn } = require('child_process')
const here = path.dirname(fs.realpathSync(process.argv[1]))
const reg = path.join(here, 'registry.json')
const read = () => (fs.existsSync(reg) ? JSON.parse(fs.readFileSync(reg, 'utf8')) : { tags: {}, images: {} })
const write = (r) => fs.writeFileSync(reg, JSON.stringify(r))
const argv = process.argv.slice(2)
fs.appendFileSync(path.join(here, 'calls.log'), JSON.stringify({ t: Date.now(), argv }) + '\n')
const exit = (code, out = '', err = '') => { process.stdout.write(out); process.stderr.write(err); process.exit(code) }
const val = (flag) => argv[argv.indexOf(flag) + 1]
if (argv[0] === 'run' && argv.includes('-d')) {
  const name = val('--name')
  const src = /src=([^,]+)/.exec(val('--mount'))[1]
  const log = path.join(here, name + '.log')
  const child = spawn('flock', ['-x', path.join(src, 'images.lock'), 'sh', '-c', 'echo locked > "$1"; exec sleep 900', 'sh', log], { detached: true, stdio: 'ignore' })
  fs.writeFileSync(path.join(here, name + '.pid'), String(child.pid))
  child.unref()
  exit(0, 'holder-id\n')
}
if (argv[0] === 'logs') {
  const log = path.join(here, argv[1] + '.log')
  exit(0, fs.existsSync(log) ? fs.readFileSync(log, 'utf8') : '')
}
if (argv[0] === 'rm') {
  const pid = path.join(here, argv[argv.length - 1] + '.pid')
  if (fs.existsSync(pid)) {
    try { process.kill(-Number(fs.readFileSync(pid, 'utf8')), 'SIGKILL') } catch {}
    fs.rmSync(pid)
    fs.rmSync(path.join(here, argv[argv.length - 1] + '.log'), { force: true })
  }
  exit(0)
}
if (argv[0] === 'image' && argv[1] === 'inspect') {
  const r = read()
  const ref = argv[argv.length - 1]
  const id = r.tags[ref]
  if (!id) exit(1, '', 'Error response from daemon: No such image: ' + ref + '\n')
  const format = val('--format')
  if (format === '{{.Id}}') exit(0, id + '\n')
  exit(0, (r.images[id].labels[/"([^"]+)"/.exec(format)[1]] ?? '') + '\n')
}
if (argv[0] === 'build') {
  const r = read()
  const id = 'sha256:' + require('crypto').randomBytes(32).toString('hex')
  const [k, v] = val('--label').split('=')
  r.images[id] = { labels: { [k]: v } }
  r.tags[val('-t')] = id
  write(r)
  exit(0)
}
if (argv[0] === 'tag') { const r = read(); r.tags[argv[2]] = argv[1]; write(r); exit(0) }
if (argv[0] === 'image' && argv[1] === 'rm') { const r = read(); delete r.tags[argv[2]]; write(r); exit(0) }
exit(0)
`

function run(script = runnerBlock()) {
  const env = {
    PATH: `${p('bin')}:${process.env.PATH}`,
    JENKINS_HOME: p('jenkins'),
    FORGE_JOB_DIR: 'forge-deploy/job',
    FORGE_REV: REV,
    BUILD_NUMBER: '7',
    FORGE_LOCK_DIR: p('lock'),
    FORGE_PG_CLIENT_IMAGE: 'postgres:18-alpine',
  }
  return new Promise<{ code: number | null; out: string; ms: number }>((resolve) => {
    const start = Date.now()
    const child = spawn('sh', ['-c', script], { env })
    let out = ''
    child.stdout.on('data', (d) => (out += d))
    child.stderr.on('data', (d) => (out += d))
    child.on('close', (code) => resolve({ code, out, ms: Date.now() - start }))
  })
}
const registry = () => JSON.parse(fs.readFileSync(p('bin', 'registry.json'), 'utf8'))
const seed = (labels: Record<string, string>) =>
  fs.writeFileSync(
    p('bin', 'registry.json'),
    JSON.stringify({
      tags: { [SHARED]: 'sha256:existing' },
      images: { 'sha256:existing': { labels } },
    })
  )
const calls = () =>
  fs
    .readFileSync(p('bin', 'calls.log'), 'utf8')
    .trim()
    .split('\n')
    .map((l) => JSON.parse(l).argv[0])
const recorded = () =>
  fs.readFileSync(p('jenkins/forge-deploy/job/private/staged/runner-image'), 'utf8').trim()

beforeEach(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-jenkins-'))
  for (const d of ['bin', 'lock', 'jenkins/forge-deploy/job/private/staged']) {
    fs.mkdirSync(p(d), { recursive: true })
  }
  fs.mkdirSync(p('jenkins/forge-deploy/job/release/releases', REV, 'docker'), { recursive: true })
  fs.writeFileSync(
    p('jenkins/forge-deploy/job/release/releases', REV, 'docker/Dockerfile.deploy-runner'),
    'FROM scratch\n'
  )
  fs.writeFileSync(p('bin', 'docker'), STUB, { mode: 0o755 })
})
afterEach(() => fs.rmSync(tmp, { recursive: true, force: true }))

describe('Jenkinsfile runner publish (real sh, real flock, stand-in docker)', () => {
  it('the extracted block passes shellcheck', () => {
    const file = p('block.sh')
    fs.writeFileSync(file, runnerBlock())
    const r = spawnSync('shellcheck', ['-s', 'sh', file], { encoding: 'utf8' })
    expect(r.status, r.stdout).toBe(0)
  })

  it('absent tag: builds with the revision label, tags under the lock, records the id read back', async () => {
    const r = await run()
    expect(r.code, r.out).toBe(0)
    const reg = registry()
    expect(recorded()).toBe(reg.tags[SHARED])
    expect(reg.images[reg.tags[SHARED]].labels['org.opencontainers.image.revision']).toBe(REV)
    expect(Object.keys(reg.tags)).toEqual([SHARED]) // the temporary tag is gone
    expect(fs.readdirSync(p('bin')).filter((f) => f.endsWith('.pid'))).toEqual([]) // lock released
  })

  it('present with this revision label: adopted without a build', async () => {
    seed({ 'org.opencontainers.image.revision': REV })
    const r = await run()
    expect(r.code, r.out).toBe(0)
    expect(r.out).toContain('Adopted the existing runner image sha256:existing')
    expect(recorded()).toBe('sha256:existing')
    expect(calls()).not.toContain('build')
    expect(calls()).not.toContain('tag')
  })

  it('present with another revision label: refusal (12), nothing built, tagged or recorded', async () => {
    seed({ 'org.opencontainers.image.revision': 'not-this-one' })
    const r = await run()
    expect(r.code).toBe(12)
    expect(r.out).toMatch(/exists with another revision label; a release tag is immutable/)
    expect(calls()).not.toContain('build')
    expect(calls()).not.toContain('tag')
    expect(fs.existsSync(p('jenkins/forge-deploy/job/private/staged/runner-image'))).toBe(false)
  })

  it('waits while another process holds the image lock, then proceeds', async () => {
    const holder = spawn('flock', ['-x', p('lock/images.lock'), 'sleep', '2'], { stdio: 'ignore' })
    await new Promise((r) => setTimeout(r, 300))
    const r = await run()
    holder.kill()
    expect(r.code, r.out).toBe(0)
    expect(r.ms).toBeGreaterThanOrEqual(1500)
    expect(recorded()).toBe(registry().tags[SHARED])
  }, 30_000)
})
