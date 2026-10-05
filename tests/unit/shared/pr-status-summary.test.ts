import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  classifyPullRequest,
  renderSummary,
  summarizePullRequests,
} from '../../../scripts/lib/pr-status-summary.js'
import {
  CACHE_VERSION,
  cacheQuery,
  DEFAULT_MONITOR_CACHE_TTL_SECONDS,
  DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS,
  DEFAULT_REFRESH_COOLDOWN_SECONDS,
  formatCacheNotice,
  formatFreshSnapshotNotice,
  getLocalOnlyModeErrors,
  getMonitorSnapshotModeErrors,
  getRemoteReadProtectionErrors,
  isRepeatRemoteReadSuppressed,
  isReusableCacheEntry,
  isUsableCacheEntry,
  parseArgs,
  readPullRequestSnapshot,
} from '../../../scripts/pr-status-summary.mjs'

function pr(overrides: Record<string, unknown> = {}) {
  return {
    autoMergeRequest: { enabledAt: '2026-05-25T12:00:00Z' },
    headRefName: 'codex/example',
    headRefOid: 'A'.repeat(40),
    isDraft: false,
    mergeStateStatus: 'BLOCKED',
    number: 101,
    reviewDecision: 'REVIEW_REQUIRED',
    state: 'OPEN',
    statusCheckRollup: [
      {
        conclusion: '',
        name: 'Rust Tests',
        status: 'IN_PROGRESS',
      },
    ],
    title: 'Example PR',
    url: 'https://github.com/example/repo/pull/101',
    ...overrides,
  }
}

const summaryScript = fileURLToPath(
  new URL('../../../scripts/pr-status-summary.mjs', import.meta.url)
)

function runSummaryCli(
  cwd: string,
  args: string[],
  env: Record<string, string> = {},
  preloadUrl = ''
) {
  const cliArgs = preloadUrl
    ? ['--import', preloadUrl, summaryScript, ...args]
    : [summaryScript, ...args]

  return spawnSync(process.execPath, cliArgs, {
    cwd,
    encoding: 'utf8',
    timeout: 10_000,
    env: {
      LANG: 'C',
      LC_ALL: 'C',
      PATH: '',
      ...env,
    },
  })
}

function createGhPreload(cwd: string) {
  const preloadPath = join(cwd, 'fake-gh-preload.mjs')
  writeFileSync(
    preloadPath,
    `import childProcess from 'node:child_process'
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs'
import { syncBuiltinESMExports } from 'node:module'

childProcess.spawnSync = (command, args = []) => {
  if (command !== 'gh') throw new Error('unexpected child command')
  appendFileSync(process.env.GH_CALLS_FILE, 'call\\n')
  writeFileSync(process.env.GH_ARGS_FILE, JSON.stringify(args))
  return {
    status: 0,
    stdout: readFileSync(process.env.GH_RESPONSE_FILE, 'utf8'),
    stderr: '',
  }
}
syncBuiltinESMExports()
`
  )
  return pathToFileURL(preloadPath).href
}

