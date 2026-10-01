import assert from 'node:assert/strict'
import { Buffer } from 'node:buffer'
import { execFileSync, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath, URL } from 'node:url'

const script = fileURLToPath(new URL('../check-secret-scan.mjs', import.meta.url))
const blockedHost = 'private.example.invalid'

function repository(t) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-secret-scan-'))
  t.after(() => fs.rmSync(cwd, { recursive: true, force: true }))
  execFileSync('git', ['init', '--quiet'], { cwd })
  return {
    write(file, content, tracked = true) {
      const target = path.join(cwd, file)
      fs.mkdirSync(path.dirname(target), { recursive: true })
      fs.writeFileSync(target, content)
      if (tracked) execFileSync('git', ['add', '--', file], { cwd })
    },
    remove(file) {
      fs.rmSync(path.join(cwd, file))
    },
    scan(blocklist = blockedHost) {
      const result = spawnSync(process.execPath, [script], {
        cwd,
        encoding: 'utf8',
        env: { ...process.env, INTERNAL_HOSTNAME_BLOCKLIST: blocklist },
      })
      assert.ifError(result.error)
      return { code: result.status, output: result.stdout + result.stderr }
    },
  }
}

test('blocks internal hosts in docs, instructions, examples, and test fixtures', (t) => {
  const repo = repository(t)
  const files = [
    'docs/guides/setup.md',
    'AGENTS.md',
    'docker/.env.example',
    'tests/unit/app/settings.test.tsx',
    'scripts/__tests__/fixture.mjs',
    'rust/crates/api/tests/fixtures/key.pem',
    'notes.custom',
  ]
  for (const file of files) repo.write(file, `https://${blockedHost}/setup\n`)
  const result = repo.scan()
  assert.equal(result.code, 1)
  for (const file of files) assert.ok(result.output.includes(`${file}:1:[internal-host]`))
  assert.ok(!result.output.includes(blockedHost), 'diagnostics must not repeat private values')
})

test('includes new unstaged files while leaving gitignored local files out', (t) => {
  const repo = repository(t)
  repo.write('.gitignore', 'local-only/\n')
  repo.write('docs/new.md', `https://${blockedHost}\n`, false)
  repo.write('local-only/private.md', `https://${blockedHost}\n`, false)
  const result = repo.scan()
  assert.equal(result.code, 1)
  assert.ok(result.output.includes('docs/new.md:1:[internal-host]'))
  assert.ok(!result.output.includes('local-only/private.md'))
})

test('a secret-manager placeholder cannot bypass the internal-host check', (t) => {
  const repo = repository(t)
  repo.write('docker/.env.example', `HOST=${blockedHost} # REPLACE_VIA_SECRET_MANAGER\n`)
  assert.equal(repo.scan().code, 1)
})

test('treats blocklist entries literally and blocks their subdomains', (t) => {
  const repo = repository(t)
  repo.write('README.md', 'https://privateXexampleXinvalid\n')
  assert.equal(repo.scan().code, 0)
  repo.write('README.md', `https://api.${blockedHost}\n`)
  assert.equal(repo.scan().code, 1)
})

test('checks high-confidence credential literals in documents and examples', (t) => {
  const repo = repository(t)
  const fakeKey = ['sk-', 'A'.repeat(24)].join('')
  for (const file of ['docs/setup.md', 'docker/.env.example', 'tests/unit/token.test.ts']) {
    repo.write(file, `API_KEY=${fakeKey}\n`)
  }
  const result = repo.scan('')
  assert.equal(result.code, 1)
  assert.ok(result.output.includes('docs/setup.md:1:[literal]'))
  assert.ok(result.output.includes('docker/.env.example:1:[literal]'))
  assert.ok(result.output.includes('tests/unit/token.test.ts:1:[literal]'))
  assert.ok(!result.output.includes(fakeKey))
  repo.write('README.md', `KEY=${fakeKey} # REPLACE_VIA_SECRET_MANAGER\n`)
  assert.ok(repo.scan('').output.includes('README.md:1:[literal]'))
})

test('preserves source assignment checks and rejects tracked environment files', (t) => {
  const repo = repository(t)
  repo.write('src/config.ts', "const password = 'unsafe-example-value'\n")
  repo.write('.env', 'SETTING=example\n')
  const result = repo.scan('')
  assert.equal(result.code, 1)
  assert.ok(result.output.includes('src/config.ts:1:[assignment]'))
  assert.ok(result.output.includes('.env'))
})

test('ignores binary files and deleted files, and permits public placeholders', (t) => {
  const repo = repository(t)
  repo.write('README.md', 'https://staging.example.com\ndev@example.com\n')
  repo.write('image.bin', Buffer.from(`\0${blockedHost}`))
  repo.write('deleted.md', blockedHost)
  repo.remove('deleted.md')
  repo.write('.env.deleted', 'SETTING=private\n')
  repo.remove('.env.deleted')
  assert.equal(repo.scan().code, 0)
  assert.equal(repo.scan('').code, 0)
})

test('rejects environment-specific files while allowing example variants', (t) => {
  const repo = repository(t)
  for (const file of ['.env.example', '.env.example.cn', 'docker/.env.sample', '.env.template']) {
    repo.write(file, 'SETTING=example\n')
  }
  assert.equal(repo.scan('').code, 0)
  for (const file of ['.env.staging', '.env.test', 'docker/.env.development.local']) {
    repo.write(file, 'SETTING=private\n')
  }
  const result = repo.scan('')
  assert.equal(result.code, 1)
  for (const file of ['.env.staging', '.env.test', 'docker/.env.development.local']) {
    assert.ok(result.output.includes(file))
  }
})
