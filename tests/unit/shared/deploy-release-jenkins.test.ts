// Round 8/9 reviews, item 3/4: the Jenkinsfile's runner-publish shell block, extracted and run
// for real with `sh` and the real `flock` on a real lock file. Only `docker` is a stand-in: a
// file-backed tag registry. No Docker daemon is used.
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
    .find((b) => b.includes('images.lock') && b.includes('forge-deploy-runner:'))
  if (!block) {
    throw new Error('runner publish block not found in the Jenkinsfile')
  }
  return block.replaceAll('\\\\', '\\')
}

const STUB = String.raw`#!/usr/bin/env node
const fs = require('fs')
const path = require('path')
const here = path.dirname(fs.realpathSync(process.argv[1]))
const reg = path.join(here, 'registry.json')
const read = () => (fs.existsSync(reg) ? JSON.parse(fs.readFileSync(reg, 'utf8')) : { tags: {}, images: {} })
const write = (r) => fs.writeFileSync(reg, JSON.stringify(r))
const argv = process.argv.slice(2)
fs.appendFileSync(path.join(here, 'calls.log'), JSON.stringify({ t: Date.now(), argv }) + '\n')
const exit = (code, out = '', err = '') => { process.stdout.write(out); process.stderr.write(err); process.exit(code) }
const val = (flag) => argv[argv.indexOf(flag) + 1]
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

function run(script = runnerBlock(), extra: Record<string, string> = {}) {
  const env = {
    PATH: `${p('bin')}:${process.env.PATH}`,
    JENKINS_HOME: p('jenkins'),
    FORGE_JOB_DIR: 'forge-deploy/job',
    FORGE_REV: REV,
    BUILD_NUMBER: '7',
    FORGE_LOCK_DIR: p('lock'),
    ...extra,
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
    expect(r.out).toMatch(/has another revision label \(a release tag is immutable/)
    expect(calls()).not.toContain('build')
    expect(calls()).not.toContain('tag')
    expect(fs.existsSync(p('jenkins/forge-deploy/job/private/staged/runner-image'))).toBe(false)
  })

  it('waits while another process holds the lock and publishes the tag, then adopts its id', async () => {
    // The other publisher: holds the real lock, needs 2 s, then creates the tag with this label.
    const other = `
      sleep 2
      node -e '
        const fs = require("fs"); const f = process.argv[1]; const r = JSON.parse(fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "{\\"tags\\":{},\\"images\\":{}}")
        r.images["sha256:first"] = { labels: { "org.opencontainers.image.revision": process.argv[2] } }
        r.tags[process.argv[3]] = "sha256:first"
        fs.writeFileSync(f, JSON.stringify(r))' "$0" "$1" "$2"`
    const holder = spawn(
      'flock',
      ['-x', p('lock/images.lock'), 'sh', '-c', other, p('bin', 'registry.json'), REV, SHARED],
      { stdio: 'ignore' }
    )
    const holderDone = new Promise((done) => holder.on('close', done))
    await new Promise((r) => setTimeout(r, 300))
    const r = await run()
    await holderDone
    expect(r.code, r.out).toBe(0)
    expect(r.ms).toBeGreaterThanOrEqual(1500)
    expect(r.out).toContain('Adopted the existing runner image sha256:first')
    expect(recorded()).toBe('sha256:first')
    expect(calls()).not.toContain('build')
    expect(calls()).not.toContain('tag')
  }, 30_000)

  it('a lock wait timeout is a refusal: non-zero exit, nothing built, tagged or recorded', async () => {
    const holder = spawn('flock', ['-x', p('lock/images.lock'), 'sleep', '5'], { stdio: 'ignore' })
    await new Promise((r) => setTimeout(r, 300))
    const r = await run(runnerBlock(), { FORGE_IMAGE_LOCK_WAIT: '1' })
    holder.kill()
    expect(r.code).toBe(12)
    expect(r.out).toMatch(/images\.lock was not free within 1 s; nothing was deployed/)
    expect(fs.existsSync(p('bin', 'registry.json'))).toBe(false)
    expect(fs.existsSync(p('jenkins/forge-deploy/job/private/staged/runner-image'))).toBe(false)
  }, 30_000)

  it('refuses when DEPLOY_LOCK_DIR is not visible on the agent', async () => {
    const r = await run(runnerBlock(), { FORGE_LOCK_DIR: p('no-such-dir') })
    expect(r.code).toBe(12)
    expect(r.out).toMatch(/is not a writable directory on the Jenkins agent/)
  })
})
