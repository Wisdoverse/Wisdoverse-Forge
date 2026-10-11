import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { URL } from 'node:url'

const source = readFileSync(
  new URL('../../.github/workflows/release-supply-chain.yml', import.meta.url),
  'utf8'
)
const step = source.split('      - name: Validate signing ref\n')[1]?.split('\n      - ')[0]
assert.ok(step, 'The signing ref guard must exist')
const script = step.split('        run: |\n')[1]?.replace(/^ {10}/gm, '')
assert.ok(script, 'The signing ref guard must contain a shell command')

test('signing rejects incorrect invocation refs before checkout or upload', () => {
  assert.ok(
    source.indexOf('      - name: Validate signing ref') <
      source.indexOf('      - uses: actions/checkout')
  )
  assert.ok(source.indexOf('cosign verify-blob') < source.indexOf('gh release upload'))

  const directory = mkdtempSync(join(tmpdir(), 'release-signing-ref-'))
  const repository = 'example/forge'
  const tag = 'v1.2.3'
  const ref = `refs/tags/${tag}`
  const workflow = `${repository}/.github/workflows/release-supply-chain.yml@${ref}`
  const cases = [
    { name: 'matching release tag', tag, ref, workflow, success: true },
    { name: 'branch dispatch', tag, ref: 'refs/heads/main', workflow, success: false },
    { name: 'different tag', tag, ref: 'refs/tags/v1.2.4', workflow, success: false },
    {
      name: 'workflow loaded from a branch',
      tag,
      ref,
      workflow: `${repository}/.github/workflows/release-supply-chain.yml@refs/heads/main`,
      success: false,
    },
    { name: 'empty tag', tag: '', ref, workflow, success: false },
    { name: 'invalid tag', tag: 'v1.2.3\nother', ref, workflow, success: false },
  ]

  try {
    for (const fixture of cases) {
      const result = spawnSync('bash', ['-c', script], {
        encoding: 'utf8',
        env: {
          ...process.env,
          RELEASE_TAG: fixture.tag,
          GITHUB_REPOSITORY: repository,
          GITHUB_REF: fixture.ref,
          GITHUB_WORKFLOW_REF: fixture.workflow,
          GITHUB_STEP_SUMMARY: join(directory, 'summary'),
        },
      })
      assert.ifError(result.error)
      assert.equal(result.status === 0, fixture.success, fixture.name)
      if (!fixture.success) assert.match(result.stdout, /::error::/, fixture.name)
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
