// Round 6, S1: entrypoint-level tests. The real CLI (`node scripts/deploy-release.mjs ...`) runs
// as a child process with real files in a temporary directory and a recording stand-in `docker`
// first on PATH. Nothing here talks to a Docker daemon.
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { CONFIG_KEYS, loadConfig } from '../../../scripts/deploy-release.mjs'
import { dockerStub } from '../../support/deploy-release/docker-stub.js'

const root = path.resolve(import.meta.dirname, '../../..')
const CLI = path.join(root, 'scripts/deploy-release.mjs')
const REV = '0123456789abcdef0123456789abcdef01234567'
const SECRETS = ['SECRET-api-1', 'SECRET-orch-2', 'SECRET-temporal-3', 'SECRET-jwt-4']

let tmp = ''
let stub: ReturnType<typeof dockerStub>
const p = (...parts: string[]) => path.join(tmp, ...parts)
const envText = () =>
  [
    'COMPOSE_PROJECT_NAME=cli-test',
    `DATABASE_URL=postgres://api:${SECRETS[0]}@db.internal:5432/forge`,
    `ORCHESTRATOR_DATABASE_URL=postgres://orch:${SECRETS[1]}@db.internal:5432/forge_orch`,
    'DB_HOST=db.internal',
    `TEMPORAL_DB_PASSWORD=${SECRETS[2]}`,
    `JWT_SECRET=${SECRETS[3]}`,
    `WEBROOT=${p('www')}`,
    `DEPLOY_LOCK_DIR=${p('lock')}`,
    `VERSION=${REV}`,
  ].join('\n') + '\n'

function stage(text = envText(), runner = REV) {
  fs.mkdirSync(p('private/staged'), { recursive: true })
  fs.writeFileSync(p('private/staged/deploy.env'), text, { mode: 0o600 })
  fs.writeFileSync(p('private/staged/runner'), `${runner}\n`)
}

function cli(...args: string[]) {
  const env = {
    PATH: `${stub.dir}:${process.env.PATH}`,
    HOME: tmp,
    TMPDIR: p('release/tmp'),
    FORGE_RELEASE_ROOT: p('release'),
    FORGE_PRIVATE_ROOT: p('private'),
    FORGE_RUNNER_IMAGE: 'forge-deploy-runner:test',
  }
  const r = spawnSync(process.execPath, [CLI, ...args], { env, encoding: 'utf8' })
  return { code: r.status, out: `${r.stdout}${r.stderr}` }
}

const composeCalls = () => stub.calls().filter((c) => c.argv[0] === 'compose')
function expectComposeContract(envFile: string) {
  const calls = composeCalls()
  expect(calls.length).toBeGreaterThan(0)
  for (const { argv } of calls) {
    const at = (flag: string) => argv[argv.indexOf(flag) + 1]
    expect(at('--project-name')).toBe('cli-test')
    expect(at('--profile')).toBe('external')
    expect(at('--env-file')).toBe(envFile)
    const files = argv.filter((_a: string, i: number) => argv[i - 1] === '-f')
    expect(files).toEqual([
      p('release/releases', REV, 'docker/compose.yml'),
      p('release/releases', REV, 'docker/compose.external.yml'),
    ])
  }
}
function expectNoSecretInArgv() {
  const all = JSON.stringify(stub.calls().map((c) => c.argv))
  SECRETS.forEach((secret) => expect(all).not.toContain(secret))
}

beforeEach(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-deploy-cli-'))
  for (const dir of ['release/releases', 'release/tmp', 'private', 'www', 'lock']) {
    fs.mkdirSync(p(dir), { recursive: true })
  }
  fs.mkdirSync(p('release/releases', REV, 'docker'), { recursive: true })
  fs.writeFileSync(p('release/releases', REV, 'docker/compose.yml'), 'services: {}\n')
  fs.writeFileSync(p('release/releases', REV, 'docker/compose.external.yml'), 'services: {}\n')
  fs.writeFileSync(p('www/index.html'), '<html></html>')
  stub = dockerStub(p('bin'))
})

afterEach(() => {
  fs.rmSync(tmp, { recursive: true, force: true })
})

