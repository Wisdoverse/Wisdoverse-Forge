#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import {
  classifyPullRequest,
  renderSummary,
  summarizePullRequests,
} from './lib/pr-status-summary.js'

const CACHE_VERSION = 1
const DEFAULT_CACHE_TTL_SECONDS = 900
const DEFAULT_MONITOR_CACHE_TTL_SECONDS = 3600
const DEFAULT_REFRESH_COOLDOWN_SECONDS = 60
const DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS = 60
const MIN_MONITOR_CACHE_TTL_SECONDS = DEFAULT_MONITOR_CACHE_TTL_SECONDS

const GH_FIELDS = [
  'autoMergeRequest',
  'headRefName',
  'isDraft',
  'mergeStateStatus',
  'number',
  'reviewDecision',
  'state',
  'statusCheckRollup',
  'title',
  'url',
].join(',')

function main() {
  const options = parseArgs(process.argv.slice(2))
  const snapshot = readPullRequestSnapshot(options)
  const pullRequests = snapshot.pullRequests
  const summary = summarizePullRequests(pullRequests)

  if (snapshot.cacheHit) {
    console.error(formatCacheNotice(snapshot))
  } else if (snapshot.source === 'GitHub') {
    console.error(formatFreshSnapshotNotice(options))
  }

  if (options.json) {
    console.info(JSON.stringify(summary, null, 2))
  } else {
    console.info(renderSummary(summary, { showWait: options.showWait }).trimEnd())
  }

  if (options.failOnAction && summary.action.length > 0) {
    process.exit(1)
  }
}

function parseArgs(args) {
  let cacheTtlSecondsExplicit = false
  const options = {
    cacheFile: '',
    cacheTtlSeconds: DEFAULT_CACHE_TTL_SECONDS,
    allowRepeatRemoteRead: false,
    failOnAction: false,
    forceRefresh: false,
    inputPath: '',
    json: false,
    limit: 120,
    localOnly: false,
    minRemoteReadIntervalSeconds: DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS,
    monitor: false,
    noCache: false,
    refresh: false,
    refreshCooldownSeconds: DEFAULT_REFRESH_COOLDOWN_SECONDS,
    showWait: false,
    state: 'open',
  }

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index]
    if (arg === '--show-wait') {
      options.showWait = true
    } else if (arg === '--json') {
      options.json = true
    } else if (arg === '--fail-on-action') {
      options.failOnAction = true
    } else if (arg === '--monitor') {
      options.monitor = true
      options.failOnAction = true
    } else if (arg === '--local-only') {
      options.localOnly = true
    } else if (arg === '--refresh') {
      options.refresh = true
    } else if (arg === '--force-refresh') {
      options.refresh = true
      options.forceRefresh = true
    } else if (arg === '--no-cache') {
      options.noCache = true
    } else if (arg === '--allow-repeat-remote-read') {
      options.allowRepeatRemoteRead = true
    } else if (arg === '--limit') {
      options.limit = parsePositiveInt(readValue(args, index, arg), options.limit)
      index += 1
    } else if (arg === '--cache-ttl-seconds') {
      cacheTtlSecondsExplicit = true
      options.cacheTtlSeconds = parseNonNegativeInt(
        readValue(args, index, arg),
        options.cacheTtlSeconds
      )
      index += 1
    } else if (arg === '--refresh-cooldown-seconds') {
      options.refreshCooldownSeconds = parseNonNegativeInt(
        readValue(args, index, arg),
        options.refreshCooldownSeconds
      )
      index += 1
    } else if (arg === '--min-remote-read-interval-seconds') {
      options.minRemoteReadIntervalSeconds = parseNonNegativeInt(
        readValue(args, index, arg),
        options.minRemoteReadIntervalSeconds
      )
      index += 1
    } else if (arg === '--cache-file') {
      options.cacheFile = readValue(args, index, arg)
      index += 1
    } else if (arg === '--state') {
      options.state = parseState(readValue(args, index, arg))
      index += 1
    } else if (arg === '--input') {
      options.inputPath = readValue(args, index, arg)
      index += 1
    } else if (arg === '--help' || arg === '-h') {
      printHelp()
      process.exit(0)
    } else {
      throwUsageError(`unknown option: ${arg}`)
    }
  }

  if (options.monitor && !cacheTtlSecondsExplicit) {
    options.cacheTtlSeconds = DEFAULT_MONITOR_CACHE_TTL_SECONDS
  }

  enforceMonitorSnapshotMode(options)
  enforceLocalOnlyMode(options)
  enforceRemoteReadProtection(options)

  return options
}