describe('PR status summary', () => {
  it.each(['ACTION', 'WAIT', 'DONE'] as const)(
    'retains a normalized head SHA in %s items',
    (status) => {
      const input = pr({
        autoMergeRequest: status === 'ACTION' ? null : { enabledAt: '2026-05-25T12:00:00Z' },
        headRefOid: 'ABCDEF0123456789ABCDEF0123456789ABCDEF01',
        mergeStateStatus: status === 'ACTION' ? 'DIRTY' : 'BLOCKED',
        state: status === 'DONE' ? 'MERGED' : 'OPEN',
      })

      expect(classifyPullRequest(input)).toMatchObject({
        headSha: 'abcdef0123456789abcdef0123456789abcdef01',
        status,
      })
    }
  )

  it.each([
    ['missing', undefined],
    ['non-string', 123],
    ['short', 'a'.repeat(39)],
    ['non-hex', 'g'.repeat(40)],
    ['whitespace', `${'a'.repeat(39)} `],
  ])('returns an unknown head SHA for %s input', (_label, headRefOid) => {
    expect(classifyPullRequest(pr({ headRefOid })).headSha).toBeNull()
  })

  it('propagates the head SHA through the CLI input snapshot', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'pr-summary-input-'))
    const inputFile = join(tmp, 'prs.json')

    try {
      writeFileSync(inputFile, JSON.stringify([pr()]))
      const result = runSummaryCli(tmp, ['--input', inputFile, '--json'])

      expect(result.status).toBe(0)
      expect(JSON.parse(result.stdout).wait[0].headSha).toBe('a'.repeat(40))
    } finally {
      rmSync(tmp, { force: true, recursive: true })
    }
  })

  it('queries, caches, and replays the head SHA without a second GitHub read', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'pr-summary-gh-'))
    const cacheFile = join(tmp, 'cache.json')
    const callsFile = join(tmp, 'calls.txt')
    const argsFile = join(tmp, 'args.txt')
    const responseFile = join(tmp, 'response.json')
    const response = [pr()]

    try {
      const preloadUrl = createGhPreload(tmp)
      writeFileSync(responseFile, JSON.stringify(response))
      const env = {
        GH_ARGS_FILE: argsFile,
        GH_CALLS_FILE: callsFile,
        GH_RESPONSE_FILE: responseFile,
      }

      const queried = runSummaryCli(tmp, ['--cache-file', cacheFile, '--json'], env, preloadUrl)
      expect(queried.status).toBe(0)
      expect(JSON.parse(queried.stdout).wait[0].headSha).toBe('a'.repeat(40))
      const capturedArgs = JSON.parse(readFileSync(argsFile, 'utf8'))
      expect(capturedArgs.slice(0, 4)).toEqual(['pr', 'list', '--state', 'open'])
      expect(capturedArgs[capturedArgs.indexOf('--json') + 1].split(',')).toContain('headRefOid')

      const replayed = runSummaryCli(
        tmp,
        ['--cache-file', cacheFile, '--local-only', '--json'],
        env,
        preloadUrl
      )
      expect(replayed.status).toBe(0)
      expect(JSON.parse(replayed.stdout).wait[0].headSha).toBe('a'.repeat(40))
      expect(readFileSync(callsFile, 'utf8')).toBe('call\n')
    } finally {
      rmSync(tmp, { force: true, recursive: true })
    }
  })

  it('rejects an old field-list cache in local-only mode without calling GitHub', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'pr-summary-old-cache-'))
    const cacheFile = join(tmp, 'cache.json')
    const callsFile = join(tmp, 'calls.txt')
    const argsFile = join(tmp, 'args.txt')
    const responseFile = join(tmp, 'response.json')
    const options = parseArgs([])
    const oldQuery = {
      ...cacheQuery(options),
      fields: cacheQuery(options).fields.replace(',headRefOid', ''),
    }

    try {
      const preloadUrl = createGhPreload(tmp)
      writeFileSync(responseFile, JSON.stringify([pr()]))
      writeFileSync(
        cacheFile,
        `${JSON.stringify({
          version: CACHE_VERSION,
          fetchedAt: Date.now(),
          query: oldQuery,
          pullRequests: [pr()],
        })}\n`
      )
      const result = runSummaryCli(
        tmp,
        ['--cache-file', cacheFile, '--local-only', '--json'],
        {
          GH_ARGS_FILE: argsFile,
          GH_CALLS_FILE: callsFile,
          GH_RESPONSE_FILE: responseFile,
        },
        preloadUrl
      )

      expect(result.status).toBe(2)
      expect(result.stderr).toContain('local PR snapshot does not match this query')
      expect(existsSync(callsFile)).toBe(false)
    } finally {
      rmSync(tmp, { force: true, recursive: true })
    }
  })

  it('keeps review-required and pending-check PRs in WAIT when auto-merge is enabled', () => {
    const item = classifyPullRequest(pr())

    expect(item.status).toBe('WAIT')
    expect(item.reasons).toContain('waiting for review')
    expect(item.reasons).toContain('pending check: Rust Tests')
  })

  it('marks failed checks as ACTION', () => {
    const item = classifyPullRequest(
      pr({
        reviewDecision: 'APPROVED',
        statusCheckRollup: [
          {
            conclusion: 'FAILURE',
            name: 'Unit Tests',
            status: 'COMPLETED',
          },
        ],
      })
    )

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('failing check: Unit Tests')
  })

  it('ignores stale failed check reruns when a later check with the same name passed', () => {
    const item = classifyPullRequest(
      pr({
        statusCheckRollup: [
          {
            conclusion: 'FAILURE',
            name: 'Beginner UX / First-Time User Path',
            status: 'COMPLETED',
          },
          {
            conclusion: 'SUCCESS',
            name: 'Beginner UX / First-Time User Path',
            status: 'COMPLETED',
          },
          {
            conclusion: '',
            name: 'Rust Tests',
            status: 'IN_PROGRESS',
          },
        ],
      })
    )

    expect(item.status).toBe('WAIT')
    expect(item.failedChecks).toEqual([])
    expect(item.reasons).toContain('pending check: Rust Tests')
  })

  it('marks missing auto-merge as ACTION for open PRs', () => {
    const item = classifyPullRequest(pr({ autoMergeRequest: null, statusCheckRollup: [] }))

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('auto-merge is not enabled')
  })

  it('marks merge conflicts as ACTION', () => {
    const item = classifyPullRequest(pr({ mergeStateStatus: 'DIRTY', reviewDecision: 'APPROVED' }))

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('merge conflict')
  })

  it('marks closed PRs as DONE', () => {
    const item = classifyPullRequest(pr({ state: 'MERGED' }))

    expect(item.status).toBe('DONE')
  })

  it('renders compact output without listing every waiting PR by default', () => {
    const summary = summarizePullRequests([
      pr(),
      pr({
        autoMergeRequest: null,
        number: 102,
        statusCheckRollup: [],
        title: 'Needs auto-merge',
      }),
    ])

    expect(renderSummary(summary)).toContain('ACTION 1 | WAIT 1 | DONE 0')
    expect(renderSummary(summary)).toContain('WAIT: 1 PR(s) waiting')
    expect(renderSummary(summary)).toContain(
      'WAIT: stop here; use npm run pr:summary:local until cache expiry or a known remote change'
    )
    expect(renderSummary(summary)).toContain(
      'WAIT: token-safe action: do not poll in chat; use scheduled monitoring for the next check'
    )
    expect(renderSummary(summary)).not.toContain('#101 codex/example')
  })

  it('defaults to a short-lived cache and supports explicit refresh', () => {
    expect(parseArgs([])).toMatchObject({
      cacheTtlSeconds: 900,
      allowRepeatRemoteRead: false,
      forceRefresh: false,
      localOnly: false,
      minRemoteReadIntervalSeconds: DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS,
      monitor: false,
      noCache: false,
      refresh: false,
      refreshCooldownSeconds: DEFAULT_REFRESH_COOLDOWN_SECONDS,
    })

    expect(parseArgs(['--refresh', '--cache-ttl-seconds', '300'])).toMatchObject({
      cacheTtlSeconds: 300,
      refresh: true,
    })

    expect(parseArgs(['--force-refresh'])).toMatchObject({
      forceRefresh: true,
      refresh: true,
    })
  })

  it('keeps local-only checks from reading GitHub', () => {
    const options = parseArgs(['--local-only'])

    expect(options).toMatchObject({
      localOnly: true,
      noCache: false,
      refresh: false,
    })
    expect(getLocalOnlyModeErrors(options)).toEqual([])
    expect(getLocalOnlyModeErrors({ ...options, refresh: true })).toContain(
      '--local-only cannot use refresh flags because it must never read GitHub.'
    )
    expect(getLocalOnlyModeErrors({ ...options, noCache: true })).toContain(
      '--local-only cannot use --no-cache because it only reads the local snapshot.'
    )
    expect(getLocalOnlyModeErrors({ ...options, allowRepeatRemoteRead: true })).toContain(
      '--local-only cannot use --allow-repeat-remote-read because no remote read is allowed.'
    )
  })

  it('uses stale cache in local-only mode instead of refreshing remotely', () => {
    const now = Date.parse('2026-06-05T12:00:00Z')
    const tmp = mkdtempSync(join(tmpdir(), 'pr-summary-'))
    const cacheFile = join(tmp, 'cache.json')
    const options = parseArgs([
      '--local-only',
      '--cache-file',
      cacheFile,
      '--cache-ttl-seconds',
      '60',
    ])

    try {
      writeFileSync(
        cacheFile,
        `${JSON.stringify({
          version: CACHE_VERSION,
          fetchedAt: now - 3_600_000,
          query: cacheQuery(options),
          pullRequests: [pr()],
        })}\n`
      )

      const snapshot = readPullRequestSnapshot(options, now)

      expect(snapshot).toMatchObject({
        cacheHit: true,
        cacheAgeSeconds: 3600,
        localOnly: true,
        localOnlyStale: true,
        pullRequests: [expect.objectContaining({ number: 101 })],
      })
      expect(formatCacheNotice(snapshot)).toContain('no remote read was made')
    } finally {
      rmSync(tmp, { force: true, recursive: true })
    }
  })

  it('uses snapshot-only defaults for monitor mode', () => {
    const options = parseArgs(['--monitor'])

    expect(options).toMatchObject({
      cacheTtlSeconds: DEFAULT_MONITOR_CACHE_TTL_SECONDS,
      failOnAction: true,
      minRemoteReadIntervalSeconds: DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS,
      monitor: true,
      noCache: false,
      refresh: false,
    })
    expect(getMonitorSnapshotModeErrors(options)).toEqual([])
  })

  it('rejects refresh bypasses in monitor mode', () => {
    const options = parseArgs(['--monitor'])

    expect(getMonitorSnapshotModeErrors({ ...options, refresh: true })).toContain(
      '--monitor cannot use refresh flags; it must let the cache decide when to read GitHub.'
    )
    expect(getMonitorSnapshotModeErrors({ ...options, noCache: true })).toContain(
      '--monitor cannot use --no-cache because monitoring must keep repeat-read protection.'
    )
    expect(getMonitorSnapshotModeErrors({ ...options, allowRepeatRemoteRead: true })).toContain(
      '--monitor cannot use --allow-repeat-remote-read because monitoring must not bypass the guard.'
    )
    expect(getMonitorSnapshotModeErrors({ ...options, cacheTtlSeconds: 300 })).toContain(
      '--monitor requires --cache-ttl-seconds >= 3600 to avoid frequent remote checks.'
    )
    expect(getMonitorSnapshotModeErrors({ ...options, minRemoteReadIntervalSeconds: 0 })).toContain(
      '--monitor requires --min-remote-read-interval-seconds >= 60.'
    )
  })

  it('keeps monitor snapshots on an hourly floor without overriding explicit longer windows', () => {
    const monitorOptions = parseArgs(['--monitor'])

    expect(monitorOptions.cacheTtlSeconds).toBe(DEFAULT_MONITOR_CACHE_TTL_SECONDS)
    expect(parseArgs(['--cache-ttl-seconds', '7200', '--monitor']).cacheTtlSeconds).toBe(7200)
    expect(getMonitorSnapshotModeErrors({ ...monitorOptions, cacheTtlSeconds: 1800 })).toContain(
      '--monitor requires --cache-ttl-seconds >= 3600 to avoid frequent remote checks.'
    )
  })

  it('rejects repeated remote reads unless the operator makes a one-time bypass explicit', () => {
    const options = parseArgs([])

    expect(
      getRemoteReadProtectionErrors({ ...options, minRemoteReadIntervalSeconds: 0 })
    ).toContain(
      '--min-remote-read-interval-seconds must be >= 60; pass --allow-repeat-remote-read only for a one-time manual check.'
    )
    expect(
      getRemoteReadProtectionErrors({
        ...options,
        allowRepeatRemoteRead: true,
        minRemoteReadIntervalSeconds: 0,
      })
    ).toEqual([])
    expect(
      getRemoteReadProtectionErrors({
        ...options,
        inputPath: '/tmp/prs.json',
        minRemoteReadIntervalSeconds: 0,
      })
    ).toEqual([])
  })

  it('keeps a repeat-read guard even for forced refreshes', () => {
    const now = Date.parse('2026-06-05T12:00:00Z')
    const options = parseArgs(['--force-refresh'])
    const entry = {
      version: CACHE_VERSION,
      fetchedAt: now - 30_000,
      query: cacheQuery(options),
      pullRequests: [pr()],
    }

    expect(isRepeatRemoteReadSuppressed(entry, options, now)).toBe(true)
    expect(isRepeatRemoteReadSuppressed({ ...entry, fetchedAt: now - 61_000 }, options, now)).toBe(
      false
    )
    expect(
      isRepeatRemoteReadSuppressed(
        entry,
        parseArgs(['--force-refresh', '--allow-repeat-remote-read']),
        now
      )
    ).toBe(false)
  })

  it('tells operators when a repeated remote read will be useful again', () => {
    expect(
      formatCacheNotice({
        cacheAgeSeconds: 30,
        cacheHit: true,
        pullRequests: [],
        remoteReadGuardRemainingSeconds: 30,
        repeatRemoteReadSuppressed: true,
        source: 'cache',
      })
    ).toContain('next remote read is allowed in 30s')

    expect(
      formatCacheNotice({
        cacheAgeSeconds: 20,
        cacheHit: true,
        pullRequests: [],
        refreshCooldownRemainingSeconds: 40,
        refreshSuppressed: true,
        source: 'cache',
      })
    ).toContain('try again in 40s')

    expect(
      formatCacheNotice({
        cacheAgeSeconds: 120,
        cacheHit: true,
        cacheTtlRemainingSeconds: 780,
        pullRequests: [],
        source: 'cache',
      })
    ).toContain('it expires in 13m')
  })

  it('tells operators how to reuse a fresh snapshot instead of polling again', () => {
    expect(formatFreshSnapshotNotice(parseArgs([]))).toBe(
      '[pr-summary] fresh GitHub snapshot saved; use npm run pr:summary:local or cached npm run pr:summary for the next 15m; repeat remote reads are blocked for 1m'
    )
    expect(formatFreshSnapshotNotice(parseArgs(['--cache-ttl-seconds', '0']))).toBe(
      '[pr-summary] fresh GitHub snapshot saved, but cache reuse is disabled; repeat remote reads are still guarded for 1m'
    )
    expect(formatFreshSnapshotNotice(parseArgs(['--no-cache', '--allow-repeat-remote-read']))).toBe(
      '[pr-summary] fresh GitHub read completed with --no-cache; no snapshot was saved, so do not use this in loops'
    )
  })

  it('reuses only fresh cache entries for the same GitHub query', () => {
    const now = Date.parse('2026-06-05T12:00:00Z')
    const options = parseArgs(['--limit', '5'])
    const entry = {
      version: CACHE_VERSION,
      fetchedAt: now - 30_000,
      query: cacheQuery(options),
      pullRequests: [pr()],
    }

    expect(isUsableCacheEntry(entry, options, now)).toBe(true)
    expect(isUsableCacheEntry({ ...entry, fetchedAt: now - 901_000 }, options, now)).toBe(false)
    expect(
      isUsableCacheEntry({ ...entry, query: { ...cacheQuery(options), limit: 6 } }, options, now)
    ).toBe(false)
  })

  it('reuses very recent cache entries when refresh is requested repeatedly', () => {
    const now = Date.parse('2026-06-05T12:00:00Z')
    const options = parseArgs(['--refresh'])
    const entry = {
      version: CACHE_VERSION,
      fetchedAt: now - 30_000,
      query: cacheQuery(options),
      pullRequests: [pr()],
    }

    expect(isReusableCacheEntry(entry, options, now)).toBe(true)
    expect(isReusableCacheEntry({ ...entry, fetchedAt: now - 61_000 }, options, now)).toBe(false)
    expect(isReusableCacheEntry(entry, parseArgs(['--force-refresh']), now)).toBe(false)
  })
})