describe('deploy-release CLI (real entrypoint, stand-in docker)', () => {
  it('status with no state shows the staged setup and touches no docker', () => {
    stage()
    const r = cli('status', '--revision', REV)
    expect(r.code).toBe(0)
    expect(r.out).toMatch(/current release: +\(none - run baseline first\)/)
    expect(r.out).toContain(`revision ${REV}: not recorded`)
    expect(stub.calls()).toEqual([])
  })

  it('status with nothing staged or recorded refuses (12) without a stack trace', () => {
    const r = cli('status')
    expect(r.code).toBe(12)
    expect(r.out).toMatch(/no staged or active inputs/)
  })

  it('build passes --env-file, project, files and profile to compose; no secret in argv', () => {
    stage()
    stub.rules([
      { match: '^image inspect', code: 1 },
      { match: '^create ', out: 'frontend-cid\n' },
      { match: '^inspect frontend-cid', code: 1 },
    ])
    const r = cli('build', '--revision', REV)
    expect(r.out).toContain(`built images and frontend for ${REV}`)
    expect(r.code).toBe(0)
    expectComposeContract(p('private/staged/deploy.env'))
    const build = composeCalls().find((c) => c.argv.includes('build'))
    expect(build?.argv.slice(-3)).toEqual(['build', 'agentforge-server', 'orchestrator'])
    expect(build?.version).toBe(REV)
    expect(
      stub
        .calls()
        .some((c) => c.argv[0] === 'build' && c.argv.includes(`agentforge-frontend:${REV}`))
    ).toBe(true)
    const create = stub.calls().find((c) => c.argv[0] === 'create')
    expect(create?.argv).toEqual(
      expect.arrayContaining(['--label', 'forge.deploy.project=cli-test'])
    )
    expectNoSecretInArgv()
  })

  it('deploy runs preflight with the right compose invocation, then refuses cleanly (12)', () => {
    stage()
    fs.mkdirSync(p('private/inputs/prevgen1'), { recursive: true })
    fs.writeFileSync(p('private/inputs/prevgen1/deploy.env'), envText())
    const record = {
      rev: 'baseline',
      revision: 'baseline',
      config: p('private/configs/baseline.json'),
      releaseDir: p('release/releases/baseline'),
      indexSha256: 'x'.repeat(64),
      frontendDir: null,
      images: { 'agentforge-server': 'sha256:prev-server' },
      refs: { 'agentforge-server': 'agentforge-server:baseline-cli-test' },
      inputs: {
        generation: 'prevgen1',
        envSha256: 'a'.repeat(64),
        identity: 'c'.repeat(64),
        runner: REV,
      },
    }
    const state = {
      schema: 1,
      current: 'baseline',
      previous: null,
      protected: { baseline: null, lastGood: null },
      records: { baseline: record },
      attempt: null,
    }
    fs.writeFileSync(p('private/state.json'), JSON.stringify(state))
    stub.rules([
      { match: '^ps ', out: '' },
      { match: '^run -d ', out: 'sql-helper-id\n' },
      {
        match: '^compose .* config --format json$',
        out: JSON.stringify({ name: 'cli-test', services: {}, volumes: {}, networks: {} }),
      },
      { match: '^exec ', code: 1, err: 'psql: connection refused' },
      { match: '^inspect ', code: 1 },
    ])
    const r = cli('deploy', '--revision', REV)
    expect(r.out).toMatch(/refused: preflight could not run \(nothing was changed\)/)
    expect(r.code).toBe(12)
    const heads = stub.calls().map((c) => c.argv.slice(0, 2).join(' '))
    expect(heads).toEqual([
      'ps -aq',
      'run -d',
      'compose --profile',
      'exec -i',
      'rm -f',
      'inspect sql-helper-id',
    ])
    expectComposeContract(p('private/staged/deploy.env'))
    const exec = stub.calls().find((c) => c.argv[0] === 'exec')
    expect(exec?.pg).toEqual(['PGDATABASE', 'PGHOST', 'PGPASSWORD', 'PGPORT', 'PGUSER'])
    expect(exec?.argv).toEqual(expect.arrayContaining(['-e', 'PGPASSWORD', 'sql-helper-id']))
    expectNoSecretInArgv()
    expect(r.out).not.toMatch(/SECRET-/)
    // Nothing was persisted: no attempt, the staged inputs are still staged.
    expect(JSON.parse(fs.readFileSync(p('private/state.json'), 'utf8')).attempt).toBeNull()
    expect(fs.existsSync(p('private/staged/deploy.env'))).toBe(true)
  })

  it('deploy without staged inputs refuses (12)', () => {
    fs.mkdirSync(p('private/inputs/prevgen1'), { recursive: true })
    fs.writeFileSync(p('private/inputs/prevgen1/deploy.env'), envText())
    const state = JSON.stringify({
      schema: 1,
      current: 'baseline',
      previous: null,
      protected: { baseline: null, lastGood: null },
      records: {
        baseline: {
          rev: 'baseline',
          revision: 'baseline',
          config: p('private/configs/baseline.json'),
          releaseDir: p('release/releases/baseline'),
          indexSha256: 'x'.repeat(64),
          frontendDir: null,
          images: { s: 'sha256:1' },
          refs: { s: 's:1' },
          inputs: {
            generation: 'prevgen1',
            envSha256: 'a'.repeat(64),
            identity: 'c'.repeat(64),
            runner: REV,
          },
        },
      },
      attempt: null,
    })
    fs.writeFileSync(p('private/state.json'), state)
    const r = cli('deploy', '--revision', REV)
    expect(r.code).toBe(12)
    expect(r.out).toMatch(/no staged inputs/)
    expect(stub.calls()).toEqual([])
  })

  it.each([
    [
      'a line that is not KEY=VALUE',
      (t: string) => `${t}this line is wrong\n`,
      /line \d+ is not KEY=VALUE/,
    ],
    [
      'a database URL that does not parse',
      (t: string) => t.replace(/DATABASE_URL=.*\n/, `DATABASE_URL=::${SECRETS[0]}::\n`),
      /DATABASE_URL is not a valid postgres:\/\/ URL/,
    ],
    [
      'a missing project name',
      (t: string) => t.replace(/COMPOSE_PROJECT_NAME=.*\n/, ''),
      /COMPOSE_PROJECT_NAME must be set/,
    ],
  ])(
    'a malformed staged env (%s) is a refusal (12) naming the variable, never its value',
    (_label, mutate, message) => {
      stage(mutate(envText()))
      for (const command of ['deploy', 'build', 'baseline']) {
        const r = cli(command, '--revision', REV)
        expect(r.code).toBe(12)
        expect(r.out).toMatch(message)
        expect(r.out).not.toContain('this line is wrong')
        SECRETS.forEach((secret) => expect(r.out).not.toContain(secret))
      }
      expect(stub.calls()).toEqual([])
    }
  )
})