function enforceLocalOnlyMode(options) {
  const errors = getLocalOnlyModeErrors(options)
  if (errors.length > 0) {
    throwUsageError(errors.join(' '))
  }
}

function getLocalOnlyModeErrors(options) {
  if (!options.localOnly) return []

  const errors = []
  if (options.refresh || options.forceRefresh) {
    errors.push('--local-only cannot use refresh flags because it must never read GitHub.')
  }
  if (options.noCache) {
    errors.push('--local-only cannot use --no-cache because it only reads the local snapshot.')
  }
  if (options.allowRepeatRemoteRead) {
    errors.push(
      '--local-only cannot use --allow-repeat-remote-read because no remote read is allowed.'
    )
  }
  return errors
}

function enforceRemoteReadProtection(options) {
  const errors = getRemoteReadProtectionErrors(options)
  if (errors.length > 0) {
    throwUsageError(errors.join(' '))
  }
}

function getRemoteReadProtectionErrors(options) {
  if (options.inputPath || options.allowRepeatRemoteRead) return []

  const errors = []
  if (options.minRemoteReadIntervalSeconds < DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS) {
    errors.push(
      `--min-remote-read-interval-seconds must be >= ${DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS}; pass --allow-repeat-remote-read only for a one-time manual check.`
    )
  }
  return errors
}

function enforceMonitorSnapshotMode(options) {
  const errors = getMonitorSnapshotModeErrors(options)
  if (errors.length > 0) {
    throwUsageError(errors.join(' '))
  }
}

function getMonitorSnapshotModeErrors(options) {
  if (!options.monitor) return []

  const errors = []
  if (options.refresh || options.forceRefresh) {
    errors.push(
      '--monitor cannot use refresh flags; it must let the cache decide when to read GitHub.'
    )
  }
  if (options.noCache) {
    errors.push(
      '--monitor cannot use --no-cache because monitoring must keep repeat-read protection.'
    )
  }
  if (options.allowRepeatRemoteRead) {
    errors.push(
      '--monitor cannot use --allow-repeat-remote-read because monitoring must not bypass the guard.'
    )
  }
  if (options.cacheTtlSeconds < MIN_MONITOR_CACHE_TTL_SECONDS) {
    errors.push(
      `--monitor requires --cache-ttl-seconds >= ${MIN_MONITOR_CACHE_TTL_SECONDS} to avoid frequent remote checks.`
    )
  }
  if (options.minRemoteReadIntervalSeconds < DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS) {
    errors.push(
      `--monitor requires --min-remote-read-interval-seconds >= ${DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS}.`
    )
  }
  return errors
}

function readPullRequestSnapshot(options, now = Date.now()) {
  if (options.inputPath) {
    return {
      pullRequests: readPullRequestsFromFile(options.inputPath),
      source: options.inputPath,
      cacheHit: false,
    }
  }

  const cachePath = resolveCachePath(options.cacheFile)
  if (options.localOnly) {
    return readLocalOnlyCache(cachePath, options, now)
  }

  if (options.noCache && !options.allowRepeatRemoteRead) {
    throwUsageError(
      '--no-cache disables repeat-read protection; pass --allow-repeat-remote-read only for a one-time manual check'
    )
  }

  if (!options.noCache) {
    const cached = readFreshCache(cachePath, options, now)
    if (cached) return cached
  }

  const guarded = readRepeatRemoteReadGuard(cachePath, options, now)
  if (guarded) return guarded

  const pullRequests = readPullRequestsFromGitHub(options)
  if (!options.noCache) {
    writeCache(cachePath, options, pullRequests, now)
  }

  return {
    pullRequests,
    source: 'GitHub',
    cacheHit: false,
  }
}

