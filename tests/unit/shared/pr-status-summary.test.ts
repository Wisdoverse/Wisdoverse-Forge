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
  getMonitorSnapshotModeErrors,
  getRemoteReadProtectionErrors,
  isRepeatRemoteReadSuppressed,
  isReusableCacheEntry,
  isUsableCacheEntry,
  parseArgs,
} from '../../../scripts/pr-status-summary.mjs'

function pr(overrides: Record<string, unknown> = {}) {
  return {
    autoMergeRequest: { enabledAt: '2026-05-25T12:00:00Z' },
    headRefName: 'codex/example',
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

describe('PR status summary', () => {
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
      'WAIT: stop here; refresh only after cache expiry or a known remote change'
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