describe('loadConfig provides every field the modules read', () => {
  const read = (rel: string) => fs.readFileSync(path.join(root, rel), 'utf8')
  const fields = (text: string, prefix: string) =>
    [...text.matchAll(new RegExp(`\\b${prefix}\\.(\\w+)`, 'g'))].map((m) => m[1])

  it('the checked list covers every cfg./config. field in the code', () => {
    const used = new Set([
      ...fields(read('scripts/lib/deploy-release/effects.js'), 'cfg'),
      ...fields(read('scripts/deploy-release.mjs'), 'cfg'),
      ...['engine.js', 'plan.js', 'checks.js'].flatMap((f) =>
        fields(read(`scripts/lib/deploy-release/${f}`), 'config')
      ),
    ])
    used.delete('revision') // derived by the engine at plan time
    expect([...used].filter((k) => !CONFIG_KEYS.includes(k))).toEqual([])
  })

  it('the object loadConfig returns contains every checked key', () => {
    stage()
    const saved = { ...process.env }
    Object.assign(process.env, {
      FORGE_RELEASE_ROOT: p('release'),
      FORGE_PRIVATE_ROOT: p('private'),
      FORGE_RUNNER_IMAGE: 'forge-deploy-runner:test',
    })
    try {
      const cfg = loadConfig('deploy', ['--revision', REV])
      expect(CONFIG_KEYS.filter((k) => !(k in cfg) || cfg[k] === undefined)).toEqual([])
      expect(cfg.envFile).toBe(p('private/staged/deploy.env'))
    } finally {
      process.env = saved
    }
  })
})