function readPullRequestsFromGitHub(options) {
  const result = spawnSync(
    'gh',
    ['pr', 'list', '--state', options.state, '--limit', String(options.limit), '--json', GH_FIELDS],
    { encoding: 'utf8' }
  )

  if (result.error) {
    throwUsageError(`unable to run gh: ${result.error.message}`)
  }
  if (result.status !== 0) {
    throwUsageError(result.stderr.trim() || 'gh pr list failed')
  }

  return parsePullRequestJson(result.stdout, 'gh pr list')
}

function readPullRequestsFromFile(inputPath) {
  return parsePullRequestJson(readFileSync(inputPath, 'utf8'), inputPath)
}

function readFreshCache(cachePath, options, now) {
  if (!existsSync(cachePath)) return null

  try {
    const cache = JSON.parse(readFileSync(cachePath, 'utf8'))
    if (!isReusableCacheEntry(cache, options, now)) return null
    return {
      pullRequests: cache.pullRequests,
      source: cachePath,
      cacheHit: true,
      cacheAgeSeconds: Math.max(0, Math.floor((now - cache.fetchedAt) / 1000)),
      cacheTtlRemainingSeconds: secondsRemaining(cache.fetchedAt, options.cacheTtlSeconds, now),
      refreshCooldownRemainingSeconds: secondsRemaining(
        cache.fetchedAt,
        options.refreshCooldownSeconds,
        now
      ),
      refreshSuppressed: options.refresh === true && options.forceRefresh !== true,
    }
  } catch {
    return null
  }
}

function readRepeatRemoteReadGuard(cachePath, options, now) {
  if (!existsSync(cachePath)) return null

  try {
    const cache = JSON.parse(readFileSync(cachePath, 'utf8'))
    if (!isRepeatRemoteReadSuppressed(cache, options, now)) return null
    return {
      pullRequests: cache.pullRequests,
      source: cachePath,
      cacheHit: true,
      cacheAgeSeconds: Math.max(0, Math.floor((now - cache.fetchedAt) / 1000)),
      remoteReadGuardRemainingSeconds: secondsRemaining(
        cache.fetchedAt,
        options.minRemoteReadIntervalSeconds,
        now
      ),
      repeatRemoteReadSuppressed: true,
    }
  } catch {
    return null
  }
}

function readLocalOnlyCache(cachePath, options, now) {
  if (!existsSync(cachePath)) {
    throwUsageError(
      'no local PR snapshot found; run npm run pr:summary:refresh once when a fresh remote read is acceptable'
    )
  }

  try {
    const cache = JSON.parse(readFileSync(cachePath, 'utf8'))
    if (!isMatchingCacheEntry(cache, options)) {
      throwUsageError(
        'local PR snapshot does not match this query; run npm run pr:summary:refresh once when a fresh remote read is acceptable'
      )
    }
    return {
      pullRequests: cache.pullRequests,
      source: cachePath,
      cacheHit: true,
      cacheAgeSeconds: Math.max(0, Math.floor((now - cache.fetchedAt) / 1000)),
      localOnly: true,
      localOnlyStale: !isUsableCacheEntry(cache, options, now),
    }
  } catch (error) {
    if (error?.code === undefined && error?.message?.startsWith?.('[pr-summary]')) {
      throw error
    }
    const message = error instanceof Error ? error.message : String(error)
    throwUsageError(`unable to read local PR snapshot: ${message}`)
  }
}

function writeCache(cachePath, options, pullRequests, now) {
  try {
    mkdirSync(dirname(cachePath), { recursive: true })
    writeFileSync(
      cachePath,
      `${JSON.stringify(
        {
          version: CACHE_VERSION,
          fetchedAt: now,
          query: cacheQuery(options),
          pullRequests,
        },
        null,
        2
      )}\n`
    )
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error(`[pr-summary] cache write skipped: ${message}`)
  }
}

