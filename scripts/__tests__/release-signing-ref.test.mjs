import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { test } from 'node:test'
import { URL } from 'node:url'

const source = readFileSync(
  new URL('../../.github/workflows/release-supply-chain.yml', import.meta.url),
  'utf8'
)
const releaseSource = readFileSync(
  new URL('../../.github/workflows/release.yml', import.meta.url),
  'utf8'
)

function stepCommand(workflow, name) {
  const step = workflow.split(`      - name: ${name}\n`)[1]?.split('\n      - ')[0]
  assert.ok(step, `The ${name} step must exist`)
  const command = step.split('        run: |\n')[1]
  assert.ok(command, `The ${name} step must contain a shell command`)
  const lines = command.split('\n')
  const end = lines.findIndex((line) => line.trim() && !line.startsWith('          '))
  return lines
    .slice(0, end === -1 ? undefined : end)
    .join('\n')
    .replace(/^ {10}/gm, '')
}

const script = stepCommand(source, 'Validate signing ref')

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

test('release preflight rejects existing tags and propagates remote errors', () => {
  const command = stepCommand(releaseSource, 'Require unused release tag')
  const directory = mkdtempSync(join(tmpdir(), 'release-tag-preflight-'))
  const repository = join(directory, 'repository')
  const setup = [
    ['init', '--initial-branch=main', repository],
    ['-C', repository, 'config', 'user.name', 'Fixture'],
    ['-C', repository, 'config', 'user.email', 'dev@example.com'],
    ['-C', repository, 'commit', '--allow-empty', '-m', 'Fixture'],
    ['-C', repository, 'tag', 'v1.2.3'],
    ['-C', repository, 'tag', '-a', 'v1.2.4', '-m', 'Fixture'],
    ['-C', repository, 'remote', 'add', 'origin', repository],
  ]

  try {
    for (const args of setup) {
      const result = spawnSync('git', args, { encoding: 'utf8' })
      assert.ifError(result.error)
      assert.equal(result.status, 0, result.stderr)
    }

    for (const fixture of [
      { tag: 'v1.2.5-rc.1', status: 0 },
      { tag: 'v1.2.3', status: 1 },
      { tag: 'v1.2.4', status: 1 },
    ]) {
      const result = spawnSync('bash', ['-c', command], {
        cwd: repository,
        encoding: 'utf8',
        env: { ...process.env, RELEASE_TAG: fixture.tag },
      })
      assert.ifError(result.error)
      assert.equal(result.status, fixture.status, fixture.tag)
      if (fixture.status === 1) assert.match(result.stdout, /::error::/)
    }

    const remote = spawnSync('git', ['remote', 'set-url', 'origin', join(directory, 'missing')], {
      cwd: repository,
      encoding: 'utf8',
    })
    assert.ifError(remote.error)
    assert.equal(remote.status, 0, remote.stderr)
    const result = spawnSync('bash', ['-c', command], {
      cwd: repository,
      encoding: 'utf8',
      env: { ...process.env, RELEASE_TAG: 'v1.2.5-rc.1' },
    })
    assert.ifError(result.error)
    assert.equal(result.status, 128, 'An unreadable remote must prevent release publication')
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

test('published release dispatches signing from its tag and propagates dispatch failures', () => {
  const job = releaseSource.split('\n  github-release:\n')[1]?.split('\n  notify:\n')[0]
  assert.ok(job)
  assert.ok(job.includes('actions: write'))
  assert.ok(job.includes('target_commitish: ${{ github.sha }}'))
  assert.ok(job.includes('RELEASE_TAG: v${{ needs.validate.outputs.version }}'))
  assert.ok(
    job.indexOf('Create release tag from validated source') <
      job.indexOf('softprops/action-gh-release')
  )
  assert.ok(
    job.indexOf('Dispatch tag-bound release signing') > job.indexOf('softprops/action-gh-release')
  )

  const command = [
    stepCommand(releaseSource, 'Create release tag from validated source'),
    stepCommand(releaseSource, 'Dispatch tag-bound release signing'),
  ].join('\n')
  const directory = mkdtempSync(join(tmpdir(), 'release-signing-dispatch-'))
  const argumentsPath = join(directory, 'arguments')
  const ghPath = join(directory, 'gh')
  writeFileSync(
    ghPath,
    '#!/bin/sh\nprintf "CALL\\n" >> "$GH_ARGUMENTS"\nprintf "%s\\n" "$@" >> "$GH_ARGUMENTS"\n' +
      'if [ "$1" = api ]; then exit "$GH_API_EXIT"; fi\nexit "$GH_DISPATCH_EXIT"\n'
  )
  chmodSync(ghPath, 0o755)

  try {
    for (const fixture of [
      { tag: 'v1.2.3', apiStatus: 0, status: 0 },
      { tag: 'v1.2.4-rc.1', apiStatus: 0, status: 0 },
      { tag: 'v1.2.3', apiStatus: 1, status: 1 },
      { tag: 'v1.2.3', apiStatus: 0, status: 23 },
    ]) {
      writeFileSync(argumentsPath, '')
      const sha = 'a'.repeat(40)
      const result = spawnSync('bash', ['-c', command], {
        encoding: 'utf8',
        env: {
          ...process.env,
          PATH: `${directory}${delimiter}${process.env.PATH}`,
          GH_TOKEN: 'fixture-token',
          GITHUB_REPOSITORY: 'example/forge',
          GITHUB_SHA: sha,
          RELEASE_TAG: fixture.tag,
          GH_ARGUMENTS: argumentsPath,
          GH_API_EXIT: String(fixture.apiStatus),
          GH_DISPATCH_EXIT: String(fixture.status),
        },
      })
      assert.ifError(result.error)
      assert.equal(result.status, fixture.status)
      assert.deepEqual(readFileSync(argumentsPath, 'utf8').trim().split('\n'), [
        'CALL',
        'api',
        '--method',
        'POST',
        'repos/example/forge/git/refs',
        '--raw-field',
        `ref=refs/tags/${fixture.tag}`,
        '--raw-field',
        `sha=${sha}`,
        ...(fixture.apiStatus === 0
          ? [
              'CALL',
              'workflow',
              'run',
              'release-supply-chain.yml',
              '--repo',
              'example/forge',
              '--ref',
              fixture.tag,
              '--raw-field',
              `tag=${fixture.tag}`,
            ]
          : []),
      ])
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