describe('round 6, S2: image tags are global, so none is ever moved', () => {
  const container = (name: string, service: string, image: string) =>
    JSON.stringify([
      {
        Id: `${name}-id`,
        Name: `/${name}`,
        Created: '2026-11-24T00:00:00Z',
        Image: image,
        State: { Running: true },
        Mounts: [],
        NetworkSettings: { Networks: {} },
        HostConfig: { PortBindings: {} },
        Config: {
          Labels: {
            'com.docker.compose.project': 'cli-test',
            'com.docker.compose.service': service,
          },
          Env: [],
          Healthcheck: null,
        },
      },
    ])
  const rendered = JSON.stringify({
    name: 'cli-test',
    services: {
      'agentforge-server': {
        container_name: 'cli-test-server',
        image: 'agentforge-server:baseline-cli-test',
      },
      orchestrator: {
        container_name: 'cli-test-orchestrator',
        image: 'agentforge-orchestrator:baseline-cli-test',
      },
    },
  })
  const baselineRules = (serverTag: { out?: string; code?: number }) => [
    { match: '^compose .* config --format json$', out: rendered },
    {
      match: '^inspect cli-test-server$',
      out: container('cli-test-server', 'agentforge-server', 'sha256:live-server'),
    },
    {
      match: '^inspect cli-test-orchestrator$',
      out: container('cli-test-orchestrator', 'orchestrator', 'sha256:live-orch'),
    },
    {
      match: '^image inspect --format \\{\\{\\.Id\\}\\} agentforge-server:baseline-cli-test$',
      ...serverTag,
    },
    {
      match: '^image inspect --format \\{\\{\\.Id\\}\\} agentforge-orchestrator:baseline-cli-test$',
      out: 'sha256:live-orch\n',
    },
  ]
  const tags = () =>
    stub
      .calls()
      .filter((c) => c.argv[0] === 'tag')
      .map((c) => c.argv.slice(1).join(' '))
  const baselineDir = () => {
    fs.mkdirSync(p('release/releases/baseline/docker'), { recursive: true })
    fs.writeFileSync(p('release/releases/baseline/docker/compose.yml'), 'services: {}\n')
    fs.writeFileSync(p('release/releases/baseline/docker/compose.external.yml'), 'services: {}\n')
  }

  it('a: baseline tags are scoped by project, and the rendering uses that version', () => {
    stage()
    baselineDir()
    // Missing before `docker tag`, present afterwards.
    stub.rules([
      {
        match: '^image inspect --format \\{\\{\\.Id\\}\\} agentforge-server:baseline-cli-test$',
        code: 1,
        once: true,
      },
      ...baselineRules({ out: 'sha256:live-server\n' }),
    ])
    const r = cli('baseline')
    expect(r.out).toContain('baseline recorded')
    expect(r.code).toBe(0)
    expect(composeCalls().find((c) => c.argv.includes('config'))?.version).toBe('baseline-cli-test')
    expect(tags()).toEqual(['sha256:live-server agentforge-server:baseline-cli-test'])
  })

  it('b: baseline refuses (12) to move a tag that points at another image, and records nothing', () => {
    stage()
    baselineDir()
    stub.rules(baselineRules({ out: 'sha256:some-other-image\n' }))
    const r = cli('baseline')
    expect(r.code).toBe(12)
    expect(r.out).toMatch(
      /tag agentforge-server:baseline-cli-test already points at another image; refusing to move it/
    )
    expect(tags()).toEqual([])
    expect(fs.existsSync(p('private/state.json'))).toBe(false)
    expect(fs.existsSync(p('private/staged/deploy.env'))).toBe(true)
  })

  it('c: build adopts an existing revision tag and builds only what is missing; it never tags', () => {
    stage()
    stub.rules([
      {
        match: `^image inspect --format \\{\\{\\.Id\\}\\} agentforge-server:${REV}$`,
        out: 'sha256:shared-server\n',
      },
      {
        match: `^image inspect --format \\{\\{\\.Id\\}\\} agentforge-orchestrator:${REV}$`,
        code: 1,
      },
      {
        match: `^image inspect --format \\{\\{\\.Id\\}\\} agentforge-frontend:${REV}$`,
        out: 'sha256:shared-frontend\n',
      },
      { match: '^create ', out: 'frontend-cid\n' },
      { match: '^inspect frontend-cid', code: 1 },
    ])
    const r = cli('build', '--revision', REV)
    expect(r.code).toBe(0)
    expect(r.out).toMatch(/adopted existing images for agentforge-server/)
    const builds = composeCalls().filter((c) => c.argv.includes('build'))
    expect(builds.map((c) => c.argv.slice(c.argv.indexOf('build')))).toEqual([
      ['build', 'orchestrator'],
    ])
    expect(stub.calls().some((c) => c.argv[0] === 'build')).toBe(false)
    expect(tags()).toEqual([])
  })
})