function parsePullRequestJson(payload, source) {
  try {
    const parsed = JSON.parse(payload)
    if (!Array.isArray(parsed)) {
      throw new Error('expected a JSON array')
    }
    return parsed
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throwUsageError(`invalid PR JSON from ${source}: ${message}`)
  }
}

function parsePositiveInt(value, fallback) {
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function parseNonNegativeInt(value, fallback) {
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback
}

function parseState(value) {
  if (value === 'open' || value === 'closed' || value === 'all') return value
  throwUsageError('--state must be open, closed, or all')
}

function readValue(args, index, flag) {
  const value = args[index + 1]
  if (!value || value.startsWith('--')) {
    throwUsageError(`${flag} requires a value`)
  }
  return value
}

function resolveCachePath(cacheFile) {
  if (cacheFile) return cacheFile

  const result = spawnSync('git', ['rev-parse', '--git-path', 'pr-status-summary-cache.json'], {
    encoding: 'utf8',
  })
  if (!result.error && result.status === 0) {
    const gitPath = result.stdout.trim()
    if (gitPath) return gitPath
  }

  return join(process.cwd(), '.pr-status-summary-cache.json')
}

function isUsableCacheEntry(cache, options, now = Date.now()) {
  if (!isMatchingCacheEntry(cache, options)) return false
  if (options.cacheTtlSeconds <= 0) return false
  if (now - cache.fetchedAt > options.cacheTtlSeconds * 1000) return false
  return true
}

function isReusableCacheEntry(cache, options, now = Date.now()) {
  if (!isMatchingCacheEntry(cache, options)) return false
  if (options.refresh) {
    if (options.forceRefresh) return false
    if (options.refreshCooldownSeconds <= 0) return false
    return now - cache.fetchedAt <= options.refreshCooldownSeconds * 1000
  }
  return isUsableCacheEntry(cache, options, now)
}

function isRepeatRemoteReadSuppressed(cache, options, now = Date.now()) {
  if (options.allowRepeatRemoteRead) return false
  if (options.minRemoteReadIntervalSeconds <= 0) return false
  if (!isMatchingCacheEntry(cache, options)) return false
  return now - cache.fetchedAt <= options.minRemoteReadIntervalSeconds * 1000
}

function isMatchingCacheEntry(cache, options) {
  if (!cache || cache.version !== CACHE_VERSION || !Array.isArray(cache.pullRequests)) return false
  if (!cache.fetchedAt || !Number.isFinite(cache.fetchedAt)) return false
  return JSON.stringify(cache.query) === JSON.stringify(cacheQuery(options))
}

function cacheQuery(options) {
  return {
    fields: GH_FIELDS,
    limit: options.limit,
    state: options.state,
  }
}

function formatCacheNotice(snapshot) {
  if (snapshot.localOnly) {
    return `[pr-summary] using local-only GitHub snapshot from ${formatAge(
      snapshot.cacheAgeSeconds
    )}; no remote read was made${
      snapshot.localOnlyStale
        ? '; the snapshot is older than the normal reuse window, so run npm run pr:summary:refresh once when a fresh remote read is acceptable'
        : ''
    }`
  }
  if (snapshot.repeatRemoteReadSuppressed) {
    return `[pr-summary] remote refresh skipped because GitHub was checked ${formatAge(
      snapshot.cacheAgeSeconds
    )}; next remote read is allowed in ${formatDuration(
      snapshot.remoteReadGuardRemainingSeconds
    )}; pass --allow-repeat-remote-read only for a one-time manual check`
  }
  if (snapshot.refreshSuppressed) {
    return `[pr-summary] refresh skipped because GitHub was checked ${formatAge(
      snapshot.cacheAgeSeconds
    )}; try again in ${formatDuration(
      snapshot.refreshCooldownRemainingSeconds
    )}; pass --force-refresh only when a new remote read is required`
  }
  return `[pr-summary] using cached GitHub snapshot from ${formatAge(
    snapshot.cacheAgeSeconds
  )}; it expires in ${formatDuration(
    snapshot.cacheTtlRemainingSeconds
  )}; pass --refresh only after a known remote change`
}

function formatFreshSnapshotNotice(options) {
  if (options.noCache) {
    return '[pr-summary] fresh GitHub read completed with --no-cache; no snapshot was saved, so do not use this in loops'
  }

  const guard = formatDuration(options.minRemoteReadIntervalSeconds)
  if (options.cacheTtlSeconds <= 0) {
    return `[pr-summary] fresh GitHub snapshot saved, but cache reuse is disabled; repeat remote reads are still guarded for ${guard}`
  }

  return `[pr-summary] fresh GitHub snapshot saved; use npm run pr:summary:local or cached npm run pr:summary for the next ${formatDuration(
    options.cacheTtlSeconds
  )}; repeat remote reads are blocked for ${guard}`
}

function secondsRemaining(fetchedAt, limitSeconds, now) {
  if (!Number.isFinite(fetchedAt) || !Number.isFinite(limitSeconds) || limitSeconds <= 0) {
    return 0
  }
  return Math.max(0, Math.ceil(limitSeconds - (now - fetchedAt) / 1000))
}

function formatAge(seconds) {
  if (!Number.isFinite(seconds) || seconds <= 0) return 'just now'
  return `${formatDuration(seconds)} ago`
}

function formatDuration(seconds) {
  if (!Number.isFinite(seconds) || seconds <= 0) return 'just now'
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  return `${hours}h`
}

function printHelp() {
  console.info(`Usage: node scripts/pr-status-summary.mjs [options]

Summarize GitHub PRs into low-token ACTION / WAIT / DONE buckets.

Options:
  --limit <n>         Number of PRs to read from GitHub. Default: 120
  --state <state>    open, closed, or all. Default: open
  --refresh          Query GitHub unless the latest snapshot is inside the refresh cooldown
  --force-refresh    Bypass the refresh cooldown, but keep the repeat-read guard
  --monitor          Snapshot-only automation mode; fail only on ACTION and reject refresh bypasses
  --local-only       Read only the local snapshot and never call GitHub
  --no-cache         Query GitHub and do not read or write the local cache; requires --allow-repeat-remote-read
  --allow-repeat-remote-read
                     Permit an immediate uncached or forced GitHub read for a one-time manual check
  --cache-ttl-seconds <n>
                     Reuse a local GitHub snapshot for this many seconds. Default: 900
                     Monitor mode raises the default and minimum to 3600
  --refresh-cooldown-seconds <n>
                     Reuse a very recent snapshot even when --refresh is passed. Default: 60
  --min-remote-read-interval-seconds <n>
                     Minimum seconds between GitHub reads for the same query. Default: 60
  --cache-file <path>
                     Use a custom cache file. Default: Git temp path
  --show-wait        Print each WAIT item instead of only the wait count
  --json             Print structured summary JSON
  --input <file>     Read gh-style PR JSON from a file instead of calling GitHub
  --fail-on-action   Exit 1 when any PR needs action
  -h, --help         Show this help
`)
}

function throwUsageError(message) {
  console.error(`[pr-summary] ERROR: ${message}`)
  process.exit(2)
}

if (process.argv[1]?.endsWith('pr-status-summary.mjs')) {
  main()
}

export {
  CACHE_VERSION,
  cacheQuery,
  classifyPullRequest,
  DEFAULT_MONITOR_CACHE_TTL_SECONDS,
  DEFAULT_MIN_REMOTE_READ_INTERVAL_SECONDS,
  DEFAULT_REFRESH_COOLDOWN_SECONDS,
  formatCacheNotice,
  formatFreshSnapshotNotice,
  getLocalOnlyModeErrors,
  getMonitorSnapshotModeErrors,
  getRemoteReadProtectionErrors,
  isRepeatRemoteReadSuppressed,
  isUsableCacheEntry,
  isReusableCacheEntry,
  parseArgs,
  readPullRequestSnapshot,
}
